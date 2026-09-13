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
        89 => "click".to_string(), // transport.toggleClick()/toggleMetronomeTicks()
        // 88, 90, 93-95, 97-98, 100-102, 104-105: resolved from a fuller re-grep of PANORAMA_P1.control.js
        // ("Thirty-third finding" in the protocol notes) -- the earlier pass's case-body grep missed these
        // because they span more of the minified line than that grep searched.
        88 => "undo_redo".to_string(), // Shift-gated: shift=application.redo(), plain=application.undo()
        90 => "overdub".to_string(), // Shift-gated: shift=transport.toggleWriteArrangerAutomation() ("Automation:"), plain=transport.toggleOverdub() ("Overdub:")
        // 93/94: both literally share ONE case body (`case CC.93: case CC.94: PATCH_PRESSED=0<e`) --
        // a fallthrough that just sets a shared "patch browsing" gate flag, not two separately-handled
        // buttons at this switch. Labeled patch_minus/patch_plus from the physical button row
        // (Shift/Track-/Track+/Patch-/Patch+/View, confirmed live via a webcam photo showing the
        // printed labels) -- matches an EARLIER, separate finding that attributed CC 94 to
        // application.zoomIn()/arrowKeyDown()/preset-scroll depending on Shift/browser state (that
        // logic lives elsewhere, likely reading PATCH_PRESSED alongside encoder direction, not at this
        // exact dispatch site) -- the two findings aren't fully reconciled yet, treat the exact
        // patch_minus-vs-patch_plus split as tentative.
        93 => "patch_minus".to_string(),
        94 => "patch_plus".to_string(),
        95 => "view".to_string(), // onView(), or (unshifted, some states) sends the 0x0B "Launcher" SysEx family documented elsewhere in this file
        91 | 92 => format!("nav_{}", cc - 91 + 1), // still not individually resolved from source
        99 => "f_keys".to_string(), // confirmed live: opens a distinct, DEVICE-NATIVE "F-KEYS" page (F1-F11/P5/P11 grid) -- rendered by the P1 itself, not by anything we (or a DAW driver) send over SysEx; source's handler just does setActiveDisplayPage/gBrowserOpen bookkeeping on the Bitwig-driver side, which isn't even running in our setup. Confirmed momentary (releases when the button is released). The P1 also exposes a genuine USB HID keyboard interface (class 3, standard boot-keyboard report descriptor, separate from MIDI) -- plausibly what "F-Keys" actually drives, but no HID report was captured yet to confirm the link empirically.
        // 100-102, 104: all resolved as browser/patch-menu "cancel"-shaped handlers (gBrowserOpen=false,
        // setActiveDisplayPage/SurfaceStatus changes) but not individually distinguished as specific
        // physical buttons yet -- kept generic and source-quoted rather than over-claiming a name.
        100 => "browser_cancel_1".to_string(), // gBrowserOpen=false; shift: application.createInstrumentTrack(-1)
        101 => "browser_cancel_2".to_string(), // gBrowserOpen=false; setActiveDisplayPage + nek_set_nektarine_instance_active(0)
        102 => "browser_cancel_3".to_string(), // gBrowserOpen=false; softTakeoverReset(); setActiveDisplayPage(internalPage)
        104 => "surface_status".to_string(), // SurfaceStatus/SURFACE.connected-state related -- plausibly not a normal user button, not yet confirmed live
        105 => "automation_write".to_string(), // transport.toggleWriteArrangerAutomation(), unconditional (unlike CC 90's Shift-gated version) -- likely the button whose LED is CC 29
        // Confirmed from PANORAMA_P1.control.js's onMidi CC dispatch (Z811481AF53E7994F1),
        // then verified live via the physical device (Twenty-eighth finding):
        96 => "shift".to_string(), // momentary; source sets a boolean gate flag on value>0/0
        // 97: CORRECTED -- source is `case CC.97: TOGGLE_MUTE_PRESSED=0<e` (a mode-gate flag), NOT the
        // jog wheel's push/click as previously guessed. Distinct from CC 30, which directly toggles
        // cursorTrack's mute AND drives its own LED -- CC 97 is plausibly a pad/drum-mode mute-select
        // button instead, not yet confirmed live which physical control this is.
        97 => "toggle_mute_pressed".to_string(),
        98 => "toggle_view_pressed".to_string(), // source: TOGGLE_VIEW_PRESSED=0<e; onToggleView() on press
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
