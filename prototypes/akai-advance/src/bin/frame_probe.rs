//! Prove the runtime SysEx frame against real hardware.
//!
//! Static analysis (Ninth finding) says the frame is
//!     F0 47 <dev> <model> <cmd> <op> <len_hi7> <len_lo7> <payload...> F7
//! and that every op under cmd 2 validates its payload length, replying with a
//! NAK that names the op when the length is wrong.
//!
//! That NAK is the whole point of this probe. We deliberately send a WRONG
//! length: a reply proves the frame is right without ever running an operation.
//! Nothing here is expected to change device state.
//!
//! `dev` and `model` are unknown — Device Inquiry reports family 0x2F, but the
//! firmware updater used 0x2E — so sweep the plausible pairs.

use advance_bridge::{hex, DEVICE_INQUIRY};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::sync::mpsc;
use std::time::Duration;

const DEVICE: &str = "ADVANCE25";
const PACE: Duration = Duration::from_millis(120);
const REPLY_WAIT: Duration = Duration::from_millis(400);

/// Ops confirmed to carry an explicit `cmp r5, #N` length check, with the N the
/// firmware expects. Sending any other length should provoke the NAK.
const OPS_WITH_LEN: &[(u8, u16)] = &[
    (0x3a, 5), (0x39, 2), (0x32, 6), (0x36, 4), (0x2e, 5), (0x38, 11),
    (0x30, 5), (0x34, 18), (0x2c, 6), (0x37, 26), (0x31, 10), (0x35, 5),
    (0x2d, 4), (0x33, 9), (0x2f, 26), (0x2a, 8), (0x29, 12), (0x15, 10),
    (0x21, 7), (0x0d, 2),
];

fn frame(dev: u8, model: u8, cmd: u8, op: u8, payload: &[u8]) -> Vec<u8> {
    let n = payload.len() as u16;
    let mut m = vec![0xF0, 0x47, dev, model, cmd, op, (n >> 7) as u8 & 0x7F, (n & 0x7F) as u8];
    m.extend_from_slice(payload);
    m.push(0xF7);
    m
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel::<(String, Vec<u8>)>();
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();

    let scan = MidiInput::new("advance-frame-scan")?;
    for p in scan
        .ports()
        .into_iter()
        .filter(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE))
    {
        let name = scan.port_name(&p)?;
        let short = name.split(':').nth(1).unwrap_or(&name).trim().to_string();
        let input = MidiInput::new("advance-frame")?;
        let tx = tx.clone();
        _keep.push(input.connect(&p, "advance-frame", move |_t, m, _| {
            let _ = tx.send((short.clone(), m.to_vec()));
        }, ())?);
    }

    let out = MidiOutput::new("advance-frame-out")?;
    let port = out
        .ports()
        .into_iter()
        .find(|p| out.port_name(p).unwrap_or_default().contains("ADVANCE25 MIDI 3"))
        .ok_or("ADVANCE25 MIDI 3 not found")?;
    let mut conn = out.connect(&port, "advance-frame")?;

    let mut alive = |conn: &mut MidiOutputConnection, rx: &mpsc::Receiver<(String, Vec<u8>)>| {
        while rx.try_recv().is_ok() {}
        conn.send(&DEVICE_INQUIRY).ok();
        rx.recv_timeout(Duration::from_secs(2)).is_ok()
    };

    if !alive(&mut conn, &rx) {
        println!("Device is not answering before we start. Power-cycle and re-run.");
        return Ok(());
    }
    println!("baseline: device answering\n");

    // Sweep dev/model with one op, wrong length, looking for any reply at all.
    println!("== phase 1: find <dev>/<model> via a deliberately wrong length ==");
    let mut found: Option<(u8, u8)> = None;
    'outer: for model in [0x2Fu8, 0x2E, 0x00] {
        for dev in [0x00u8, 0x7F] {
            // op 0x39 wants len 2; send 3 so it must take the NAK path.
            let msg = frame(dev, model, 0x02, 0x39, &[0, 0, 0]);
            while rx.try_recv().is_ok() {}
            conn.send(&msg)?;
            if let Ok((port, reply)) = rx.recv_timeout(REPLY_WAIT) {
                println!("  dev={dev:#04x} model={model:#04x} -> REPLY on {port}");
                println!("     {}", hex(&reply));
                found = Some((dev, model));
                break 'outer;
            }
            println!("  dev={dev:#04x} model={model:#04x} -> silence");
            std::thread::sleep(PACE);
        }
    }

    let Some((dev, model)) = found else {
        println!("\nNo reply on any dev/model pair. Frame or transport assumption is wrong.");
        println!("Device still alive: {}", alive(&mut conn, &rx));
        return Ok(());
    };

    // With the addressing known, confirm the per-op length check by sending each
    // op with a length one greater than the firmware expects.
    println!("\n== phase 2: per-op NAK sweep (dev={dev:#04x} model={model:#04x}) ==");
    for &(op, want) in OPS_WITH_LEN {
        let bad = vec![0u8; want as usize + 1];
        while rx.try_recv().is_ok() {}
        conn.send(&frame(dev, model, 0x02, op, &bad))?;
        match rx.recv_timeout(REPLY_WAIT) {
            Ok((_, r)) => println!("  op {op:#04x} (wants {want:>2}) -> {}", hex(&r)),
            Err(_) => println!("  op {op:#04x} (wants {want:>2}) -> silence"),
        }
        std::thread::sleep(PACE);
    }

    println!("\nDevice still alive: {}", alive(&mut conn, &rx));
    Ok(())
}
