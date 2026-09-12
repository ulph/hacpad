//! hacpad USB Bridge prototype -- Nektar Panorama P1
//!
//! Minimal, dependency-free bridge: talks to the P1 over its ALSA rawmidi
//! device node directly (no external MIDI crate needed). Two jobs:
//!
//!   1. Write to the P1's own screen via vendor SysEx (output/feedback).
//!   2. Decode incoming Control Change messages from its physical controls
//!      (input) using the documented CC map.
//!
//! Protocol facts (SysEx structure, CC map) are written up independently in
//! research/panorama-p1-protocol-notes.md, credited there to a community
//! reference (LukeLandry/nektar-panorama-p1-bitwig, no license declared).
//! This is our own from-scratch implementation of those facts, not a port
//! of that project's code.
//!
//! Usage:
//!     cargo run -- [/dev/snd/midiC1D0]

use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;

const DEFAULT_DEVICE: &str = "/dev/snd/midiC1D0";

// --- SysEx protocol ------------------------------------------------------
// F0 00 01 77 7F 01 ...  F7
//   00 01 77 = Nektar's registered 3-byte MIDI SysEx manufacturer ID
//   7F 01    = device/model + unit byte (unverified, assumed fixed for P1)

const SYSEX_PREFIX: [u8; 6] = [0xF0, 0x00, 0x01, 0x77, 0x7F, 0x01];
const SYSEX_END: u8 = 0xF7;

// Lifecycle messages (bytes after the manufacturer prefix, before F7).
const INIT_LINUX_ONLY: [u8; 7] = [0x08, 0x01, 0x00, 0x00, 0x01, 0x01, 0x75];
const INIT_1: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x01, 0x73];
const INIT_2: [u8; 7] = [0x09, 0x03, 0x00, 0x00, 0x01, 0x3E, 0x34];
const EXIT_1: [u8; 7] = [0x09, 0x00, 0x00, 0x00, 0x01, 0x00, 0x75];
const EXIT_2: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x00, 0x74];

const WRITE: u8 = 0x06;

// "part" bytes for a display write -- which region of the screen.
// NOTE: the community reference has two overlapping numbering schemes for
// this byte (headerLine/messageLine/messageValue/buttonLabels/toggleModeDisplays
// = 01-05 in one place, pageTitle/controlNames/controlValues/menu = 05-08 in
// another). Unverified which applies when; button labels (0x04) is the one
// we've actually sent to the hardware so far (visual confirmation pending
// better camera focus -- see protocol notes).
#[allow(dead_code)]
const PART_HEADER_LINE: u8 = 0x01;
#[allow(dead_code)]
const PART_MESSAGE_LINE: u8 = 0x02;
#[allow(dead_code)]
const PART_MESSAGE_VALUE: u8 = 0x03;
#[allow(dead_code)]
const PART_BUTTON_LABELS: u8 = 0x04;
#[allow(dead_code)]
const PART_TOGGLE_MODE_DISPLAYS: u8 = 0x05;

#[allow(dead_code)]
const LAYOUT_MIXER: u8 = 0x02; // unverified byte value guess; only 0x02 has been tried

// Known-good example message (replayed verbatim from the protocol notes --
// confirmed to transmit without error; visual confirmation on the P1's own
// screen is still pending better camera focus).
const EXAMPLE_BUTTON_LABELS_HEX: &str = "F0 00 01 77 7F 01 06 02 04 00 05 00 00 00 00 00 00 01 01 2B 00 02 07 42 72 6F 77 73 65 72 00 03 06 50 72 65 73 65 74 00 04 06 52 65 6D 6F 74 65 00 05 05 50 61 67 65 73 F7";

fn hex_to_bytes(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).expect("bad hex byte in literal"))
        .collect()
}

/// Build one "write text at index" SysEx entry: index, len, ascii bytes.
fn build_write_entry(index: u8, text: &str, max_len: Option<usize>) -> Vec<u8> {
    let truncated: &str = match max_len {
        Some(n) => &text[..text.len().min(n)],
        None => text,
    };
    let mut out = vec![index, truncated.len() as u8];
    out.extend_from_slice(truncated.as_bytes());
    out
}

