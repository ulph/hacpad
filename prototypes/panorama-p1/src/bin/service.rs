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
//!    "names": ["...", ...], "values": ["...", ...]}
//! Each field is optional; only the fields present get written.
//!
//! The service also remembers the last update it received (in memory, for
//! as long as this process runs -- the one and only place "current screen
//! state" lives now, replacing the earlier Python-side STATE dict) and
//! immediately sends it to every newly-connected client, so multiple
//! browser tabs (or any other client) stay in sync with each other through
//! the service itself, not through client-side polling of a separate HTTP
//! server. There is no reply to a client's own update beyond this --
//! see "Thirteenth finding" in the protocol notes: the device itself has no
//! read-back, so there's nothing more authoritative to report.
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
use serde::Deserialize;
use tungstenite::Message;

use panorama_bridge::*;

const WS_PORT: u16 = 8091;

#[derive(Deserialize, Default)]
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
        if update.layout == "knobs" && !update.values.is_empty() {
            self.default_conn.send(&write_values(template, &update.values))?;
        }
        if !update.tabs.is_empty() {
            self.default_conn.send(&write_tabs(template, &update.tabs))?;
        }
        Ok(())
    }
}

/// The one place "current screen state" lives, for as long as this process
/// runs -- replaces the earlier Python-side STATE dict, which had no way to
/// know about writes that didn't go through it (e.g. a CLI tool talking to
/// the device directly). Every write from any client goes through here.
type LastState = std::sync::Mutex<Option<String>>;

fn handle_client(stream: std::net::TcpStream, device: &std::sync::Mutex<Device>, last_state: &LastState) {
    let mut socket = match tungstenite::accept(stream) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("WebSocket handshake failed: {e}");
            return;
        }
    };
    println!("client connected");

    // Sync this new client up with whatever the last client (or this one,
    // last time) actually set -- so opening a second tab doesn't start from
    // stale defaults. (Real-time push to *already-connected* tabs when a
    // different tab edits isn't done yet -- each connected socket only reads
    // in this loop, there's no separate writer channel per client for that
    // yet. A reasonable next step, not required for the sync-on-connect case.)
    if let Some(existing) = last_state.lock().unwrap().clone() {
        let _ = socket.send(Message::Text(existing.into()));
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
        *last_state.lock().unwrap() = Some(text.to_string());
    }
    println!("client disconnected");
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("Connecting to Panorama P1...");
    let device = Device::connect()?;
    let device = std::sync::Arc::new(std::sync::Mutex::new(device));
    let last_state: std::sync::Arc<LastState> = std::sync::Arc::new(std::sync::Mutex::new(None));
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
