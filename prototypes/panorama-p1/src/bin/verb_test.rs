//! Exercises the semantic `Background` verb layer (lib.rs) directly against
//! real hardware -- proves the typed schema in
//! research/panorama-p1-widget-model.md actually matches what renders, not
//! just that it compiles. Usage:
//!   cargo run --bin verb_test -- pads       # Background::PadView
//!   cargo run --bin verb_test -- launcher   # Background::TransportLauncher
//!   cargo run --bin verb_test -- mixer      # Background::Mixer
//! Holds the connection open indefinitely (like msg_test --persist) so the
//! result can be photographed without the exit sequence wiping it.

use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

use midir::MidiOutput;
use panorama_bridge::*;

fn main() -> Result<(), Box<dyn Error>> {
    let which = std::env::args().nth(1).unwrap_or_else(|| "pads".to_string());

    let default_out = MidiOutput::new("hacpad-verb-test-default")?;
    let default_port = find_out_port(&default_out, PORT_DEFAULT)?;
    let mut default_conn = default_out.connect(&default_port, "hacpad-verb-test-default-conn")?;

    let port1_out = MidiOutput::new("hacpad-verb-test-port1")?;
    let port1_port = find_out_port(&port1_out, PORT_ONE)?;
    let mut port1_conn = port1_out.connect(&port1_port, "hacpad-verb-test-port1-conn")?;

    println!("Sending init sequence...");
    port1_conn.send(&sysex(&INIT_LINUX_ONLY))?;
    sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_1))?;
    sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_2))?;
    sleep(Duration::from_millis(200));

    let title_bar: [String; 3] = ["VERB1".into(), "VERB2".into(), "VERB3".into()];

    let bg = match which.as_str() {
        "pads" => Background::PadView {
            // One Pad per physical pad -- label and lit state together,
            // instead of two parallel 16-lists aligned by index.
            pads: std::array::from_fn(|i| Pad {
                name: format!("P{}", i + 1),
                state: if i == 0 || i == 1 || i == 15 { PadState::Lit } else { PadState::Default },
            }),
        },
        "launcher" => Background::TransportLauncher {
            loop_left: "L:1.1.1".to_string(),
            loop_right: "R:5.1.1".to_string(),
            buttons: std::array::from_fn(|i| format!("T{}", i + 1)),
        },
        "mixer" | "overlaytest" => {
            const NAMES: [&str; 8] = ["CUT", "RES", "ATK", "DEC", "SUS", "REL", "DRV", "MIX"];
            const VALUES: [&str; 8] = ["1.2k", "45%", "12ms", "80ms", "0dB", "200ms", "30%", "wet"];
            Background::Mixer {
                knobs: std::array::from_fn(|i| Knob {
                    name: NAMES[i].to_string(),
                    value: VALUES[i].to_string(),
                }),
            }
        }
        other => {
            eprintln!("unknown background {other:?}, expected pads|launcher|mixer|overlaytest");
            std::process::exit(1);
        }
    };

    if which == "overlaytest" {
        // Exercises DeviceState directly: background with real content, then
        // popup, then message on TOP of both -- the exact "does this look
        // messy" scenario flagged when designing show/hide semantics.
        let mut state = DeviceState::default();
        println!("1) switch_background(Mixer)...");
        for msg in state.switch_background(bg, title_bar) {
            default_conn.send(&msg)?;
            sleep(Duration::from_millis(50));
        }
        sleep(Duration::from_secs(2));

        println!("2) show_popup...");
        let items = vec!["Item1".into(), "Item2".into(), "Item3".into()];
        for msg in state.show_popup(&items) {
            default_conn.send(&msg)?;
            sleep(Duration::from_millis(50));
        }
        default_conn.send(&state.set_popup_highlight(2))?;
        sleep(Duration::from_secs(2));

        println!("3) show_message (on top of background+popup)...");
        for msg in state.show_message("OVERLAY TEST") {
            default_conn.send(&msg)?;
            sleep(Duration::from_millis(50));
        }
        sleep(Duration::from_secs(10));

        println!("4) hide_message (should restore Mixer background; popup also cleared per the model)...");
        for msg in state.hide_message() {
            default_conn.send(&msg)?;
            sleep(Duration::from_millis(50));
        }

        println!("Persisting -- holding connection open indefinitely. Ctrl-C to release.");
        loop {
            sleep(Duration::from_secs(3600));
        }
    }

    println!("Sending Background::{which} via switch_background_messages()...");
    for msg in switch_background_messages(&bg, &title_bar) {
        default_conn.send(&msg)?;
        sleep(Duration::from_millis(50));
    }

    println!("Persisting -- holding connection open indefinitely. Ctrl-C to release.");
    loop {
        sleep(Duration::from_secs(3600));
    }
}
