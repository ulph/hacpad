//! Characterise what the Advance 25 emits with nobody touching it.
//!
//! The first-contact probe saw a burst of `A0 00 00` on MIDI 2 and MIDI 3.
//! Before reading anything into that, establish: is it a steady heartbeat, a
//! one-off flush when the port opens, or noise? Counts per port per message
//! shape, sampled in one-second buckets.

use advance_bridge::hex;
use midir::{MidiInput, MidiInputConnection};
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEVICE: &str = "ADVANCE25";
const SECONDS: u64 = 10;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let (tx, rx) = mpsc::channel::<(String, Instant, Vec<u8>)>();
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();

    let scan = MidiInput::new("advance-idle-scan")?;
    for p in scan
        .ports()
        .into_iter()
        .filter(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE))
    {
        let name = scan.port_name(&p)?;
        let short = name.split(':').nth(1).unwrap_or(&name).trim().to_string();
        let input = MidiInput::new("advance-idle")?;
        let tx = tx.clone();
        let label = short.clone();
        _keep.push(input.connect(
            &p,
            "advance-idle",
            move |_ts, msg, _| {
                let _ = tx.send((label.clone(), Instant::now(), msg.to_vec()));
            },
            (),
        )?);
    }

    println!("listening {SECONDS}s — do not touch the device\n");

    // (port, message-bytes) -> count, plus a per-second histogram per port.
    let mut shapes: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut buckets: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    let deadline = Instant::now() + Duration::from_secs(SECONDS);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok((port, at, bytes)) = rx.recv_timeout(remaining) else {
            break;
        };
        let sec = at.duration_since(start).as_secs() as usize;
        let hist = buckets
            .entry(port.clone())
            .or_insert_with(|| vec![0; SECONDS as usize + 2]);
        if sec < hist.len() {
            hist[sec] += 1;
        }
        *shapes.entry((port, hex(&bytes))).or_default() += 1;
    }

    println!("== distinct messages ==");
    for ((port, msg), n) in &shapes {
        println!("  {port:<14} {msg:<40} x{n}");
    }

    println!("\n== per-second counts ==");
    for (port, hist) in &buckets {
        let cells: Vec<String> = hist.iter().map(|n| format!("{n:>4}")).collect();
        println!("  {port:<14} {}", cells.join(" "));
    }

    if shapes.is_empty() {
        println!("  (silent)");
    }
    Ok(())
}
