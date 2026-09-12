//! Shared Panorama P1 SysEx protocol code -- the single canonical
//! implementation, used by every binary in this crate (the `panorama-bridge`
//! demo, `msg_test`/`led_test`/`chunk_test` scratch tools, and `service`, the
//! persistent WebSocket-facing service). Every byte here is confirmed
//! against real hardware -- see research/panorama-p1-protocol-notes.md.

use std::error::Error;
use std::sync::OnceLock;

use midir::{MidiInput, MidiInputPort, MidiOutput, MidiOutputPort};
use serde::Deserialize;

/// The confirmed protocol vocabulary (page templates, displayId fields, and
/// their slot counts/descriptions) -- embedded at compile time from
/// protocol.json, parsed once, and used as the actual source `layout_template`
/// reads from below, rather than a second hand-duplicated copy of the same
/// facts. protocol.json's own header names research/panorama-p1-protocol-notes.md
/// as the narrative evidence trail this summarizes; a proper schema
/// (FlatBuffers or similar) is a deliberately deferred upgrade, not done yet.
#[derive(Deserialize)]
pub struct ProtocolSpec {
    pub page_templates: Vec<PageTemplateSpec>,
    pub display_ids: Vec<DisplayIdSpec>,
}

#[derive(Deserialize)]
pub struct PageTemplateSpec {
    pub id: u8,
    pub name: String,
    #[allow(dead_code)]
    pub confirmed: bool,
    #[allow(dead_code)]
    pub description: String,
}

#[derive(Deserialize)]
pub struct DisplayIdSpec {
    #[allow(dead_code)]
    pub id: u8,
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub slots: Option<u8>,
    #[allow(dead_code)]
    pub is_text_field: Option<bool>,
    #[allow(dead_code)]
    pub description: String,
}

static PROTOCOL_JSON: &str = include_str!("../protocol.json");
static PROTOCOL: OnceLock<ProtocolSpec> = OnceLock::new();

pub fn protocol_spec() -> &'static ProtocolSpec {
    PROTOCOL.get_or_init(|| {
        serde_json::from_str(PROTOCOL_JSON).expect("protocol.json failed to parse -- check it's valid JSON")
    })
}

pub const SYSEX_PREFIX: [u8; 6] = [0xF0, 0x00, 0x01, 0x77, 0x7F, 0x01];
pub const SYSEX_END: u8 = 0xF7;

// Lifecycle messages, confirmed byte-for-byte against the official driver's
// nektarinit()/nektarexit() -- see "Fifth finding" for the port story.
pub const INIT_LINUX_ONLY: [u8; 7] = [0x08, 0x01, 0x00, 0x00, 0x01, 0x01, 0x75];
pub const INIT_1: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x01, 0x73];
pub const INIT_2: [u8; 7] = [0x09, 0x03, 0x00, 0x00, 0x01, 0x3E, 0x34];
pub const EXIT_1: [u8; 7] = [0x09, 0x00, 0x00, 0x00, 0x01, 0x00, 0x75];
pub const EXIT_2: [u8; 7] = [0x08, 0x02, 0x00, 0x00, 0x01, 0x00, 0x74];

pub const CMD_WRITE_DISPLAY: u8 = 0x06;

// pageTemplate=1's dedicated one-shot "message" shortcut (own fixed shape,
// distinct from the general compose path below) -- see "Seventh finding".
pub const MSG_PAGE_TEMPLATE: u8 = 0x01;

// DISPLAY_ID values, confirmed by direct extraction from the driver's own
// enum (not a guess) -- see "Tenth finding".
pub const DISPLAY_ID_BIG_FONT: u8 = 2; // currentParameterInfo -- "Twelfth finding"
pub const DISPLAY_ID_TITLE_BAR: u8 = 1;
pub const DISPLAY_ID_MENU_BUTTON: u8 = 4; // bottom tab row -- confirmed template-independent
pub const DISPLAY_ID_CTRL_NAME: u8 = 6;
pub const DISPLAY_ID_CTRL_VALUE: u8 = 7;

/// Real ports the official driver uses -- see "Fifth finding" for how this
/// was found to be backwards from the first assumption.
pub const PORT_DEFAULT: &str = "PANORAMA P1 Instrument";
pub const PORT_ONE: &str = "PANORAMA P1 Internal";

