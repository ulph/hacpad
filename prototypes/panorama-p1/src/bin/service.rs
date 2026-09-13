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
//! Also decodes the device's own CC input (via lib.rs's shared `InputState`,
//! moved here from what used to be main.rs's own private copy) through a
//! permanent input connection, and hands each newly-connecting client a
//! snapshot of the latest known value per control as
//! `{"inputSnapshot": {"fader_1": {...}, ...}}`, right after the normal
//! screen-state sync. A client can also ask for a fresh one anytime by
//! sending `{"queryInput": true}` -- index.html polls this every few
//! hundred ms so the input panel feels live. This is polling, not a real
//! server push: a true push needs a writer channel per client so the input
//! thread could send unprompted, but `handle_client`'s loop and this
//! process's `tungstenite` version are synchronous -- splitting reads and
//! writes across two threads on the SAME socket risks two independent
//! writers interleaving frame bytes on the wire, a real protocol hazard,
//! not just an inconvenience. Polling sidesteps that entirely by reusing
//! the existing one-request-one-reply loop every other message already
//! goes through.
//!
//! **Two ways to talk to this service, both accepted on the same socket**:
//! the raw `ScreenUpdate` shape above (a direct, uncomposed field write --
//! "raw perspective": you get exactly the SysEx writes you asked for, with
//! no memory of what was shown before, which is also why dismissing an
//! overlay by re-sending empty fields is a confirmed no-op -- see
//! `write_page_menu`/`write_message`'s doc comments in lib.rs), and a
//! `{"semantic": {"cmd": "...", ...}}` envelope carrying one of lib.rs's
//! `DeviceState` verbs (`switchBackground`/`showPopup`/`setPopupHighlight`/
//! `hidePopup`/`showMessage`/`hideMessage`). A semantic command is NOT a
//! different capability, just a convenient way to COMPOSE the same raw
//! writes -- `DeviceState` already knows how to correctly restore whatever
//! Background was really active, which is exactly the SysEx sequence a
//! human would otherwise have to construct by hand via the raw path to get
//! a working dismiss. Every semantic command gets a `{"semanticState": {...
//! DeviceSnapshot}}` reply on the SAME socket right after it's applied, so
//! the sender can immediately show the true post-command state without
//! guessing -- see `DeviceState::snapshot()` in lib.rs.
//!
//! Usage:
//!     cargo run --bin service            # listens on ws://0.0.0.0:8091

use std::error::Error;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
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
    // cursorTrack.getVolume() feedback, split across CC 15/47 -- NOT a simple
    // on/off LED (see lib.rs's cursor_volume_messages doc comment,
    // "Thirty-third finding"). 0-1023, matching the real observer's own scale.
    #[serde(default, rename = "cursorVolume")]
    cursor_volume: Option<u16>,
    // The 4-LED status strip (CC 99-102) -- CONFIRMED a hardware mutex (one
    // register, not 4 bits), see STATUS_LED_CCS/status_led_messages in
    // lib.rs ("Thirty-fifth finding"). `Some(1..=4)` selects that position,
    // `Some(0)` (or any other value) clears all four, `None` means "leave
    // alone". Deliberately its own field, not folded into `ledsOn`.
    #[serde(default, rename = "statusLed")]
    status_led: Option<u8>,
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
        cursor_volume: Some(768), // arbitrary non-zero demo value (0-1023 scale)
        status_led: Some(1), // Status1 -- demo default; this group is a mutex, see status_led_messages
    }
}

/// The `{"semantic": {...}}` envelope's payload -- one `DeviceState` verb per
/// variant. `#[serde(tag = "cmd", rename_all = "camelCase")]` makes the wire
/// shape `{"cmd": "switchBackground", "background": {...}, "titleBar": [...]}`
/// etc., matching the rest of this protocol's camelCase convention.
#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
enum SemanticCommand {
    #[serde(rename_all = "camelCase")]
    SwitchBackground { background: Background, title_bar: Vec<String> },
    ShowPopup { items: Vec<String> },
    #[serde(rename_all = "camelCase")]
    SetPopupHighlight { row: u8 },
    HidePopup,
    ShowMessage { text: String },
    HideMessage,
    SetFooter { tabs: Vec<String> },
    #[serde(rename_all = "camelCase")]
    SetHeaderBigFont { text: String },
    #[serde(rename_all = "camelCase")]
    SetHeaderCurrentValue { text: String },
}

