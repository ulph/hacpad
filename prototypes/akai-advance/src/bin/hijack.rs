//! Hijack a live firmware Lua page: redefine draw() in existing script slots so
//! the firmware's own redraw loop runs OUR code.
//!
//! Pages 2-7 are drawn by Lua widgets (confirmed by the page-map montage). Their
//! draw() runs continuously. Overwriting a slot's draw() with a red fill should
//! turn that page red on the next frame -- our pixels via the firmware's own
//! compositor, no page/widget creation needed.
//!
//!   hijack <page> <slot_start> <slot_end>
//!
//! Sends op 0x10 (set page) then op 0x3B (run chunk in EXISTING slot) for each
//! slot. No create_slot: we target the firmware's own live slots. Paced.

use advance_bridge::hex;
use midir::{MidiOutput};
use std::time::Duration;

fn pack(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut body = Vec::new();
    let (mut acc, mut bits): (u32, u32) = (0, 0);
    for &b in data {
        acc |= (b as u32) << bits; bits += 8;
        while bits >= 7 { body.push((acc & 0x7F) as u8); acc >>= 7; bits -= 7; }
    }
    if bits > 0 { body.push((acc & 0x7F) as u8); }
    let mut out = vec![((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8];
    out.extend(body); out
}
fn be14(v: usize) -> [u8; 2] { [((v >> 7) & 0x7F) as u8, (v & 0x7F) as u8] }

fn run_msg(slot: u16, script: &[u8]) -> Vec<u8> {
    let n = script.len();
    let mut data = Vec::new();
    data.extend_from_slice(&be14(slot as usize));
    data.extend_from_slice(&be14(n));
    data.extend_from_slice(&pack(script));
    let mut m = vec![0xF0, 0x47, 0x00, 0x2F, 0x02, 0x3B];
    m.extend_from_slice(&be14(data.len()));
    m.extend_from_slice(&data); m.push(0xF7); m
}
fn set_page(n: u8) -> Vec<u8> { vec![0xF0,0x47,0x00,0x2F,0x02,0x10,0x00,0x01,n & 0x7F,0xF7] }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let page: u8 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let s0: u16 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let s1: u16 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(31);

    // Redefine draw() to fill the panel red. Also keep an init-time fill in case
    // draw() is not the entry point for this widget.
    let script = b"function draw(a) draw_rect(0,0,480,272,0xffff0000) end draw_rect(0,0,480,272,0xffff0000)";

    let out = MidiOutput::new("advance-hijack")?;
    let port = out.ports().into_iter()
        .find(|p| out.port_name(p).unwrap_or_default().contains("ADVANCE25 MIDI 3"))
        .ok_or("ADVANCE25 MIDI 3 not found")?;
    let mut conn = out.connect(&port, "advance-hijack")?;

    conn.send(&set_page(page))?;
    println!("set page {page}");
    std::thread::sleep(Duration::from_millis(200));

    for slot in s0..=s1 {
        conn.send(&run_msg(slot, script))?;
        if slot == s0 { println!("msg[{slot}]: {}", hex(&run_msg(slot, script))); }
        std::thread::sleep(Duration::from_millis(40));
    }
    println!("redefined draw() in slots {s0}..={s1} on page {page}");
    Ok(())
}
