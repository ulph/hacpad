//! Minimal persistent hacpad service for the Panorama P1: holds the one
//! real connection to the device (via the confirmed protocol in
//! prototypes/panorama-p1/src/lib.rs) and exposes it over a plain WebSocket,
//! so the webcam-viewer's screen simulator (or anything else) can push live
//! screen updates without holding its own separate connection to the
//! hardware. This is the "one canonical thing that talks to the device"
//! the earlier Python-direct-MIDI approach was missing.
//!
//! Protocol (client -> service): one JSON text message per WebSocket frame,
//! shaped like:
//!   {"layout": "knobs", "bigfont": "...", "titleBar": ["","",""],
//!    "names": ["...", ...], "values": ["...", ...], "tabs": ["...", ...],
//!    "currentValue": "...", "pageLabels": ["...", ...],
//!    "padState": [0,1,0,...], "menuItems": ["...", ...], "menuHighlight": 3,
//!    "message": "...", "ledsOn": [16,84,...]}
//! Each field is optional; only the fields present get written. `layout`
//! only ever selects which real page_template the *content* fields
//! (bigfont/titleBar/names/values/tabs/pageLabels/padState) are written
//! against; `menuItems`/`menuHighlight` (the displayId-8 popup), `message`
//! (the pageTemplate=1 one-shot overlay), and `ledsOn` (plain CC feedback)
//! all ignore it entirely -- see lib.rs's write_page_menu/write_message/
//! led_cc_messages doc comments.
//!
//! The service also remembers the last update it received (in memory, for
//! as long as this process runs -- the one and only place "current screen
//! state" lives now, replacing the earlier Python-side STATE dict) and
//! immediately sends it to every newly-connected client, so multiple
//! browser tabs (or any other client) stay in sync with each other through
//! the service itself, not through client-side polling of a separate HTTP
//! server, and not through any hardcoded fallback of the client's own --
//! index.html ships with every field blank and waits for this. Before any
//! client has ever pushed anything, that shared state is `default_state()`
//! below (applied to the real device too, at startup) rather than nothing,
//! so a lone first tab still sees every field populated. There is no reply
//! to a client's own update beyond this -- see "Thirteenth finding" in the
//! protocol notes: the device itself has no read-back, so there's nothing
//! more authoritative to report.
//!
//! Only handles output (screen writes) for now; relaying the device's own
//! CC input back over the same WebSocket is a natural next step, not yet
//! implemented -- main.rs already does CC decoding, this binary doesn't
//! duplicate that yet.
//!
//! Usage:
//!     cargo run --bin service            # listens on ws://0.0.0.0:8091

use std::error::Error;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use midir::{MidiOutput, MidiOutputConnection};
use serde::{Deserialize, Serialize};
use tungstenite::Message;

use panorama_bridge::*;

const WS_PORT: u16 = 8091;

#[derive(Deserialize, Serialize, Default, Clone)]
struct ScreenUpdate {
    #[serde(default)]
    layout: String,
    #[serde(default)]
    bigfont: String,
    #[serde(default, rename = "titleBar")]
    title_bar: Vec<String>,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    values: Vec<String>,
    #[serde(default)]
    tabs: Vec<String>,
    // Fields added so the simulator can show every displayId independently
    // of the `layout` (template) choice above -- see lib.rs's write_current_value
    // /write_page_labels/write_pad_state/write_page_menu doc comments for the
    // hardware evidence behind each.
    #[serde(default, rename = "currentValue")]
    current_value: String,
    #[serde(default, rename = "pageLabels")]
    page_labels: Vec<String>,
    #[serde(default, rename = "padState")]
    pad_state: Vec<u8>,
    #[serde(default, rename = "menuItems")]
    menu_items: Vec<String>,
    #[serde(default, rename = "menuHighlight")]
    menu_highlight: Option<u8>,
    // The pageTemplate=1 one-shot "message" overlay (writeMessageToDisplay) --
    // a distinct rendering pathway from the compose fields above, see
    // write_message's doc comment in lib.rs and the "hacpad" baseline note in
    // the protocol notes. Deliberately its own field, not folded into
    // `bigfont`/the compose path.
    #[serde(default)]
    message: String,
    // Plain CC feedback (see lib.rs's LED_CCS/led_cc_messages) -- unrelated to
    // page_template entirely, unlike every other field above. `None` means
    // "field absent, leave LEDs alone"; `Some(vec![])` means "all off" --
    // Option is required here (unlike the Vec-default fields above) so an
    // explicit "turn everything off" is distinguishable from "not mentioned".
    #[serde(default, rename = "ledsOn")]
    leds_on: Option<Vec<u8>>,
}

