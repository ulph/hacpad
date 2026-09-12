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

use panorama_bridge::*;

// --- CC map ---------------------------------------------------------------
// Confirmed against real hardware this session (fader/encoder/button CCs
// verified live); transport/nav ordering within their ranges is still an
// unverified guess (the reference only gave the range and named functions,
// not the exact per-CC assignment).

#[derive(Debug)]
enum CcKind {
    Fader,
    Encoder,
    Button,
    Unknown,
}

fn cc_name(cc: u8) -> String {
    match cc {
        0..=7 => format!("fader_{}", cc + 1),
        14 => "fader_master".to_string(),
        16..=23 => format!("select_{}", cc - 16 + 1),
        48..=55 => format!("pan_encoder_{}", cc - 48 + 1),
        64..=71 => format!("param_encoder_{}", cc - 64 + 1),
        // Transport row and friends -- CORRECTED AGAIN (Twenty-ninth finding): the first pass at
        // this (photo-of-finger-position based) turned out to have a one-step lag between the
        // physical press and the log line/photo landing during rapid sequential presses -- the
        // webcam attribution below was consistently off by one position. Re-derived from the
        // actual PANORAMA_P1.control.js CC enum + each case body's real Bitwig API call
        // (transport.play(), .stop(), .record(), .rewind(), .fastForward(), .toggleLoop()), which
        // is authoritative and not subject to that lag at all. Trust this over any earlier photo-
        // based guess for these six.
        80 => "loop".to_string(),    // transport.toggleLoop()
        81 => "rewind".to_string(),  // transport.rewind()
        82 => "forward".to_string(), // transport.fastForward()
        83 => "stop".to_string(),    // transport.stop() / transport.setPosition(0)
        84 => "play".to_string(),    // transport.play()
        85 => "record".to_string(),  // transport.record()
        86 => "loop_in".to_string(), // transport.getInPosition().set(...)
        87 => "loop_out".to_string(), // transport.getOutPosition().set(...)
        // 88, 90: not resolved from source in this pass (case bodies not matched by a simple
        // grep -- may span more of the minified line than searched). 91-95 confirmed to exist as a
        // 5-button nav cluster, tied to zoom/arrow-key/preset-scroll actions (94 confirmed:
        // application.zoomIn() / arrowKeyDown() / preset-scroll depending on Shift and browser
        // state) -- exact per-button identity (which is "up" vs "zoom" etc) not individually
        // distinguished yet. 89 confirmed: transport.toggleClick()/toggleMetronomeTicks() --
        // labeled "click" below, correcting the earlier photo-based "patch_plus" guess.
        89 => "click".to_string(),
        91..=95 => format!("nav_zoom_{}", cc - 91 + 1),
        99 => "f_keys".to_string(), // confirmed live: opens a distinct, DEVICE-NATIVE "F-KEYS" page (F1-F11/P5/P11 grid) -- rendered by the P1 itself, not by anything we (or a DAW driver) send over SysEx; source's handler just does setActiveDisplayPage/gBrowserOpen bookkeeping on the Bitwig-driver side, which isn't even running in our setup. Confirmed momentary (releases when the button is released). The P1 also exposes a genuine USB HID keyboard interface (class 3, standard boot-keyboard report descriptor, separate from MIDI) -- plausibly what "F-Keys" actually drives, but no HID report was captured yet to confirm the link empirically.
        100 => "rewind_bar".to_string(), // positional guess only, not source- or photo-confirmed with confidence
        101 => "forward_bar".to_string(), // positional guess only
        102 => "undo".to_string(),        // positional guess only
        // Confirmed from PANORAMA_P1.control.js's onMidi CC dispatch (Z811481AF53E7994F1),
        // then verified live via the physical device (Twenty-eighth finding):
        96 => "shift".to_string(), // momentary; source sets a boolean gate flag on value>0/0
        97 => "jog_click".to_string(), // fires right alongside the jog wheel's own cc (111) -- likely its push/click function
        103 => "mode".to_string(), // source: setActiveDisplayPage(internalPage) on press
        106 => "menu_button_0".to_string(), // 5th of the "menu buttons" LED range (106-110); not otherwise distinguished from 107-110
        107 => "screen_button_1".to_string(),
        108 => "screen_button_2".to_string(),
        109 => "screen_button_3_exit".to_string(), // confirmed live: closes the popup menu (onMenuCancel) when one is open
        110 => "menu_enter".to_string(), // confirmed live: onMenuEnter when a popup menu is open
        111 => "jog_wheel".to_string(), // confirmed live: relative encoder ticks; also reused as the popup-menu highlight-index CC in the output direction (Twenty-seventh finding)
        _ => format!("cc_{cc}"),
    }
}

fn cc_kind(cc: u8) -> CcKind {
    match cc {
        0..=7 | 14 => CcKind::Fader,
        48..=55 | 64..=71 | 111 => CcKind::Encoder,
        16..=23 | 80..=90 | 91..=103 | 106..=110 => CcKind::Button,
        _ => CcKind::Unknown,
    }
}

#[derive(Debug)]
#[allow(dead_code)] // fields are read via the derived Debug impl in main's print loop
enum Event {
    Fader { cc: u8, name: String, value: u8, normalized: f32 },
    Encoder { cc: u8, name: String, delta: i8 },
    Button { cc: u8, name: String, pressed: bool },
    Unknown { cc: u8, name: String, value: u8 },
}

fn decode_cc(cc: u8, value: u8) -> Event {
    let name = cc_name(cc);
    match cc_kind(cc) {
        CcKind::Fader => Event::Fader {
            cc,
            name,
            value,
            normalized: value as f32 / 127.0,
        },
        CcKind::Encoder => {
            // relative 2's-complement: 1..=63 = +delta, 65..=127 = -delta
            let delta: i8 = if value < 64 {
                value as i8
            } else {
                -(128 - value as i16) as i8
            };
            Event::Encoder { cc, name, delta }
        }
        CcKind::Button => Event::Button {
            cc,
            name,
            pressed: value == 127,
        },
        CcKind::Unknown => Event::Unknown { cc, name, value },
    }
}

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
