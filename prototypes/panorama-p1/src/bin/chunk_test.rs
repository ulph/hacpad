//! Test whether sending a second "message" write replaces the first, or
//! whether the two can coexist (which would be required for any chunking
//! strategy to build a bigger image out of several small writes).

use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

use midir::MidiOutput;

const PREFIX: [u8; 6] = [0xF0, 0x00, 0x01, 0x77, 0x7F, 0x01];
const INIT_LINUX_ONLY: [u8; 7] = [0x08, 0x01, 0x00, 0x00, 0x01, 0x01, 0x75];
const INIT_1: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x01, 0x73];
const INIT_2: [u8; 7] = [0x09, 0x03, 0x00, 0x00, 0x01, 0x3E, 0x34];

fn sysex(body: &[u8]) -> Vec<u8> {
    let mut msg = PREFIX.to_vec();
    msg.extend_from_slice(body);
    msg.push(0xF7);
    msg
}

fn write_message(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut body = vec![0x06, 0x01, 0x00, 0x00, bytes.len() as u8];
    body.extend_from_slice(bytes);
    body.push(0x04);
    sysex(&body)
}

fn main() -> Result<(), Box<dyn Error>> {
    let default_out = MidiOutput::new("hacpad-chunk-default")?;
    let default_port = default_out
        .ports()
        .into_iter()
        .find(|p| default_out.port_name(p).map(|n| n.contains("PANORAMA P1 Instrument")).unwrap_or(false))
        .ok_or("no Instrument port")?;
    let mut default_conn = default_out.connect(&default_port, "hacpad-chunk-default-conn")?;

    let port1_out = MidiOutput::new("hacpad-chunk-port1")?;
    let port1_port = port1_out
        .ports()
        .into_iter()
        .find(|p| port1_out.port_name(p).map(|n| n.contains("PANORAMA P1 Internal")).unwrap_or(false))
        .ok_or("no Internal port")?;
    let mut port1_conn = port1_out.connect(&port1_port, "hacpad-chunk-port1-conn")?;

    println!("init...");
    port1_conn.send(&sysex(&INIT_LINUX_ONLY))?;
    sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_1))?;
    sleep(Duration::from_millis(50));
    default_conn.send(&sysex(&INIT_2))?;
    sleep(Duration::from_millis(200));

    println!("write AAAA, hold 4s...");
    default_conn.send(&write_message("AAAA-FIRST"))?;
    sleep(Duration::from_secs(4));

    println!("write BBBB (second write, no exit in between), hold 8s...");
    default_conn.send(&write_message("BBBB-SECOND"))?;
    sleep(Duration::from_secs(8));

    Ok(())
}
