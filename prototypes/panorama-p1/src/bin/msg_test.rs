//! Test the real Nektar driver's init sequence + the "quick message" display
//! write, confirmed from the official PANORAMA_P1.control.js
//! (nektarinit / nektarexit / OutputState.prototype.send / writeMessageToDisplay).
//!
//! Port mapping -- CORRECTED per the official "Using Panorama P-Series with
//! Bitwig Studio" guide's Linux manual-add instructions (port config table,
//! page 6): "Output1: Instrument, Output2: Internal". Bitwig's Controller
//! Settings UI numbers ports 1-based matching getMidiOutPort(0)/(1), so:
//!   "PANORAMA P1 Instrument" == Bitwig DEFAULT port (port 0, bare sendSysex())
//!   "PANORAMA P1 Internal"   == Bitwig host.getMidiOutPort(1)
//! This is the REVERSE of what we assumed all last session (we had these two
//! swapped) -- see research/panorama-p1-protocol-notes.md, "Fifth finding".
//!
//! IMPORTANT: per that same finding, none of this matters while the device is
//! physically in **Internal mode** (one of its four Mode-button states) --
//! that mode explicitly bypasses the whole DAW display protocol by design.
//! Leave Internal mode (press the physical Mode button) before expecting any
//! effect from this test at all.
//!
//! Real init sequence (from nektarinit()):
//!   1. (Linux only) on host.getMidiOutPort(1) ("Internal"): F0 00 01 77 7F 01 08 01 00 00 01 01 75 F7
//!   2. on the default port ("Instrument"):                 F0 00 01 77 7F 01 08 02 00 00 01 01 73 F7
//!   3. on the default port ("Instrument"):                 F0 00 01 77 7F 01 09 03 00 00 01 3E 34 F7
//!      (SURFACE.Connected branch, since the "is P1 mk2 / nektarine" flag is false)
//!
//! "Quick message" write (writeMessageToDisplay, pageTemplate constant == 1):
//!   F0 00 01 77 7F 01 06 01 00 00 <len> <ascii bytes> 04 F7

use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

use midir::{MidiInput, MidiInputPort, MidiOutput, MidiOutputPort};

const PREFIX: [u8; 6] = [0xF0, 0x00, 0x01, 0x77, 0x7F, 0x01];

const INIT_LINUX_ONLY: [u8; 7] = [0x08, 0x01, 0x00, 0x00, 0x01, 0x01, 0x75];
const INIT_1: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x01, 0x73];
const INIT_2: [u8; 7] = [0x09, 0x03, 0x00, 0x00, 0x01, 0x3E, 0x34];
const EXIT_1: [u8; 7] = [0x09, 0x00, 0x00, 0x00, 0x01, 0x00, 0x75];
const EXIT_2: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x00, 0x74];

const CMD_WRITE_DISPLAY: u8 = 0x06;
const MSG_PAGE_TEMPLATE: u8 = 0x01;

const PORT_DEFAULT: &str = "PANORAMA P1 Instrument"; // Bitwig's implicit port 0
const PORT_ONE: &str = "PANORAMA P1 Internal"; // Bitwig's host.getMidiOutPort(1)

fn sysex(body: &[u8]) -> Vec<u8> {
    let mut msg = PREFIX.to_vec();
    msg.extend_from_slice(body);
    msg.push(0xF7);
    msg
}

fn write_message(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut body = vec![CMD_WRITE_DISPLAY, MSG_PAGE_TEMPLATE, 0x00, 0x00, bytes.len() as u8];
    body.extend_from_slice(bytes);
    body.push(0x04);
    sysex(&body)
}

fn find_port(out: &MidiOutput, needle: &str) -> Result<MidiOutputPort, Box<dyn Error>> {
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

/// Parse a space-separated hex-byte string like "F0 00 01 77 ... F7" into raw bytes.
fn parse_hex(s: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    s.split_whitespace()
        .map(|tok| u8::from_str_radix(tok, 16).map_err(|e| e.into()))
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `msg_test --raw "F0 00 ... F7"` sends a literal hex-byte SysEx message
    // (after the same init sequence) instead of building a "quick message" write.
    // Used to replay known example messages verbatim.
    let raw_override: Option<Vec<u8>> = if args.first().map(|s| s.as_str()) == Some("--raw") {
        Some(parse_hex(&args[1..].join(" "))?)
    } else {
        None
    };

    let text = args.join(" ");
    let text = if text.is_empty() { "HACPAD".to_string() } else { text };

    let default_out = MidiOutput::new("hacpad-default")?;
    let default_port = find_port(&default_out, PORT_DEFAULT)?;
    let mut default_conn = default_out.connect(&default_port, "hacpad-default-conn")?;

    let port1_out = MidiOutput::new("hacpad-port1")?;
    let port1_port = find_port(&port1_out, PORT_ONE)?;
    let mut port1_conn = port1_out.connect(&port1_port, "hacpad-port1-conn")?;

    // The real driver's nektarinit() also opens MIDI INPUT connections on both
    // ports (host.getMidiInPort(0)/(1).setMidiCallback(...)). Kept for parity,
    // even though it was already confirmed to make no difference on its own.
    let default_in = MidiInput::new("hacpad-default-in")?;
    let default_in_port = find_in_port(&default_in, PORT_DEFAULT)?;
    let _default_in_conn = default_in.connect(
        &default_in_port,
        "hacpad-default-in-conn",
        |_stamp, msg, _| println!("<< default IN: {msg:02X?}"),
        (),
    )?;

    let port1_in = MidiInput::new("hacpad-port1-in")?;
    let port1_in_port = find_in_port(&port1_in, PORT_ONE)?;
    let _port1_in_conn = port1_in.connect(
        &port1_in_port,
        "hacpad-port1-in-conn",
        |_stamp, msg, _| println!("<< port1 IN: {msg:02X?}"),
        (),
    )?;

    println!("Sending real init sequence (corrected port mapping, input ports also open)...");
    port1_conn.send(&sysex(&INIT_LINUX_ONLY))?;
    sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_1))?;
    sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_2))?;
    sleep(Duration::from_millis(200));

    let msg = match &raw_override {
        Some(raw) => {
            println!("Sending raw override message ({} bytes)", raw.len());
            raw.clone()
        }
        None => {
            println!("Sending quick message: {text:?}");
            write_message(&text)
        }
    };
    print!("bytes:");
    for b in &msg {
        print!(" {b:02X}");
    }
    println!();

    // A single send is sufficient and persists on screen (confirmed once the
    // port mapping was fixed) -- resending on a timer was only ever a test
    // for the "does it need a refreshed write" hypothesis (disproved), and
    // just causes a visible redraw flicker for no benefit. Send once, hold
    // quietly.
    default_conn.send(&msg)?;
    println!("Holding for 8s so we can photograph the screen...");
    sleep(Duration::from_secs(8));

    println!("Sending exit sequence...");
    default_conn.send(&sysex(&EXIT_1))?;
    sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&EXIT_2))?;

    Ok(())
}
