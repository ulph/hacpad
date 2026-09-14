//! Load a Lua script onto the Advance 25 via command 2, op 0x3B.
//!
//! Payload format, fully derived from firmware (findings Ninth–Eleventh) and the
//! packer validated by round-trip against the firmware's own unpacker:
//!
//!   F0 47 00 2F 02 3B <outerlen:2x7> <id:2x7> <len:2x7> <N:2x7> <packed> F7
//!
//! where id is the script slot (<1024), len and N both equal the script byte
//! count, and <packed> is the script as a contiguous LSB-first 7-bit stream.
//! The firmware runs the script's main chunk immediately on load, so a script
//! whose top level calls draw_rect should paint at load time — that is the test.
//!
//! Safe by construction: one paced message, a liveness check before and after,
//! and every payload byte is <0x80 so nothing can be mistaken for F0/F7.

use advance_bridge::{ascii, hex, DEVICE_INQUIRY};
use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc;
use std::time::Duration;

const DEVICE: &str = "ADVANCE25";

/// Pack a byte stream as the firmware expects: 2-byte length (N, 7 bits each),
/// then the data as a contiguous LSB-first 7-bit stream.
fn pack(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut body = Vec::new();
    let (mut acc, mut bits): (u32, u32) = (0, 0);
    for &b in data {
        acc |= (b as u32) << bits;
        bits += 8;
        while bits >= 7 {
            body.push((acc & 0x7F) as u8);
            acc >>= 7;
            bits -= 7;
        }
    }
    if bits > 0 {
        body.push((acc & 0x7F) as u8);
    }
    let mut out = vec![((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8];
    out.extend(body);
    out
}

fn be14(v: usize) -> [u8; 2] {
    [((v >> 7) & 0x7F) as u8, (v & 0x7F) as u8]
}

fn load_script_msg(slot: u16, script: &[u8]) -> Vec<u8> {
    let n = script.len();
    let mut data = Vec::new();
    data.extend_from_slice(&be14(slot as usize)); // id
    data.extend_from_slice(&be14(n)); // len (B)
    data.extend_from_slice(&pack(script)); // N-prefixed packed body

    let mut msg = vec![0xF0, 0x47, 0x00, 0x2F, 0x02, 0x3B];
    msg.extend_from_slice(&be14(data.len())); // outerlen
    msg.extend_from_slice(&data);
    msg.push(0xF7);
    msg
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let slot: u16 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let script: Vec<u8> = match args.get(2) {
        Some(path) => std::fs::read(path)?,
        // Default: fill the whole 480x272 panel red, from the main chunk.
        None => b"draw_rect(0, 0, 480, 272, 0xffff0000)\n".to_vec(),
    };

    println!("slot {slot}, script {} bytes:", script.len());
    println!("  {}", String::from_utf8_lossy(&script).trim());

    let msg = load_script_msg(slot, &script);
    println!("message {} bytes: {}", msg.len(), hex(&msg[..msg.len().min(48)]));

    let (tx, rx) = mpsc::channel::<(String, Vec<u8>)>();
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();
    let scan = MidiInput::new("advance-load-scan")?;
    for p in scan
        .ports()
        .into_iter()
        .filter(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE))
    {
        let name = scan.port_name(&p)?;
        let short = name.split(':').nth(1).unwrap_or(&name).trim().to_string();
        let input = MidiInput::new("advance-load")?;
        let tx = tx.clone();
        _keep.push(input.connect(&p, "advance-load", move |_t, m, _| {
            let _ = tx.send((short.clone(), m.to_vec()));
        }, ())?);
    }

    let out = MidiOutput::new("advance-load-out")?;
    let port = out
        .ports()
        .into_iter()
        .find(|p| out.port_name(p).unwrap_or_default().contains("ADVANCE25 MIDI 3"))
        .ok_or("ADVANCE25 MIDI 3 not found")?;
    let mut conn = out.connect(&port, "advance-load")?;

    let alive = |conn: &mut midir::MidiOutputConnection, rx: &mpsc::Receiver<(String, Vec<u8>)>| {
        while rx.try_recv().is_ok() {}
        conn.send(&DEVICE_INQUIRY).ok();
        rx.recv_timeout(Duration::from_secs(2)).is_ok()
    };

    if !alive(&mut conn, &rx) {
        println!("device not answering before load — power-cycle and retry.");
        return Ok(());
    }
    println!("baseline: alive");

    while rx.try_recv().is_ok() {}
    conn.send(&msg)?;
    println!("sent. collecting all replies for 1.2s...");
    let deadline = std::time::Instant::now() + Duration::from_millis(1200);
    let mut n = 0;
    while let Some(rem) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(rem) {
            Ok((port, r)) => {
                n += 1;
                let a = ascii(&r);
                let atxt = if a.chars().any(|c| c != '.') { format!("  \"{a}\"") } else { String::new() };
                println!("  [{port}] {}{atxt}", hex(&r));
            }
            Err(_) => break,
        }
    }
    if n == 0 {
        println!("  no reply (parser may have rejected the frame).");
    }

    std::thread::sleep(Duration::from_millis(300));
    println!("post-load liveness: {}", if alive(&mut conn, &rx) { "alive" } else { "WEDGED — power-cycle" });
    println!("\nLook at the Advance screen. A red fill means the main chunk drew.");
    Ok(())
}