/// Mirrors the observed shape: WRITE, layout, part, then each entry's bytes
/// joined by a single 0x00 separator, terminated by F7. Unverified beyond
/// the one known example message -- treat constructed-from-scratch messages
/// as a hypothesis to confirm on hardware before trusting them.
#[allow(dead_code)]
fn build_display_write(layout: u8, part: u8, entries: &[(u8, &str)]) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::new();
    for (i, (index, text)) in entries.iter().enumerate() {
        if i > 0 {
            body.push(0x00);
        }
        body.extend(build_write_entry(*index, text, None));
    }
    let mut msg = SYSEX_PREFIX.to_vec();
    msg.push(WRITE);
    msg.push(layout);
    msg.push(part);
    msg.extend(body);
    msg.push(SYSEX_END);
    msg
}

fn sysex(payload_after_prefix: &[u8]) -> Vec<u8> {
    let mut msg = SYSEX_PREFIX.to_vec();
    msg.extend_from_slice(payload_after_prefix);
    msg.push(SYSEX_END);
    msg
}

// --- CC map ---------------------------------------------------------------
// From research/panorama-p1-protocol-notes.md; transport/nav ordering within
// their ranges is an unverified guess (source only gave the range and the
// named functions, not the exact per-CC assignment).

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

struct PanoramaP1 {
    file: std::fs::File,
}

impl PanoramaP1 {
    fn open(path: &str) -> std::io::Result<Self> {
        // O_NONBLOCK on the read side would complicate a simple blocking
        // demo loop, so we open plain read+write; ALSA rawmidi accepts a
        // raw MIDI byte stream directly, no ioctl setup needed for this.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(0) // explicit: no O_NONBLOCK
            .open(path)?;
        Ok(Self { file })
    }

    fn send_init(&mut self, linux: bool) -> std::io::Result<()> {
        if linux {
            self.file.write_all(&sysex(&INIT_LINUX_ONLY))?;
        }
        self.file.write_all(&sysex(&INIT_1))?;
        self.file.write_all(&sysex(&INIT_2))?;
        Ok(())
    }

    fn send_exit(&mut self) -> std::io::Result<()> {
        self.file.write_all(&sysex(&EXIT_1))?;
        self.file.write_all(&sysex(&EXIT_2))?;
        Ok(())
    }

    fn write_known_button_labels(&mut self) -> std::io::Result<()> {
        let msg = hex_to_bytes(EXAMPLE_BUTTON_LABELS_HEX);
        self.file.write_all(&msg)
    }

    /// Blocking read loop: minimal running parser for Control Change
    /// (0xBn) messages, passing through/ignoring SysEx (0xF0 ... 0xF7)
    /// since input is documented as CC-only, but we don't want to choke
    /// if the device ever echoes one back.
    fn read_events<F: FnMut(Event)>(&mut self, mut on_event: F) -> std::io::Result<()> {
        let mut buf = [0u8; 64];
        let mut msg: Vec<u8> = Vec::with_capacity(3);
        let mut in_sysex = false;
        loop {
            let n = self.file.read(&mut buf)?;
            if n == 0 {
                continue;
            }
            for &b in &buf[..n] {
                if in_sysex {
                    if b == SYSEX_END {
                        in_sysex = false;
                    }
                    continue;
                }
                if b == 0xF0 {
                    in_sysex = true;
                    continue;
                }
                msg.push(b);
                if msg.len() >= 3 && (0xB0..=0xBF).contains(&msg[0]) {
                    on_event(decode_cc(msg[1], msg[2]));
                    msg.clear();
                } else if msg.len() == 1 && msg[0] < 0x80 {
                    // stray data byte with no status -- drop it
                    msg.clear();
                } else if msg.len() > 3 {
                    msg.clear();
                }
            }
        }
    }
}

impl Drop for PanoramaP1 {
    fn drop(&mut self) {
        let _ = self.send_exit();
    }
}

fn main() -> std::io::Result<()> {
    let device = env::args().nth(1).unwrap_or_else(|| DEFAULT_DEVICE.to_string());
    println!("Opening {device} ...");
    let mut p1 = PanoramaP1::open(&device)?;

    println!("Sending init sequence...");
    p1.send_init(true)?;
    std::thread::sleep(Duration::from_millis(200));

    println!("Writing test button labels (Browser/Preset/Remote/Pages, known-good example)...");
    p1.write_known_button_labels()?;

    println!("Listening for CC input (Ctrl-C to stop and send exit sequence)...");
    p1.read_events(|event| {
        println!("{event:?}");
    })?;

    Ok(())
}
