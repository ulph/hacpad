//! hacpad USB Bridge prototype -- Nektar Panorama P1
//!
//! Talks to the P1 over named ALSA sequencer ports (via the `midir` crate),
//! matching the real Bitwig driver's two-output-port usage. Two jobs:
//!
//!   1. Decode incoming Control Change messages from its physical controls
//!      (input) using the documented CC map. **Confirmed working** against
//!      real hardware.
//!   2. Write to the P1's own screen via vendor SysEx (output/feedback).
//!      **Confirmed working** against real hardware -- text (including
//!      multi-line ASCII banners, `\n`-separated) renders correctly.
//!
//! The actual protocol implementation (constants, SysEx builders, port
//! helpers) lives in src/lib.rs now, shared with every other binary in this
//! crate (msg_test, led_test, chunk_test, and service -- the persistent
//! WebSocket-facing service the webcam-viewer's screen simulator talks to).
//! This binary keeps only what's specific to it: CC-input decoding and the
//! demo main() loop.
//!
//! Protocol facts (SysEx structure, CC map, port mapping) are written up in
//! research/panorama-p1-protocol-notes.md, cross-checked against Nektar's own
//! official Bitwig driver (`PANORAMA_P1.control.js` / `pnx1.js` / `pnx2.js`)
//! and, for CC input, a community reference
//! (LukeLandry/nektar-panorama-p1-bitwig, no license declared, credited there).
//! This is our own from-scratch implementation of those facts.
//!
//! Usage:
//!     cargo run                    # send init, attempt a screen message, listen for CC input
//!     cargo run -- "some text"     # same, with a custom message

use std::env;
use std::error::Error;
use std::time::Duration;

use midir::{MidiInput, MidiOutput};

// CC-input decoding (CcKind/cc_name/cc_kind/InputEvent/decode_cc) moved into
// lib.rs -- shared with service.rs now, not a copy that lives only here. See
// lib.rs's "Device -> host: control input" section for the full evidence
// comments (unchanged, just relocated).
use panorama_bridge::*;

fn main() -> Result<(), Box<dyn Error>> {
    let text = env::args().skip(1).collect::<Vec<_>>().join(" ");
    let text = if text.is_empty() { "hacpad".to_string() } else { text };

    let default_out = MidiOutput::new("hacpad-p1-default")?;
    let default_port = find_out_port(&default_out, PORT_DEFAULT)?;
    let mut default_conn = default_out.connect(&default_port, "hacpad-p1-default-conn")?;

    let port1_out = MidiOutput::new("hacpad-p1-port1")?;
    let port1_port = find_out_port(&port1_out, PORT_ONE)?;
    let mut port1_conn = port1_out.connect(&port1_port, "hacpad-p1-port1-conn")?;

    // Real driver also opens MIDI input on both ports; CC input in practice
    // arrives on the "Instrument"/default port.
    let input = MidiInput::new("hacpad-p1-input")?;
    let input_port = find_in_port(&input, PORT_DEFAULT)?;

    println!("Sending init sequence...");
    port1_conn.send(&sysex(&INIT_LINUX_ONLY))?;
    std::thread::sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_1))?;
    std::thread::sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_2))?;
    std::thread::sleep(Duration::from_millis(200));

    // NOTE: sending a pageTemplate=1 (write_message) after a pageTemplate=16
    // (write_title_bar) write clears the title bar -- switching pageTemplate
    // resets the whole page context, so these two don't currently coexist in
    // one session (see "Thirteenth finding" in the protocol notes).
    println!("Writing screen message: {text:?}");
    default_conn.send(&write_message(&text))?;

    println!("Listening for CC input (Ctrl-C to stop)...");
    let _in_conn = input.connect(
        &input_port,
        "hacpad-p1-input-conn",
        move |_stamp, msg, _| {
            if msg.len() >= 3 && (0xB0..=0xBF).contains(&msg[0]) {
                println!("{:?}", decode_cc(msg[1], msg[2]));
            }
        },
        (),
    )?;

    // Block forever (Ctrl-C to exit); real cleanup on exit would send
    // EXIT_1/EXIT_2 here, omitted for this simple blocking demo loop.
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
