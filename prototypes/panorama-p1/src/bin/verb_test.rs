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

    let title_bar = vec!["VERB1".to_string(), "VERB2".to_string(), "VERB3".to_string()];

    let bg = match which.as_str() {
        "pads" => Background::PadView {
            pad_names: [
                "P1".into(), "P2".into(), "P3".into(), "P4".into(),
                "P5".into(), "P6".into(), "P7".into(), "P8".into(),
                "P9".into(), "P10".into(), "P11".into(), "P12".into(),
                "P13".into(), "P14".into(), "P15".into(), "P16".into(),
            ],
            pad_states: [
                PadState::Lit, PadState::Lit, PadState::Default, PadState::Default,
                PadState::Default, PadState::Default, PadState::Default, PadState::Default,
                PadState::Default, PadState::Default, PadState::Default, PadState::Default,
                PadState::Default, PadState::Default, PadState::Default, PadState::Lit,
            ],
        },
        "launcher" => Background::TransportLauncher {
            loop_left: "L:1.1.1".to_string(),
            loop_right: "R:5.1.1".to_string(),
            labels: [
                "T1".into(), "T2".into(), "T3".into(), "T4".into(),
                "T5".into(), "T6".into(), "T7".into(), "T8".into(),
            ],
        },
        "mixer" | "overlaytest" => Background::Mixer {
            param_names: [
                "CUT".into(), "RES".into(), "ATK".into(), "DEC".into(),
                "SUS".into(), "REL".into(), "DRV".into(), "MIX".into(),
            ],
            param_values: [
                "1.2k".into(), "45%".into(), "12ms".into(), "80ms".into(),
                "0dB".into(), "200ms".into(), "30%".into(), "wet".into(),
            ],
        },
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
        default_conn.send(&state.show_popup(&items))?;
        sleep(Duration::from_millis(50));
        default_conn.send(&state.set_popup_highlight(2))?;
        sleep(Duration::from_secs(2));

        println!("3) show_message (on top of background+popup)...");
        default_conn.send(&state.show_message("OVERLAY TEST"))?;
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
