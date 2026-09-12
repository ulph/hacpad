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
//!      multi-line ASCII banners, `\n`-separated) renders correctly. The
//!      missing piece for most of this investigation was the port mapping
//!      (see PORT_DEFAULT/PORT_ONE below and research/panorama-p1-protocol-notes.md,
//!      "Fifth finding"), not the message bytes, which were already correct.
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

use midir::{MidiInput, MidiInputPort, MidiOutput, MidiOutputPort};

// --- SysEx protocol ------------------------------------------------------
// F0 00 01 77 7F 01 ...  F7
//   00 01 77 = Nektar's registered 3-byte MIDI SysEx manufacturer ID
//   7F 01    = device/model + unit byte (unverified, assumed fixed for P1)

const SYSEX_PREFIX: [u8; 6] = [0xF0, 0x00, 0x01, 0x77, 0x7F, 0x01];
const SYSEX_END: u8 = 0xF7;

// Lifecycle messages (bytes after the manufacturer prefix, before F7).
// Confirmed byte-for-byte against the official driver's nektarinit()/nektarexit().
const INIT_LINUX_ONLY: [u8; 7] = [0x08, 0x01, 0x00, 0x00, 0x01, 0x01, 0x75];
const INIT_1: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x01, 0x73];
const INIT_2: [u8; 7] = [0x09, 0x03, 0x00, 0x00, 0x01, 0x3E, 0x34];
// Not sent by this simple demo loop (it blocks forever until Ctrl-C kills the
// process outright, with no clean-shutdown hook) -- kept here, confirmed
// correct, for whenever the bridge grows a real shutdown path.
#[allow(dead_code)]
const EXIT_1: [u8; 7] = [0x09, 0x00, 0x00, 0x00, 0x01, 0x00, 0x75];
#[allow(dead_code)]
const EXIT_2: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x00, 0x74];

const CMD_WRITE_DISPLAY: u8 = 0x06;

// The dedicated one-shot "message" page-template constant, confirmed from
// `PANORAMA_P1.control.js`'s pageTemplate enum (`MESSAGE: 1`) and used by
// `writeMessageToDisplay()`/`OutputState.prototype.send()`'s message branch.
// NOT the 02 we originally guessed before finding the official driver.
const MSG_PAGE_TEMPLATE: u8 = 0x01;

/// Real ports the official driver uses -- CORRECTED per the official "Using
/// Panorama P-Series with Bitwig Studio" guide's Linux port-config table
/// ("Output1: Instrument, Output2: Internal"): the default output port
/// (Bitwig's bare `sendSysex()`, no port arg) is "Instrument"; the one
/// explicit `host.getMidiOutPort(1)` call (used only for the Linux-only init
/// message) is "Internal". This is the REVERSE of what earlier testing this
/// session assumed -- see research/panorama-p1-protocol-notes.md, "Fifth
/// finding" -- and was the actual reason display writes weren't rendering.
const PORT_DEFAULT: &str = "PANORAMA P1 Instrument";
const PORT_ONE: &str = "PANORAMA P1 Internal";

fn sysex(body: &[u8]) -> Vec<u8> {
    let mut msg = SYSEX_PREFIX.to_vec();
    msg.extend_from_slice(body);
    msg.push(SYSEX_END);
    msg
}

/// The "quick message" one-shot display write (`writeMessageToDisplay` in the
/// real driver). Simpler than the full per-field page-composition path and
/// doesn't need live DAW session state to construct. **Confirmed working**:
/// renders on the real screen, including multi-line text via embedded `\n`.
fn write_message(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut body = vec![CMD_WRITE_DISPLAY, MSG_PAGE_TEMPLATE, 0x00, 0x00, bytes.len() as u8];
    body.extend_from_slice(bytes);
    body.push(0x04); // trailing type/terminator byte before F7, per the real driver
    sysex(&body)
}

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
        81..=85 => {
            // unverified order
            ["play", "stop", "record", "rewind", "forward"][(cc - 81) as usize].to_string()
        }
        91..=95 => format!("nav_{}", cc - 91 + 1), // unverified mapping
        _ => format!("cc_{cc}"),
    }
}

fn cc_kind(cc: u8) -> CcKind {
    match cc {
        0..=7 | 14 => CcKind::Fader,
        48..=55 | 64..=71 => CcKind::Encoder,
        16..=23 | 81..=85 | 91..=95 => CcKind::Button,
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

fn find_out_port(out: &MidiOutput, needle: &str) -> Result<MidiOutputPort, Box<dyn Error>> {
    out.ports()
        .into_iter()
        .find(|p| out.port_name(p).map(|n| n.contains(needle)).unwrap_or(false))
        .ok_or_else(|| format!("no MIDI output port matching {needle:?}").into())
}

fn find_in_port(inp: &MidiInput, needle: &str) -> Result<MidiInputPort, Box<dyn Error>> {
    inp.ports()
        .into_iter()
        .find(|p| inp.port_name(p).map(|n| n.contains(needle)).unwrap_or(false))
        .ok_or_else(|| format!("no MIDI input port matching {needle:?}").into())
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
