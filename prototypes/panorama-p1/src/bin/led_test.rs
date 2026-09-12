//! Test LED on/off feedback for the Panorama P1's illuminated buttons.
//!
//! Confirmed from the official driver: LED state is set via plain Control
//! Change messages (sendChannelController -> a normal 0xBn status byte), on
//! the SAME CC number as the control's own input, sent on the default port
//! ("Instrument" -- see msg_test.rs / main.rs doc comments for the port
//! mapping story). No SysEx involved at all for LEDs.
//!
//! Candidate LED-bearing CCs, extracted from every
//! `Z81134B25E3E8D3DA8(0, CC.<name>, ...)` (sendChannelController) call site
//! in PANORAMA_P1.control.js:
//!   16-23  select/track buttons (trackButtonLed)      -- indexed, +0..7
//!   106-110 menu buttons (5 of them)                   -- indexed, +0..4
//!   84     transport Play LED
//!   80     transport Loop/Cycle LED
//!   85     transport Record LED
//!   29     arranger automation write LED
//!   99     sent unconditionally =127 during nektarinit's post-init task
//!          (likely a static "connected"/online indicator)
//! (64-71 param-encoder and 48-55 pan-encoder CCs are also written back, but
//!  those drive per-encoder LED RING position, i.e. a value 0-127, not a
//!  simple on/off -- not attempted here, see protocol notes.)
//!
//! Usage:
//!   led_test on      # light every candidate LED (value 127)
//!   led_test off     # turn them all off (value 0)

use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

use midir::{MidiOutput, MidiOutputPort};

const CHANNEL: u8 = 0; // status byte 0xB0 = Control Change, channel 1 (0-indexed 0)

const LED_CCS: &[u8] = &[
    16, 17, 18, 19, 20, 21, 22, 23, // select/track buttons
    106, 107, 108, 109, 110, // menu buttons
    84, 80, 85, 29, // transport-related
    99, // "connected" indicator (sent unconditionally on init in the real driver)
];

fn find_port(out: &MidiOutput, needle: &str) -> Result<MidiOutputPort, Box<dyn Error>> {
    out.ports()
        .into_iter()
        .find(|p| out.port_name(p).map(|n| n.contains(needle)).unwrap_or(false))
        .ok_or_else(|| format!("no MIDI output port matching {needle:?}").into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "on".to_string());
    let value: u8 = if mode == "off" { 0 } else { 127 };

    let out = MidiOutput::new("hacpad-led-test")?;
    let port = find_port(&out, "PANORAMA P1 Instrument")?; // default port
    let mut conn = out.connect(&port, "hacpad-led-test-conn")?;

    println!("Setting {} LED CCs to value {value}...", LED_CCS.len());
    for &cc in LED_CCS {
        conn.send(&[0xB0 | CHANNEL, cc, value])?;
        sleep(Duration::from_millis(5));
    }
    println!("Done. Holding 5s to observe...");
    sleep(Duration::from_secs(5));

    Ok(())
}
