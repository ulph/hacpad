//! General cmd-2 op sender for interactive Advance 25 reversing.
//!
//!   send_op <op-hex> [payload-byte-hex ...]
//!
//! Builds  F0 47 00 2F 02 <op> <outerlen:2x7> <payload...> F7,  sends it paced
//! with a liveness check on each side, and prints every reply for ~1s. Payload
//! bytes are given as raw hex (each must be <0x80 to be legal SysEx data).
//!
//! This is the workbench for mapping the page/widget/show ops: send a candidate,
//! photograph the panel, keep or discard.

use advance_bridge::{ascii, hex, DEVICE_INQUIRY};
use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEVICE: &str = "ADVANCE25";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: send_op <op-hex> [payload-hex ...]");
        std::process::exit(2);
    }
    let op = u8::from_str_radix(args[1].trim_start_matches("0x"), 16)?;
    let payload: Vec<u8> = args[2..]
        .iter()
        .map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16))
        .collect::<Result<_, _>>()?;
    if payload.iter().any(|&b| b >= 0x80) {
        eprintln!("payload bytes must be <0x80");
        std::process::exit(2);
    }

    let n = payload.len();
    let cmd = std::env::var("CMD").ok().and_then(|x| u8::from_str_radix(x.trim_start_matches("0x"),16).ok()).unwrap_or(0x02); // CMD_ENV
    let mut msg = vec![0xF0, 0x47, 0x00, 0x2F, cmd, op, ((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8];
    msg.extend_from_slice(&payload);
    msg.push(0xF7);

    let (tx, rx) = mpsc::channel::<(String, Vec<u8>)>();
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();
    let scan = MidiInput::new("advance-op-scan")?;
    for p in scan.ports().into_iter().filter(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE)) {
        let name = scan.port_name(&p)?;
        let short = name.split(':').nth(1).unwrap_or(&name).trim().to_string();
        let input = MidiInput::new("advance-op")?;
        let tx = tx.clone();
        _keep.push(input.connect(&p, "advance-op", move |_t, m, _| { let _ = tx.send((short.clone(), m.to_vec())); }, ())?);
    }
    let out = MidiOutput::new("advance-op-out")?;
    let port = out.ports().into_iter()
        .find(|p| out.port_name(p).unwrap_or_default().contains("ADVANCE25 MIDI 3"))
        .ok_or("ADVANCE25 MIDI 3 not found")?;
    let mut conn = out.connect(&port, "advance-op")?;

    let alive = |conn: &mut midir::MidiOutputConnection, rx: &mpsc::Receiver<(String, Vec<u8>)>| {
        while rx.try_recv().is_ok() {}
        conn.send(&DEVICE_INQUIRY).ok();
        rx.recv_timeout(Duration::from_secs(2)).is_ok()
    };
    if !alive(&mut conn, &rx) { println!("device not answering — power-cycle."); return Ok(()); }

    println!("send op {op:#04x}: {}", hex(&msg));
    while rx.try_recv().is_ok() {}
    conn.send(&msg)?;
    let deadline = Instant::now() + Duration::from_millis(1000);
    let mut got = 0;
    while let Some(rem) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(rem) {
            Ok((p, r)) => { got += 1; let a = ascii(&r);
                let at = if a.chars().any(|c| c != '.') { format!("  \"{a}\"") } else { String::new() };
                println!("  [{p}] {}{at}", hex(&r)); }
            Err(_) => break,
        }
    }
    if got == 0 { println!("  (no reply)"); }
    std::thread::sleep(Duration::from_millis(200));
    println!("liveness: {}", if alive(&mut conn, &rx) { "alive" } else { "WEDGED" });
    Ok(())
}