struct Device {
    default_conn: MidiOutputConnection,
    #[allow(dead_code)] // kept open for the session; init already sent through it
    port1_conn: MidiOutputConnection,
    /// Whether the device actually ACKed INIT_2 -- the only read-back this
    /// protocol offers at all (`expected_ack` in lib.rs). Ordinary compose
    /// writes get no reply, so this can only confirm the session got
    /// established at startup, not that every later write landed -- but
    /// that's still a real improvement over assuming it silently worked
    /// (see the widget-model doc's "no error/rejection modeling" gap).
    session_verified: bool,
    /// The semantic model (lib.rs) -- used ONLY by `apply_semantic`, never
    /// touched by the raw `apply()` path below. The two can drift apart from
    /// each other (a raw field write doesn't update this), which is expected:
    /// they're deliberately two independent perspectives on the same device,
    /// not one canonical source of truth forcing the other to stay in sync.
    device_state: DeviceState,
}

impl Device {
    fn connect() -> Result<Self, Box<dyn Error>> {
        let default_out = MidiOutput::new("hacpad-service-default")?;
        let default_port = find_out_port(&default_out, PORT_DEFAULT)?;
        let mut default_conn = default_out.connect(&default_port, "hacpad-service-default-conn")?;

        let port1_out = MidiOutput::new("hacpad-service-port1")?;
        let port1_port = find_out_port(&port1_out, PORT_ONE)?;
        let mut port1_conn = port1_out.connect(&port1_port, "hacpad-service-port1-conn")?;

        // Temporary input connection, just long enough to check for INIT_2's
        // ACK -- dropped right after, since this is a one-shot verification,
        // not ongoing input handling (that's main.rs's job).
        let default_in = MidiInput::new("hacpad-service-verify")?;
        let default_in_port = find_in_port(&default_in, PORT_DEFAULT)?;
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let _verify_conn = default_in.connect(
            &default_in_port,
            "hacpad-service-verify-conn",
            move |_stamp, msg, _| {
                let _ = tx.send(msg.to_vec());
            },
            (),
        )?;

        port1_conn.send(&sysex(&INIT_LINUX_ONLY))?;
        thread::sleep(Duration::from_millis(50));
        default_conn.send(&sysex(&INIT_1))?;
        thread::sleep(Duration::from_millis(50));
        let init2 = sysex(&INIT_2);
        default_conn.send(&init2)?;

        let deadline = std::time::Instant::now() + Duration::from_millis(400);
        let mut session_verified = false;
        while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(remaining) {
                Ok(msg) if is_expected_ack(&init2, &msg) => {
                    session_verified = true;
                    break;
                }
                Ok(_) => continue, // some other unsolicited message, keep waiting for the ack
                Err(_) => break,   // timed out
            }
        }
        if session_verified {
            println!("Session verified: device ACKed INIT_2.");
        } else {
            eprintln!(
                "warning: no ACK received for INIT_2 within 400ms -- session may not be \
                 established (device disconnected, or in a bad state from an earlier \
                 EXIT-shaped write elsewhere -- see \"session-invalidation\" in the widget-model doc)"
            );
        }
        // _verify_conn drops here, releasing the temporary input connection.

        thread::sleep(Duration::from_millis(200));