/// The state a freshly-started service applies to the real device and hands
/// to the first client to connect, before anyone has pushed anything of
/// their own -- every field populated with something so the simulator (and
/// the real screen) shows every displayId at once, self-descriptively named
/// per slot (matches index.html having no hardcoded defaults of its own any
/// more: this is the one shared, bidirectional source of truth for "current
/// state", same as any other client's update).
fn default_state() -> ScreenUpdate {
    ScreenUpdate {
        layout: "knobs".to_string(),
        bigfont: "BIGFONT".to_string(),
        title_bar: vec!["TB1", "TB2", "TB3"].into_iter().map(String::from).collect(),
        names: (1..=16).map(|i| format!("N{i}")).collect(),
        values: (1..=16).map(|i| format!("V{i}")).collect(),
        tabs: (1..=5).map(|i| format!("Tab{i}")).collect(),
        current_value: "PARAMVAL".to_string(),
        page_labels: (1..=3).map(|i| format!("PL{i}")).collect(),
        pad_state: { let mut v = vec![0u8; 16]; v[0] = 1; v[1] = 1; v },
        menu_items: (1..=8).map(|i| format!("Item{i}")).collect(),
        menu_highlight: Some(3),
        message: "hacpad".to_string(), // the established resting-baseline text (see the "hacpad" note in the protocol notes)
        leds_on: Some(vec![16, 18, 20, 22, 106, 108, 110, 80, 85]), // arbitrary mix so both on/off states show
    }
}

struct Device {
    default_conn: MidiOutputConnection,
    #[allow(dead_code)] // kept open for the session; init already sent through it
    port1_conn: MidiOutputConnection,
}

impl Device {
    fn connect() -> Result<Self, Box<dyn Error>> {
        let default_out = MidiOutput::new("hacpad-service-default")?;
        let default_port = find_out_port(&default_out, PORT_DEFAULT)?;
        let mut default_conn = default_out.connect(&default_port, "hacpad-service-default-conn")?;

        let port1_out = MidiOutput::new("hacpad-service-port1")?;
        let port1_port = find_out_port(&port1_out, PORT_ONE)?;
        let mut port1_conn = port1_out.connect(&port1_port, "hacpad-service-port1-conn")?;

        port1_conn.send(&sysex(&INIT_LINUX_ONLY))?;
        thread::sleep(Duration::from_millis(50));
        default_conn.send(&sysex(&INIT_1))?;
        thread::sleep(Duration::from_millis(50));
        default_conn.send(&sysex(&INIT_2))?;
        thread::sleep(Duration::from_millis(200));

        Ok(Self { default_conn, port1_conn })
    }

    fn apply(&mut self, update: &ScreenUpdate) -> Result<(), Box<dyn Error>> {
        let template = layout_template(&update.layout);
        if !update.bigfont.is_empty() {
            self.default_conn.send(&write_bigfont(template, &update.bigfont))?;
        }
        if !update.title_bar.is_empty() {
            self.default_conn.send(&write_title_bar(template, &update.title_bar))?;
        }
        if !update.names.is_empty() {
            self.default_conn.send(&write_names(template, &update.names))?;
        }
        if !update.values.is_empty() {
            self.default_conn.send(&write_values(template, &update.values))?;
        }
        if !update.tabs.is_empty() {
            self.default_conn.send(&write_tabs(template, &update.tabs))?;
        }
        if !update.current_value.is_empty() {
            self.default_conn.send(&write_current_value(template, &update.current_value))?;
        }
        if !update.page_labels.is_empty() {
            self.default_conn.send(&write_page_labels(template, &update.page_labels))?;
        }
        if !update.pad_state.is_empty() {
            self.default_conn.send(&write_pad_state(template, &update.pad_state))?;
        }
        if !update.menu_items.is_empty() {
            self.default_conn.send(&write_page_menu(&update.menu_items))?;
        }
        if let Some(row) = update.menu_highlight {
            self.default_conn.send(&cc_message(CC_MENU_HIGHLIGHT, row))?;
        }
        if !update.message.is_empty() {
            self.default_conn.send(&write_message(&update.message))?;
        }
        if let Some(on) = &update.leds_on {
            for msg in led_cc_messages(on) {
                self.default_conn.send(&msg)?;
                thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(())
    }
}

/// The one place "current screen state" lives, for as long as this process
/// runs -- replaces the earlier Python-side STATE dict, which had no way to
/// know about writes that didn't go through it (e.g. a CLI tool talking to
/// the device directly). Every write from any client goes through here.
///
/// A JSON *object* that gets MERGED into (top-level keys only), not replaced
/// wholesale by each incoming message -- the diffing client (index.html)
/// deliberately sends one small message per changed field (each still
/// carrying `layout`), so treating the raw text of "the last message" as
/// the whole state (the original, buggy behavior here) meant a new client
/// only ever saw whichever single field happened to be sent most recently,
/// with every other field silently missing. Merging keeps all of them.
type LastState = std::sync::Mutex<serde_json::Map<String, serde_json::Value>>;

fn handle_client(stream: std::net::TcpStream, device: &std::sync::Mutex<Device>, last_state: &LastState) {
    let mut socket = match tungstenite::accept(stream) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("WebSocket handshake failed: {e}");
            return;
        }
    };
    println!("client connected");

