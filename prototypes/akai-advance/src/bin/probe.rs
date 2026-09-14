//! First contact with the Advance 25.
//!
//! Opens every ADVANCE25 input port, then sends a Universal Device Inquiry on
//! every ADVANCE25 output port, one at a time, logging which port answered.
//! Everything received is printed with the port it arrived on, so we learn the
//! port topology and the device's SysEx identity in a single run.
//!
//! Read-only apart from the inquiry itself, which is a standard MIDI query.

use advance_bridge::{ascii, hex, DEVICE_INQUIRY};
use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEVICE: &str = "ADVANCE25";

struct Rx {
    port: String,
    at: Instant,
    bytes: Vec<u8>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let (tx, rx) = mpsc::channel::<Rx>();

    // Hold the connections for the lifetime of main; dropping them closes the port.
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();

    let scan = MidiInput::new("advance-probe-scan")?;
    let in_ports: Vec<_> = scan
        .ports()
        .into_iter()
        .filter(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE))
        .collect();

    println!("== inputs ==");
    for p in &in_ports {
        let name = scan.port_name(p)?;
        println!("  {name}");
        let input = MidiInput::new("advance-probe")?;
        let tx = tx.clone();
        let label = name.clone();
        _keep.push(input.connect(
            p,
            "advance-probe",
            move |_ts, msg, _| {
                let _ = tx.send(Rx {
                    port: label.clone(),
                    at: Instant::now(),
                    bytes: msg.to_vec(),
                });
            },
            (),
        )?);
    }

    let out = MidiOutput::new("advance-probe-out")?;
    let out_ports: Vec<_> = out
        .ports()
        .into_iter()
        .filter(|p| out.port_name(p).unwrap_or_default().contains(DEVICE))
        .collect();

    println!("== outputs ==");
    for p in &out_ports {
        println!("  {}", out.port_name(p)?);
    }

    // Quiet window first: anything arriving unprompted is the device talking on
    // its own, which is itself a finding.
    println!("\n== idle listen (2s, touch nothing) ==");
    drain(&rx, start, Duration::from_secs(2));

    for p in &out_ports {
        let out = MidiOutput::new("advance-probe-out")?;
        let name = out.port_name(p)?;
        println!("\n== device inquiry -> {name} ==");
        let mut conn = out.connect(p, "advance-probe")?;
        conn.send(&DEVICE_INQUIRY)?;
        let n = drain(&rx, start, Duration::from_millis(800));
        if n == 0 {
            println!("  (no reply)");
        }
        conn.close();
    }

    println!("\n== done ==");
    Ok(())
}

/// Print everything that arrives within `window`, returning how many messages landed.
fn drain(rx: &mpsc::Receiver<Rx>, start: Instant, window: Duration) -> usize {
    let deadline = Instant::now() + window;
    let mut n = 0;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(remaining) {
            Ok(m) => {
                n += 1;
                println!(
                    "  [{:>8.3}s] {:<24} {} bytes",
                    m.at.duration_since(start).as_secs_f64(),
                    m.port,
                    m.bytes.len()
                );
                println!("      {}", hex(&m.bytes));
                let text = ascii(&m.bytes);
                if text.chars().filter(|c| *c != '.').count() >= 3 {
                    println!("      \"{text}\"");
                }
            }
            Err(_) => break,
        }
    }
    n
}
