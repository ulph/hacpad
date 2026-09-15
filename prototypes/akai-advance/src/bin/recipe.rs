//! Build a page+widget+script and show it — the full draw recipe, numeric.
//!
//! Sequence (arg layouts from the op-handler disassembly):
//!   create_page(page, nwidgets)   op 0x0d  [page_u8, n_u8]
//!   create_widget(wid, a, b)      op 0x00  [wid_u14, a_u8, b_u8]
//!   create_slot(script)           op 0x39  [script_u14]
//!   load(script, chunk)           op 0x3b  [script_u14, len_u14, packed]
//!   bind(wid, flag, script)       op 0x3a  [wid_u14, flag_u8, script_u14]
//!   attach(page, slot, wid, b)    op 0x11  [page_u14, slot_u8, wid_u14, b_u8]
//!   set_active_page(page)         op 0x10  [page_u8]
//!
//! Every op replies; we print each so a failing step is visible even while the
//! panel stays blank until the whole chain is right.

use advance_bridge::{hex, DEVICE_INQUIRY};
use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc;
use std::time::Duration;

fn pack(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut body = Vec::new();
    let (mut acc, mut bits): (u32, u32) = (0, 0);
    for &b in data { acc |= (b as u32) << bits; bits += 8;
        while bits >= 7 { body.push((acc & 0x7F) as u8); acc >>= 7; bits -= 7; } }
    if bits > 0 { body.push((acc & 0x7F) as u8); }
    let mut out = vec![((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8];
    out.extend(body); out
}
fn u14(v: u16) -> [u8; 2] { [((v >> 7) & 0x7F) as u8, (v & 0x7F) as u8] }

fn frame(op: u8, payload: &[u8]) -> Vec<u8> {
    let n = payload.len();
    let mut m = vec![0xF0, 0x47, 0x00, 0x2F, 0x02, op, ((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8];
    m.extend_from_slice(payload); m.push(0xF7); m
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // defaults; override any via env for iteration
    let g = |k: &str, d: u16| std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d);
    let page = g("PAGE", 20) as u8;
    let wid = g("WID", 1);
    let script = g("SCRIPT", 100);
    let wtype_a = g("WA", 0) as u8;
    let wtype_b = g("WB", 0) as u8;
    let bind_flag = g("BF", 0) as u8;
    let slot = g("SLOT", 0) as u8;
    let attach_b = g("AB", 0) as u8;

    let chunk = b"function draw(a) draw_rect(0,0,480,272,0xffff0000) end";

    let mut steps: Vec<(&str, Vec<u8>)> = Vec::new();
    steps.push(("create_page", frame(0x0d, &[page, 1])));
    steps.push(("create_widget", frame(0x00, &[u14(wid)[0], u14(wid)[1], wtype_a, wtype_b])));
    steps.push(("create_slot", frame(0x39, &u14(script))));
    {
        let mut p = Vec::new();
        p.extend_from_slice(&u14(script));
        p.extend_from_slice(&u14(chunk.len() as u16));
        p.extend_from_slice(&pack(chunk));
        steps.push(("load", frame(0x3b, &p)));
    }
    steps.push(("bind", frame(0x3a, &[u14(wid)[0], u14(wid)[1], bind_flag, u14(script)[0], u14(script)[1]])));
    // op 0x0e = add_widget_to_page(page_u8, f0_u8, f1_u14, f2_u14, f3_u14). The
    // render loop keys widget lookup on {slot[0]=f0, slot[2]=f1}; f2/f3 land in
    // slot[4]/[6] (geometry). f0/f1 must resolve to our widget id -- swept via env.
    let f0 = g("F0", wid) as u8;
    let f1 = g("F1", 0);
    let f2 = g("F2", 0);
    let f3 = g("F3", 0);
    let mut e = vec![page, f0];
    e.extend_from_slice(&u14(f1));
    e.extend_from_slice(&u14(f2));
    e.extend_from_slice(&u14(f3));
    steps.push(("attach(0x0e)", frame(0x0e, &e)));
    steps.push(("set_active_page", frame(0x10, &[page])));
    let _ = (slot, attach_b); // retained for reference

    // MIDI in/out
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let mut _keep: Vec<MidiInputConnection<()>> = Vec::new();
    let scan = MidiInput::new("adv-recipe-scan")?;
    for p in scan.ports().into_iter().filter(|p| scan.port_name(p).unwrap_or_default().contains("ADVANCE25")) {
        let input = MidiInput::new("adv-recipe")?; let tx = tx.clone();
        _keep.push(input.connect(&p, "adv-recipe", move |_t, m, _| { let _ = tx.send(m.to_vec()); }, ())?);
    }
    let out = MidiOutput::new("adv-recipe-out")?;
    let port = out.ports().into_iter().find(|p| out.port_name(p).unwrap_or_default().contains("ADVANCE25 MIDI 3")).ok_or("no MIDI 3")?;
    let mut conn = out.connect(&port, "adv-recipe")?;

    while rx.try_recv().is_ok() {}
    conn.send(&DEVICE_INQUIRY)?;
    if rx.recv_timeout(Duration::from_secs(2)).is_err() { println!("device not answering"); return Ok(()); }

    println!("params: page={page} wid={wid} script={script} type=({wtype_a},{wtype_b}) bindflag={bind_flag} slot={slot} attachb={attach_b}\n");
    for (name, msg) in &steps {
        while rx.try_recv().is_ok() {}
        conn.send(msg)?;
        let reply = rx.recv_timeout(Duration::from_millis(500));
        match reply {
            Ok(r) => {
                // decode result byte: reply ...3D..<op> <RESULT> F7 ; RESULT 0x40=ok
                let res = r.iter().rev().nth(1).copied().unwrap_or(0);
                let ok = res == 0x40;
                println!("  {name:<16} {} -> {} ({})", hex(&msg[..msg.len().min(12)]),
                         hex(&r), if ok {"OK"} else {"?!"});
            }
            Err(_) => println!("  {name:<16} {} -> (no reply)", hex(&msg[..msg.len().min(12)])),
        }
        std::thread::sleep(Duration::from_millis(80));
    }
    println!("\ndone — look at the panel.");
    Ok(())
}