// The browser's `layout` dropdown values (index.html) don't match
// protocol.json's `name` fields one-for-one (the page has friendlier names
// grouped by widget shape, e.g. "faders-split" for what protocol.json calls
// "instrument_layer_container") -- this table is the (small, explicit)
// mapping between the two, kept here rather than renaming either side.
const LAYOUT_TO_TEMPLATE_NAME: &[(&str, &str)] = &[
    ("knobs", "mixer"),
    ("faders-split", "instrument_layer_container"),
    ("faders-row", "faders_row_a"),
    ("pads", "drum_pads"),
    ("list", "list"),
    ("grid5", "grid5"),
];

/// Looks up the real page-template id for a browser-facing layout name, via
/// protocol.json (see `protocol_spec`) -- not a hand-duplicated match arm.
pub fn layout_template(layout: &str) -> u8 {
    let template_name = LAYOUT_TO_TEMPLATE_NAME
        .iter()
        .find(|(l, _)| *l == layout)
        .map(|(_, name)| *name)
        .unwrap_or("mixer");
    protocol_spec()
        .page_templates
        .iter()
        .find(|t| t.name == template_name)
        .map(|t| t.id)
        .unwrap_or(16)
}

pub fn sysex(body: &[u8]) -> Vec<u8> {
    let mut msg = SYSEX_PREFIX.to_vec();
    msg.extend_from_slice(body);
    msg.push(SYSEX_END);
    msg
}

/// The pageTemplate=1 one-shot message write (`writeMessageToDisplay` in the
/// real driver) -- simpler than the compose path, no live session state
/// needed, but capped near ~127 total bytes (see "Seventh finding").
pub fn write_message(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut body = vec![CMD_WRITE_DISPLAY, MSG_PAGE_TEMPLATE, 0x00, 0x00, bytes.len() as u8];
    body.extend_from_slice(bytes);
    body.push(0x04);
    sysex(&body)
}

/// The general per-field page-composition write (`composeStart`/`textEntry`/
/// finish in the real driver). `entries`: (1-based index, text) pairs.
pub fn compose_write(page_template: u8, display_id: u8, entries: &[(u8, &str)]) -> Vec<u8> {
    let mut body = vec![CMD_WRITE_DISPLAY, page_template, display_id];
    for (i, (index, text)) in entries.iter().enumerate() {
        if i > 0 {
            body.push(0x00);
        }
        let bytes = text.as_bytes();
        body.push(*index);
        body.push(bytes.len() as u8);
        body.extend_from_slice(bytes);
    }
    sysex(&body)
}

fn indexed_entries<'a>(items: &'a [String], max: usize) -> Vec<(u8, &'a str)> {
    items
        .iter()
        .take(max)
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .map(|(i, s)| ((i + 1) as u8, s.as_str()))
        .collect()
}

pub fn write_bigfont(page_template: u8, text: &str) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_BIG_FONT, &[(1, text)])
}

pub fn write_title_bar(page_template: u8, segments: &[String]) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_TITLE_BAR, &indexed_entries(segments, 3))
}

/// Bottom menu-button row -- confirmed to work with any template, not just
/// template 2 (see the "Follow-up: displayId 4 works with ANY template"
/// finding).
pub fn write_tabs(page_template: u8, tabs: &[String]) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_MENU_BUTTON, &indexed_entries(tabs, 5))
}

pub fn write_names(page_template: u8, names: &[String]) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_CTRL_NAME, &indexed_entries(names, 16))
}

pub fn write_values(page_template: u8, values: &[String]) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_CTRL_VALUE, &indexed_entries(values, 8))
}

pub fn find_out_port(out: &MidiOutput, needle: &str) -> Result<MidiOutputPort, Box<dyn Error>> {
    out.ports()
        .into_iter()
        .find(|p| out.port_name(p).map(|n| n.contains(needle)).unwrap_or(false))
        .ok_or_else(|| format!("no MIDI output port matching {needle:?}").into())
}

pub fn find_in_port(inp: &MidiInput, needle: &str) -> Result<MidiInputPort, Box<dyn Error>> {
    inp.ports()
        .into_iter()
        .find(|p| inp.port_name(p).map(|n| n.contains(needle)).unwrap_or(false))
        .ok_or_else(|| format!("no MIDI input port matching {needle:?}").into())
}
