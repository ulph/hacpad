//! Test the real Nektar driver's init sequence + the "quick message" display
//! write, confirmed from the official PANORAMA_P1.control.js
//! (nektarinit / nektarexit / OutputState.prototype.send / writeMessageToDisplay).
//!
//! Port mapping (confirmed via ALSA sequencer port listing):
//!   "PANORAMA P1 Internal"   == Bitwig default port (port 0)
//!   "PANORAMA P1 Instrument" == Bitwig host.getMidiOutPort(1)
//!
//! Real init sequence (from nektarinit()):
//!   1. (Linux only) on Instrument port: F0 00 01 77 7F 01 08 01 00 00 01 01 75 F7
//!   2. on Internal port:                F0 00 01 77 7F 01 08 02 00 00 01 01 73 F7
//!   3. on Internal port:                F0 00 01 77 7F 01 09 03 00 00 01 3E 34 F7
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

    let internal_out = MidiOutput::new("hacpad-internal")?;
    let internal_port = find_port(&internal_out, "PANORAMA P1 Internal")?;
    let mut internal = internal_out.connect(&internal_port, "hacpad-internal-conn")?;

    let instrument_out = MidiOutput::new("hacpad-instrument")?;
    let instrument_port = find_port(&instrument_out, "PANORAMA P1 Instrument")?;
    let mut instrument = instrument_out.connect(&instrument_port, "hacpad-instrument-conn")?;

    // The real driver's nektarinit() also opens MIDI INPUT connections on both
    // ports (host.getMidiInPort(0)/(1).setMidiCallback(...)). We've never done
    // this before -- test whether the device gates its behavior on seeing a
    // live input subscription, i.e. "is a computer actually listening back".
    let internal_in = MidiInput::new("hacpad-internal-in")?;
    let internal_in_port = find_in_port(&internal_in, "PANORAMA P1 Internal")?;
    let _internal_in_conn = internal_in.connect(
        &internal_in_port,
        "hacpad-internal-in-conn",
        |_stamp, msg, _| println!("<< Internal IN: {msg:02X?}"),
        (),
    )?;

    let instrument_in = MidiInput::new("hacpad-instrument-in")?;
    let instrument_in_port = find_in_port(&instrument_in, "PANORAMA P1 Instrument")?;
    let _instrument_in_conn = instrument_in.connect(
        &instrument_in_port,
        "hacpad-instrument-in-conn",
        |_stamp, msg, _| println!("<< Instrument IN: {msg:02X?}"),
        (),
    )?;

    println!("Sending real init sequence (with input ports also open)...");
    instrument.send(&sysex(&INIT_LINUX_ONLY))?;
    sleep(Duration::from_millis(50));
    internal.send(&sysex(&INIT_1))?;
    sleep(Duration::from_millis(50));
    internal.send(&sysex(&INIT_2))?;
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
    internal.send(&msg)?;

    println!("Holding for 8s so we can photograph the screen...");
    sleep(Duration::from_secs(8));

    println!("Sending exit sequence...");
    internal.send(&sysex(&EXIT_1))?;
    sleep(Duration::from_millis(50));
    internal.send(&sysex(&EXIT_2))?;

    Ok(())
}