        Ok(Self { default_conn, port1_conn, session_verified, device_state: DeviceState::default() })
    }

    /// Applies one semantic verb by delegating to `DeviceState` (lib.rs) for
    /// the actual composition, then sending whatever raw messages it hands
    /// back -- this function does no protocol reasoning of its own, it's
    /// purely "call the right DeviceState method, send the bytes it returns."
    fn apply_semantic(&mut self, cmd: SemanticCommand) -> Result<(), Box<dyn Error>> {
        match cmd {
            SemanticCommand::SwitchBackground { background, title_bar } => {
                for msg in self.device_state.switch_background(background, title_bar) {
                    self.default_conn.send(&msg)?;
                }
            }
            SemanticCommand::ShowPopup { items } => {
                for msg in self.device_state.show_popup(&items) {
                    self.default_conn.send(&msg)?;
                }
            }
            SemanticCommand::SetPopupHighlight { row } => {
                let msg = self.device_state.set_popup_highlight(row);
                self.default_conn.send(&msg)?;
            }
            SemanticCommand::HidePopup => {
                for msg in self.device_state.hide_popup() {
                    self.default_conn.send(&msg)?;
                }
            }
            SemanticCommand::ShowMessage { text } => {
                for msg in self.device_state.show_message(&text) {
                    self.default_conn.send(&msg)?;
                }
            }
            SemanticCommand::HideMessage => {
                for msg in self.device_state.hide_message() {
                    self.default_conn.send(&msg)?;
                }
            }
            SemanticCommand::SetFooter { tabs } => {
                self.default_conn.send(&self.device_state.set_footer(tabs))?;
            }
            SemanticCommand::SetHeaderBigFont { text } => {
                self.default_conn.send(&self.device_state.set_header_big_font(text))?;
            }
            SemanticCommand::SetHeaderCurrentValue { text } => {
                self.default_conn.send(&self.device_state.set_header_current_value(text))?;
            }
        }
        Ok(())
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
        if let Some(v) = update.cursor_volume {
            for msg in cursor_volume_messages(v) {
                self.default_conn.send(&msg)?;
            }
        }
        if let Some(pos) = update.status_led {
            for msg in status_led_messages(if pos == 0 { None } else { Some(pos) }) {
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

/// Device -> host: the last known value/event for every semantic input
/// control (`InputState`, see lib.rs -- keyed by `cc_name()`, e.g.
/// `"fader_3"`, `"jog_wheel"`, with encoders tracked as an accumulated
/// 0..=127 position since they have no absolute value of their own on the
/// wire). The mirror image of `LastState` (host -> device). Updated only by
/// the permanent input listener started in `main()`; read by `handle_client`
/// to hand a fresh-connecting client a snapshot of "what does the hardware
/// say right now", same spirit as `LastState`'s screen-state snapshot.
type LastInput = std::sync::Mutex<InputState>;

/// Opens a permanent MIDI input connection and decodes every Control Change
/// via lib.rs's `InputState::observe` (which wraps `decode_cc` and also
/// remembers the result), updating `last_input`. Distinct from
/// `Device::connect()`'s temporary ACK-verification input connection, which
/// sends INIT_2 and drops itself before this runs -- ALSA/midir is fine
/// with the two being sequential, not concurrent, since the temporary one
/// is gone by the time this opens.
fn start_input_listener(
    last_input: std::sync::Arc<LastInput>,
) -> Result<MidiInputConnection<()>, Box<dyn Error>> {
    let input = MidiInput::new("hacpad-service-input")?;
    let input_port = find_in_port(&input, PORT_DEFAULT)?;
    let conn = input.connect(
        &input_port,
        "hacpad-service-input-conn",
        move |_stamp, msg, _| {
            if msg.len() >= 3 && (0xB0..=0xBF).contains(&msg[0]) {
                let event = last_input.lock().unwrap().observe(msg[1], msg[2]);
                println!("input: {event:?}");
            }
        },
        (),
    )?;
    Ok(conn)
}

fn handle_client(
    stream: std::net::TcpStream,
    device: &std::sync::Mutex<Device>,
    last_state: &LastState,
    last_input: &LastInput,
) {
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
    // Device -> host: hand over whatever the physical controls last reported,
    // as its own message so existing clients that don't know this key
    // (index.html doesn't yet) can just ignore it.
    {
        let input_snapshot = serde_json::json!({ "inputSnapshot": *last_input.lock().unwrap() });
        if let Ok(text) = serde_json::to_string(&input_snapshot) {
            let _ = socket.send(Message::Text(text.into()));
        }
    }
    // The semantic model's own view, independent of `last_state` above (see
    // Device.device_state's doc comment: the two are separate perspectives,
    // not reconciled into each other) -- so a client that speaks semantic
    // commands starts from the real current DeviceState, not from nothing.
    {
        let snapshot = device.lock().unwrap().device_state.snapshot();
        if let Ok(text) = serde_json::to_string(&serde_json::json!({ "semanticState": snapshot })) {
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

        // Two shapes accepted on the same socket -- see this file's top doc
        // comment. A `"semantic"` key routes to DeviceState; anything else
        // falls through to the raw, uncomposed ScreenUpdate path unchanged.
        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("bad JSON from client: {e}");
                continue;
            }
        };
        // A third shape: `{"queryInput": true}` asks for a fresh
        // inputSnapshot right now. This is polling, not a real server push
        // (see this file's top doc comment on why -- splitting reads/writes
        // across threads on one sync `tungstenite` socket risks interleaving
        // frames from two writers) -- but it rides the same one-request-one-
        // reply loop every other message already uses, so index.html can
        // just poll this every few hundred ms and get a live-feeling input
        // panel without any protocol risk.
        if parsed.get("queryInput").is_some() {
            let snapshot = serde_json::json!({ "inputSnapshot": *last_input.lock().unwrap() });
            if let Ok(text) = serde_json::to_string(&snapshot) {
                let _ = socket.send(Message::Text(text.into()));
            }
            continue;
        }
        if let Some(semantic) = parsed.get("semantic") {
            let cmd: SemanticCommand = match serde_json::from_value(semantic.clone()) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("bad semantic command JSON: {e}");
                    continue;
                }
            };
            let snapshot = {
                let mut dev = device.lock().unwrap();
                if let Err(e) = dev.apply_semantic(cmd) {
                    eprintln!("failed to write semantic command to device: {e}");
                    continue;
                }
                dev.device_state.snapshot()
            };
            // Echo the resulting state back on THIS socket right away -- not
            // a broadcast to other clients (no writer channel for that yet,
            // same limitation as the raw path), but enough for the sender's
            // own UI to reflect ground truth instead of assuming its request
            // landed the way it hoped.
            if let Ok(text) = serde_json::to_string(&serde_json::json!({ "semanticState": snapshot })) {
                let _ = socket.send(Message::Text(text.into()));
            }
            continue;
        }

        let update: ScreenUpdate = match serde_json::from_value(parsed.clone()) {
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
        if let serde_json::Value::Object(incoming) = parsed {
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
    if !device.session_verified {
        eprintln!(
            "Continuing anyway -- writes will be sent, but they may be silently ignored by \
             the device until it's power-cycled or otherwise recovers. Restarting this \
             process (which re-sends INIT) is the current recovery mechanism."
        );
    }

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

    // Seed the semantic model (device_state) to match what was JUST written
    // above -- otherwise it starts believing last_background is None, and
    // hide_popup()/hide_message() silently send nothing the first time
    // they're used (reported bug: "hiding popup does not really work").
    // Mirrors default_state()'s own knobs/N1-8/V1-8/TB1-3 content (Mixer
    // only renders 8 of the 16 name/value slots that get written -- the
    // other 8 are for FaderSplit's 16-slot layout, not visible here) and its
    // "hacpad" message, which really is showing on top of it (see the
    // comment above about not writing the popup at boot -- the message IS
    // written, unconditionally).
    device.device_state.seed_background(
        Background::Mixer {
            param_names: std::array::from_fn(|i| initial.names[i].clone()),
            param_values: std::array::from_fn(|i| initial.values[i].clone()),
        },
        initial.title_bar.clone(),
    );
    device.device_state.seed_message_shown(&initial.message);

    let initial_map = match serde_json::to_value(&initial)? {
        serde_json::Value::Object(m) => m,
        _ => unreachable!("ScreenUpdate always serializes to a JSON object"),
    };

    let device = std::sync::Arc::new(std::sync::Mutex::new(device));
    let last_state: std::sync::Arc<LastState> = std::sync::Arc::new(std::sync::Mutex::new(initial_map));
    let last_input: std::sync::Arc<LastInput> = std::sync::Arc::new(std::sync::Mutex::new(InputState::default()));

    // Kept alive for the rest of main()'s life (never read again after this
    // point) -- dropping it would silently stop delivering input.
    let _input_conn = match start_input_listener(std::sync::Arc::clone(&last_input)) {
        Ok(conn) => Some(conn),
        Err(e) => {
            eprintln!("warning: could not start input listener ({e}); device -> host values won't be tracked");
            None
        }
    };

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
        let last_input = std::sync::Arc::clone(&last_input);
        thread::spawn(move || handle_client(stream, &device, &last_state, &last_input));
    }
    Ok(())
}
