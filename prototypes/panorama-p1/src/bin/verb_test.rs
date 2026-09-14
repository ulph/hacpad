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

    let title = TitleBar { title_left: "VERB1".into(), title_center: "VERB2".into(), title_right: "VERB3".into() };

    let bg = match which.as_str() {
        // Pads named by the row letters the device prints down the side.
        "pads" => Background::PadView {
            pad_a1: Pad { name: "A1".into(), state: PadState::Lit },
            pad_a2: Pad { name: "A2".into(), state: PadState::Lit },
            pad_a3: Pad { name: "A3".into(), state: PadState::Default },
            pad_a4: Pad { name: "A4".into(), state: PadState::Default },
            pad_b1: Pad { name: "B1".into(), state: PadState::Default },
            pad_b2: Pad { name: "B2".into(), state: PadState::Default },
            pad_b3: Pad { name: "B3".into(), state: PadState::Default },
            pad_b4: Pad { name: "B4".into(), state: PadState::Default },
            pad_c1: Pad { name: "C1".into(), state: PadState::Default },
            pad_c2: Pad { name: "C2".into(), state: PadState::Default },
            pad_c3: Pad { name: "C3".into(), state: PadState::Default },
            pad_c4: Pad { name: "C4".into(), state: PadState::Default },
            pad_d1: Pad { name: "D1".into(), state: PadState::Default },
            pad_d2: Pad { name: "D2".into(), state: PadState::Default },
            pad_d3: Pad { name: "D3".into(), state: PadState::Default },
            pad_d4: Pad { name: "D4".into(), state: PadState::Lit },
        },
        "launcher" => Background::TransportLauncher {
            loop_left: "L:1.1.1".to_string(),
            loop_right: "R:5.1.1".to_string(),
            button_1: "T1".into(), button_2: "T2".into(), button_3: "T3".into(), button_4: "T4".into(),
            button_5: "T5".into(), button_6: "T6".into(), button_7: "T7".into(), button_8: "T8".into(),
        },
        "mixer" | "overlaytest" => Background::Mixer {
            knob_1: Knob { name: "CUT".into(), value: "1.2k".into() },
            knob_2: Knob { name: "RES".into(), value: "45%".into() },
            knob_3: Knob { name: "ATK".into(), value: "12ms".into() },
            knob_4: Knob { name: "DEC".into(), value: "80ms".into() },
            knob_5: Knob { name: "SUS".into(), value: "0dB".into() },
            knob_6: Knob { name: "REL".into(), value: "200ms".into() },
            knob_7: Knob { name: "DRV".into(), value: "30%".into() },
            knob_8: Knob { name: "MIX".into(), value: "wet".into() },
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
        for msg in state.switch_background(bg, title.clone()) {
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
    for msg in switch_background_messages(&bg, &[title.title_left.clone(), title.title_center.clone(), title.title_right.clone()]) {
        default_conn.send(&msg)?;
        sleep(Duration::from_millis(50));
    }

    println!("Persisting -- holding connection open indefinitely. Ctrl-C to release.");
    loop {
        sleep(Duration::from_secs(3600));
    }
}
