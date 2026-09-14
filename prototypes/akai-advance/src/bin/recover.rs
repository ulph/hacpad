//! Watch the Advance 25 through a power cycle.
//!
//! Answers two things in one run:
//!   1. Does the SysEx-flood wedge (Fifth finding) clear on a power cycle alone?
//!   2. Is the `A0 00 00` first-open burst (Fourth finding) reproducible, or was
//!      it a one-off from the very first enumeration?
//!
//! Waits for the ports to disappear, waits for them to come back, opens them as
//! fast as it can to catch anything emitted at first open, then issues a Device
//! Inquiry to confirm the device is answering again.

use advance_bridge::{hex, DEVICE_INQUIRY};
use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEVICE: &str = "ADVANCE25";
const POLL: Duration = Duration::from_millis(100);
const WAIT_LIMIT: Duration = Duration::from_secs(180);
/// How long to listen after first open, to catch any enumeration-time burst.
const LISTEN: Duration = Duration::from_secs(5);

fn present() -> bool {
    let Ok(scan) = MidiInput::new("advance-recover-scan") else {
        return false;
    };
    scan.ports()
        .iter()
        .any(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE))
}

fn wait_until(want: bool, label: &str) -> bool {
    let start = Instant::now();
    print!("{label}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    while start.elapsed() < WAIT_LIMIT {
        if present() == want {
            println!(" ok ({:.1}s)", start.elapsed().as_secs_f64());
            return true;
        }
        std::thread::sleep(POLL);
    }
    println!(" TIMED OUT after {:?}", WAIT_LIMIT);
    false
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Power-cycle the Advance 25 now.\n");

    if !wait_until(false, "waiting for device to disappear...") {
        println!("Device never went away — was it actually powered off?");
        return Ok(());
    }
    if !wait_until(true, "waiting for device to come back...") {
        return Ok(());
    }

    // Small settle so ALSA has registered every port, then open all at once.
    std::thread::sleep(Duration::from_millis(250));

    let (tx, rx) = mpsc::channel::<(String, Instant, Vec<u8>)>();
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();
    let opened = Instant::now();

    let scan = MidiInput::new("advance-recover-scan")?;
    for p in scan
        .ports()
        .into_iter()
        .filter(|p| scan.port_name(p).unwrap_or_default().contains(DEVICE))
    {
        let name = scan.port_name(&p)?;
        let short = name.split(':').nth(1).unwrap_or(&name).trim().to_string();
        let input = MidiInput::new("advance-recover")?;
        let tx = tx.clone();
        _keep.push(input.connect(
            &p,
            "advance-recover",
            move |_ts, msg, _| {
                let _ = tx.send((short.clone(), Instant::now(), msg.to_vec()));
            },
            (),
        )?);
    }
    println!("\nports open, listening {LISTEN:?} for a first-open burst");

    let mut burst: Vec<(String, f64, Vec<u8>)> = Vec::new();
    let deadline = Instant::now() + LISTEN;
    while let Some(rem) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(rem) {
            Ok((port, at, bytes)) => {
                burst.push((port, at.duration_since(opened).as_secs_f64(), bytes))
            }
            Err(_) => break,
        }
    }

    if burst.is_empty() {
        println!("  nothing — the A0 00 00 burst did NOT reproduce on this open");
    } else {
        println!("  {} messages at first open:", burst.len());
        // Collapse repeats; the interesting part is shape and timing, not volume.
        let mut i = 0;
        while i < burst.len() {
            let (port, t, bytes) = &burst[i];
            let mut j = i;
            while j + 1 < burst.len() && burst[j + 1].2 == *bytes && burst[j + 1].0 == *port {
                j += 1;
            }
            let n = j - i + 1;
            println!(
                "    [{t:>6.3}s] {port:<14} {} {}",
                hex(bytes),
                if n > 1 { format!("x{n}") } else { String::new() }
            );
            i = j + 1;
        }
    }

    println!("\n== liveness ==");
    let out = MidiOutput::new("advance-recover-out")?;
    let port = out
        .ports()
        .into_iter()
        .find(|p| out.port_name(p).unwrap_or_default().contains("ADVANCE25 MIDI 3"))
        .ok_or("ADVANCE25 MIDI 3 not found")?;
    let mut conn = out.connect(&port, "advance-recover")?;
    while rx.try_recv().is_ok() {}
    conn.send(&DEVICE_INQUIRY)?;
    match rx.recv_timeout(Duration::from_secs(3)) {
        Ok((port, _, bytes)) => {
            println!("  RECOVERED — {port}: {}", hex(&bytes));
            println!("  => the flood wedge clears on a power cycle alone.");
        }
        Err(_) => {
            println!("  STILL NOT ANSWERING after a power cycle.");
            println!("  => the wedge is not merely a queue backlog; needs investigation.");
        }
    }
    Ok(())
}
