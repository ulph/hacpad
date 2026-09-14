//! Akai Advance 25 bridge — separate line of investigation from the Panorama P1.
//!
//! Nothing is asserted here yet. The P1 library earned its shape from confirmed
//! hardware behaviour; this crate starts empty on purpose, so that every type
//! that appears below is one we have evidence for.

/// Akai's SysEx manufacturer ID (one byte, 0x47).
pub const AKAI_MANUFACTURER_ID: u8 = 0x47;

/// Universal (non-realtime) Device Inquiry, broadcast to all device IDs.
pub const DEVICE_INQUIRY: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

/// Render bytes the way the research notes do, so log lines paste straight in.
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The ASCII rendering of a byte slice, with non-printables as `.`, for spotting
/// embedded strings in unknown payloads.
pub fn ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7F).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}
