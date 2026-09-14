//! Measure the real byte rate to the Advance 25, by drain rather than enqueue.
//!
//! An earlier version of this binary sent 1 MB unpaced and reported ~89 MB/s.
//! That number was the ALSA buffer accepting bytes, not the wire — the device is
//! full-speed USB and cannot do that — and the flood wedged the device's MIDI
//! interface hard enough to need a power cycle. See the Fifth and Sixth findings
//! in research/akai-advance-protocol-notes.md.
//!
//! This version measures what actually matters: send a bounded, paced volume,
//! then time how long a trailing Device Inquiry takes to come back. The reply
//! cannot overtake the queue ahead of it, so that latency is the drain time.
//!
//! Payloads use manufacturer ID 0x7D (reserved for non-commercial use), which no
//! Akai device should act on.

use advance_bridge::{hex, DEVICE_INQUIRY};
use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEVICE: &str = "ADVANCE25";
const CHUNK: usize = 256;
/// Start small. Raise deliberately, one step at a time, checking liveness after
/// each — this device is known to wedge under volume.
const CHUNKS: usize = 64; // 16 KB
/// Gap between messages. Unpaced sending is what wedged the device.
const PACE: Duration = Duration::from_millis(2);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(20);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel::<(String, Vec<u8>)>();
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();

    let scan = MidiInput::new("advance-tp-scan")?;
    for p in scan
        .ports()
        .into_iter()
        .filter(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE))
    {
        let name = scan.port_name(&p)?;
        let short = name.split(':').nth(1).unwrap_or(&name).trim().to_string();
        let input = MidiInput::new("advance-tp")?;
        let tx = tx.clone();
        _keep.push(input.connect(
            &p,
            "advance-tp",
            move |_ts, msg, _| {
                let _ = tx.send((short.clone(), msg.to_vec()));
            },
            (),
        )?);
    }

    let out = MidiOutput::new("advance-tp-out")?;
    let port = out
        .ports()
        .into_iter()
        .find(|p| out.port_name(p).unwrap_or_default().contains("ADVANCE25 MIDI 3"))
        .ok_or("ADVANCE25 MIDI 3 not found")?;
    let mut conn = out.connect(&port, "advance-tp")?;

    // Prove the device is answering *before* we load it, so a silent result
    // afterwards is attributable to this run and not to a pre-existing wedge.
    println!("== baseline liveness ==");
    conn.send(&DEVICE_INQUIRY)?;
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok((port, bytes)) => println!("  {port}: {}", hex(&bytes)),
        Err(_) => {
            println!("  NO REPLY before sending anything — device is already wedged.");
            println!("  Power-cycle it and re-run; do not interpret this run.");
            return Ok(());
        }
    }
    while rx.try_recv().is_ok() {}

    let mut msg = vec![0xF0, 0x7D];
    msg.resize(CHUNK - 1, 0x00);
    msg.push(0xF7);

    let total = CHUNK * CHUNKS;
    println!("\n== sending {total} bytes, {CHUNK}B/msg, {PACE:?} apart ==");
    let start = Instant::now();
    for _ in 0..CHUNKS {
        conn.send(&msg)?;
        std::thread::sleep(PACE);
    }
    let enqueued = start.elapsed();

    // The inquiry queues behind everything above; its reply marks the drain.
    conn.send(&DEVICE_INQUIRY)?;
    match rx.recv_timeout(DRAIN_TIMEOUT) {
        Ok((port, bytes)) => {
            let drained = start.elapsed();
            println!("  enqueued in {:.2}s", enqueued.as_secs_f64());
            println!("  drained  in {:.2}s  ({port}: {})", drained.as_secs_f64(), hex(&bytes));
            let rate = total as f64 / drained.as_secs_f64();
            println!("\n== result ==");
            println!("  >= {:.1} KB/s of MIDI stream", rate / 1024.0);
            if drained.as_secs_f64() <= enqueued.as_secs_f64() * 1.1 {
                println!("  NOTE: drain kept up with the {PACE:?} pacing, so this is a");
                println!("  lower bound only — the pipe was never saturated. Reduce PACE");
                println!("  or raise CHUNKS to find the ceiling.");
            }
        }
        Err(_) => println!("  NO REPLY within {DRAIN_TIMEOUT:?} — wedged again. Power-cycle."),
    }
    Ok(())
}
