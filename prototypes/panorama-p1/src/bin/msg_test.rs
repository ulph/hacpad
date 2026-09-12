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

use midir::{MidiOutput, MidiOutputPort};

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

fn main() -> Result<(), Box<dyn Error>> {
    let text = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let text = if text.is_empty() { "HACPAD".to_string() } else { text };

    let internal_out = MidiOutput::new("hacpad-internal")?;
    let internal_port = find_port(&internal_out, "PANORAMA P1 Internal")?;
    let mut internal = internal_out.connect(&internal_port, "hacpad-internal-conn")?;

    let instrument_out = MidiOutput::new("hacpad-instrument")?;
    let instrument_port = find_port(&instrument_out, "PANORAMA P1 Instrument")?;
    let mut instrument = instrument_out.connect(&instrument_port, "hacpad-instrument-conn")?;

    println!("Sending real init sequence...");
    instrument.send(&sysex(&INIT_LINUX_ONLY))?;
    sleep(Duration::from_millis(50));
    internal.send(&sysex(&INIT_1))?;
    sleep(Duration::from_millis(50));
    internal.send(&sysex(&INIT_2))?;
    sleep(Duration::from_millis(200));

    println!("Sending quick message: {text:?}");
    let msg = write_message(&text);
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