    // Sync this new client up with the full merged state -- so opening a
    // second tab doesn't start from stale/incomplete defaults. (Real-time
    // push to *already-connected* tabs when a different tab edits isn't done
    // yet -- each connected socket only reads in this loop, there's no
    // separate writer channel per client for that yet. A reasonable next
    // step, not required for the sync-on-connect case.)
    {
        let snapshot = serde_json::Value::Object(last_state.lock().unwrap().clone());
        if let Ok(text) = serde_json::to_string(&snapshot) {
            let _ = socket.send(Message::Text(text.into()));
        }
    }

    loop {
        let msg = match socket.read() {
            Ok(m) => m,
            Err(_) => break, // client disconnected
        };
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let update: ScreenUpdate = match serde_json::from_str(&text) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("bad screen-update JSON: {e}");
                continue;
            }
        };
        {
            let mut dev = device.lock().unwrap();
            if let Err(e) = dev.apply(&update) {
                eprintln!("failed to write to device: {e}");
                continue;
            }
        }
        if let Ok(serde_json::Value::Object(incoming)) = serde_json::from_str::<serde_json::Value>(&text) {
            let mut state = last_state.lock().unwrap();
            for (k, v) in incoming {
                state.insert(k, v);
            }
        }
    }
    println!("client disconnected");
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("Connecting to Panorama P1...");
    let mut device = Device::connect()?;

    // Apply the exhaustive default state to the real device up front, and
    // seed `last_state` with it -- so the very first client to connect (no
    // one has pushed anything yet) is handed this instead of nothing, and
    // the physical screen already matches it. See default_state()'s doc
    // comment: this is the shared, bidirectional source of truth, not a
    // client-side fallback baked into index.html.
    let initial = default_state();

    // Cross-checked against the real screen (webcam): applying `initial`
    // verbatim -- including its popup-menu demo content (menu_items/
    // menu_highlight) -- left the popup's highlighted-row bars visibly
    // overlaid ON TOP of the "hacpad" message text, instead of a clean
    // message-mode takeover. The popup (displayId 8, hardcoded
    // page_template=0) is confirmed dismissed only by a genuine switch to a
    // different real page_template ("Twenty-seventh finding"); the
    // pageTemplate=1 message write is a separate pathway that evidently
    // does NOT do that, so it doesn't clear a popup opened earlier in the
    // same apply() sequence. Don't actually write the popup fields to the
    // real device as part of the default boot state (avoids the visible
    // conflict) -- but keep them in the shared/client-facing JSON below so
    // the simulator still offers that demo content to explicitly try.
    let mut initial_for_device = initial.clone();
    initial_for_device.menu_items = Vec::new();
    initial_for_device.menu_highlight = None;
    if let Err(e) = device.apply(&initial_for_device) {
        eprintln!("warning: failed to apply default state to device: {e}");
    }
    let initial_map = match serde_json::to_value(&initial)? {
        serde_json::Value::Object(m) => m,
        _ => unreachable!("ScreenUpdate always serializes to a JSON object"),
    };

    let device = std::sync::Arc::new(std::sync::Mutex::new(device));
    let last_state: std::sync::Arc<LastState> = std::sync::Arc::new(std::sync::Mutex::new(initial_map));
    println!("Connected. Listening on ws://0.0.0.0:{WS_PORT}");

    let listener = TcpListener::bind(("0.0.0.0", WS_PORT))?;
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("connection error: {e}");
                continue;
            }
        };
        let device = std::sync::Arc::clone(&device);
        let last_state = std::sync::Arc::clone(&last_state);
        thread::spawn(move || handle_client(stream, &device, &last_state));
    }
    Ok(())
}
