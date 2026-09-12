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
pub const DISPLAY_ID_PAD_STATE: u8 = 0; // padState -- raw 1-byte clip-state enum, NOT text ("Twenty-third"/"Twenty-fifth" findings)
pub const DISPLAY_ID_BIG_FONT: u8 = 2; // currentParameterInfo -- "Twelfth finding"
pub const DISPLAY_ID_TITLE_BAR: u8 = 1;
pub const DISPLAY_ID_CURRENT_VALUE: u8 = 3; // currentParameterValue, paired with big_font -- "Fifteenth finding"
pub const DISPLAY_ID_MENU_BUTTON: u8 = 4; // bottom tab row -- confirmed template-independent
pub const DISPLAY_ID_PAGE_LABELS: u8 = 5; // transport position labels -- "Nineteenth finding"
pub const DISPLAY_ID_CTRL_NAME: u8 = 6;
pub const DISPLAY_ID_CTRL_VALUE: u8 = 7;
pub const DISPLAY_ID_PAGE_MENU: u8 = 8; // Bitwig's browse popup, menuHandler.showMenu -- "Twenty-sixth"/"Twenty-seventh" findings; ALWAYS page_template=0
pub const CC_MENU_HIGHLIGHT: u8 = 111; // selectedMenuItem -- the popup's highlighted row is a plain CC, not SysEx text

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
    ("pads16", "drum_pads"),
    ("pads12", "drum_pads_3row"),
    ("list", "list"),
    ("grid5", "grid5"),
    ("message", "message"),
    ("menu", "menu"),
    ("transport-launcher", "transport_launcher"),
    ("list-highlighted", "list_highlighted"),
    ("scene-buttons", "scene_buttons"),
    ("browser-list", "browser_list"),
    ("reset", "reset"),
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

/// Like `compose_write`, but for fields whose per-entry payload is a single
/// raw byte (e.g. `padState`'s `padEntry`, confirmed from source in the
/// Twenty-third finding: `uint7ToHex(1)` length, one raw value byte) rather
/// than length-prefixed ASCII text. Structurally identical wire shape --
/// `index, length=1, byte` -- just not text.
pub fn compose_write_raw(page_template: u8, display_id: u8, entries: &[(u8, u8)]) -> Vec<u8> {
    let mut body = vec![CMD_WRITE_DISPLAY, page_template, display_id];
    for (i, (index, value)) in entries.iter().enumerate() {
        if i > 0 {
            body.push(0x00);
        }
        body.push(*index);
        body.push(0x01); // length -- always 1 raw byte
        body.push(*value);
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

/// 16 slots, not 8 -- confirmed from source (Twenty-third finding): indices
/// 1-8 are `ctrlElementValue` proper, indices 9-16 are the shared
/// `faderElementValue` (`textEntry(1+b+8, ...)` for b in 0..8).
pub fn write_values(page_template: u8, values: &[String]) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_CTRL_VALUE, &indexed_entries(values, 16))
}

/// currentParameterValue -- paired with write_bigfont (displayId 2), renders
/// top-right of the big-font row. Confirmed "global chrome" (Fourteenth
/// finding): survives a template switch untouched, like title_bar/big_font.
pub fn write_current_value(page_template: u8, text: &str) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_CURRENT_VALUE, &[(1, text)])
}

/// Transport position labels -- only ever seen rendering (a single slot, near
/// the first knob position) on template 16; not characterized on other
/// templates (Nineteenth/Twenty-fifth findings). Up to 3 named slots exist in
/// source but only 1 has ever actually been tested end-to-end.
pub fn write_page_labels(page_template: u8, labels: &[String]) -> Vec<u8> {
    compose_write(page_template, DISPLAY_ID_PAGE_LABELS, &indexed_entries(labels, 3))
}

/// padState -- raw single-byte clip-state enum per pad (NOT text; see
/// `compose_write_raw`), only actually rendered by the driver when
/// `page_template` is 21 or 22 (Twenty-third/Twenty-fifth findings), though
/// the write itself is accepted regardless. `values[i]` is the raw enum byte
/// for pad index `i+1` (0 = default/off, 1 = confirmed red/pink "lit" tint on
/// hardware; 2-8 are the rest of the 9-value clip-state enum, not
/// individually distinguished by us yet).
pub fn write_pad_state(page_template: u8, values: &[u8]) -> Vec<u8> {
    let entries: Vec<(u8, u8)> = values.iter().take(16).copied().enumerate().map(|(i, v)| ((i + 1) as u8, v)).collect();
    compose_write_raw(page_template, DISPLAY_ID_PAD_STATE, &entries)
}

/// Bitwig's patch/preset/category browse popup (`menuHandler.showMenu`) --
/// ALWAYS hardcoded to `page_template=0` in the real driver regardless of
/// whatever template is actually showing (a non-destructive overlay, not a
/// real template switch); this builder does the same unconditionally, so the
/// `page_template` argument elsewhere is irrelevant to this one call.
/// Confirmed on hardware (Twenty-seventh finding): populates cleanly on top
/// of an already-showing template without disturbing it. Up to 8 items per
/// page (real device paginates in chunks of 8 up to 24 total via `offset`,
/// not implemented here -- only the first page is exposed). Dismissing it is
/// NOT done by resending an empty entry (confirmed inert on hardware) -- the
/// only confirmed way is a genuine switch to a different real page_template.
pub fn write_page_menu(items: &[String]) -> Vec<u8> {
    compose_write(0, DISPLAY_ID_PAGE_MENU, &indexed_entries(items, 8))
}

/// The popup menu's highlighted row -- confirmed on hardware (Twenty-seventh
/// finding) to be a plain Control Change (CC 111 = selectedMenuItem, 1-based
/// row number within the current page of 8), not part of the SysEx text at
/// all, and to live-track the value rather than paint once.
pub fn cc_message(cc: u8, value: u8) -> [u8; 3] {
    [0xB0, cc, value]
}

/// Every CC confirmed (from source, "Eighth finding") to drive a simple
/// on/off illuminated-button LED via plain Control Change feedback -- same
/// CC as that control's own input, value 127=on/0=off, no SysEx involved.
/// Confirmed genuinely two-state only: no color/brightness gradient evidence
/// anywhere in source or on hardware for these. Deliberately excludes the
/// param/pan encoder CCs (48-55/64-71): those ARE also written back by the
/// real driver, but to set an LED **ring position** (0-127), and this
/// specific physical unit was confirmed (Twenty-first finding) to have no
/// LED-ring hardware at all -- nothing to light.
pub const LED_CCS: &[u8] = &[
    16, 17, 18, 19, 20, 21, 22, 23, // select/track buttons
    106, 107, 108, 109, 110, // menu buttons
    80, // transport Loop/Cycle
    84, // transport Play
    85, // transport Record
    29, // arranger automation write -- CONFIRMED on hardware: toggling this produces no visible effect
    99, // CONFIRMED on hardware: NOT the F-Keys button's own backlight -- lights the first of a row of
        // 4 small status LEDs above the screen. The other 3 in that strip have no known CC yet.
];

/// One CC message per entry in `LED_CCS`, each set to on (127) if that CC
/// appears in `on`, else off (0) -- a full resync of every known LED rather
/// than a differential update, since there are only 18 of them and this way
/// there's no separate "which LEDs are currently lit" bookkeeping to drift.
pub fn led_cc_messages(on: &[u8]) -> Vec<[u8; 3]> {
    LED_CCS.iter().map(|&cc| cc_message(cc, if on.contains(&cc) { 127 } else { 0 })).collect()
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
