//! Shared Panorama P1 SysEx protocol code -- the single canonical
//! implementation, used by every binary in this crate (the `panorama-bridge`
//! demo, `msg_test`/`led_test`/`chunk_test` scratch tools, and `service`, the
//! persistent WebSocket-facing service). Every byte here is confirmed
//! against real hardware -- see research/panorama-p1-protocol-notes.md.

use std::collections::HashMap;
use std::error::Error;
use std::sync::OnceLock;

use midir::{MidiInput, MidiInputPort, MidiOutput, MidiOutputPort};
use serde::{Deserialize, Serialize};

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

/// The ONLY read-back this protocol offers at all (confirmed repeatedly: no
/// query/state-read capability exists otherwise). The device replies to
/// `0x09`-family lifecycle SysEx specifically -- confirmed directly that
/// `0x08`-family lifecycle commands and ordinary `0x06` compose writes get
/// NO reply whatsoever, so this cannot verify an arbitrary write, only a
/// lifecycle (init/exit) one. The reply mirrors the sent bytes exactly
/// except the manufacturer sub-id (byte 5: `0x01` host-to-device becomes
/// `0x02` device-to-host) and the final content byte (the one right before
/// the trailing `0xF7`), which is decremented by 1 -- e.g. sending INIT_2
/// (`...09 03 00 00 01 3E 34`) gets back `...09 03 00 00 01 3E 33`.
/// Returns `None` if `sent` isn't a `0x09`-family message (nothing to
/// expect an ack for).
pub fn expected_ack(sent: &[u8]) -> Option<Vec<u8>> {
    if sent.len() < 9 || sent[0] != 0xF0 || sent[5] != 0x01 || sent[6] != 0x09 {
        return None;
    }
    let mut ack = sent.to_vec();
    ack[5] = 0x02;
    let last = ack.len() - 2; // the byte immediately before the trailing F7
    ack[last] = ack[last].wrapping_sub(1);
    Some(ack)
}

/// Convenience wrapper around `expected_ack` for checking an actually-
/// received message against what was sent.
pub fn is_expected_ack(sent: &[u8], received: &[u8]) -> bool {
    expected_ack(sent).as_deref() == Some(received)
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

// --- Semantic verb layer -----------------------------------------------
//
// Everything above this point is the WIRE layer: page_template ids,
// displayId numbers, raw compose entries. See
// research/panorama-p1-widget-model.md for the model this implements --
// callers of `Background`/`switch_background_messages` never need to know
// a template id or displayId number at all; the semantics (which fields
// THIS widget shape actually draws) are the API surface instead.

/// A raw padState byte, semantically -- source confirms a 9-value clip-state
/// enum; we've only visually distinguished 2 of the 9 on hardware (see
/// "Twenty-fifth finding"), so the rest are kept as `Raw(n)` rather than
/// guessed at.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum PadState {
    Default,
    Lit, // confirmed red/pink tint on hardware
    Raw(u8),
}

impl PadState {
    fn as_byte(self) -> u8 {
        match self {
            PadState::Default => 0,
            PadState::Lit => 1,
            PadState::Raw(v) => v,
        }
    }
}

/// The Background verb (Axis 1's "Background" layer, paired with its own
/// Content -- switching a Background always clears Content, so the two
/// travel together in one call, per the widget-model doc). Each variant's
/// fields are exactly what that Background's Content schema actually holds,
/// confirmed on hardware ("Tenth"/"Fifteenth"/"Sixteenth" findings) -- no
/// page_template id or displayId number appears anywhere in this type.
/// `#[serde(tag = "type")]`: the wire shape for e.g. `Mixer` is
/// `{"type": "Mixer", "paramNames": [...], "paramValues": [...]}` -- this is
/// what lets a WebSocket client send a `Background` directly as part of a
/// semantic command (see service.rs's `SemanticCommand::SwitchBackground`)
/// instead of the service needing a hand-maintained parallel DTO. Each
/// variant gets its own `rename_all` (serde applies it per-variant, not
/// automatically from the enum) so field names are camelCase on the wire,
/// matching the rest of the JSON protocol (`titleBar`, `padState`, etc.).
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Background {
    /// 4x2 knob grid (the default page on connect).
    #[serde(rename_all = "camelCase")]
    Mixer { param_names: [String; 8], param_values: [String; 8] },
    /// 8 faders split into two groups of 4; 16 labels, two stacked per fader.
    FaderSplit { labels: [String; 16] },
    /// 8 faders in one continuous row.
    FaderRow { labels: [String; 8] },
    /// 4x4 pad grid, rows A-D, all 16 pads individually labelable.
    #[serde(rename_all = "camelCase")]
    PadView { pad_names: [String; 16], pad_states: [PadState; 16] },
    /// 3x4 pad grid, rows A-C only -- genuinely distinct from PadView, not
    /// just an unlabeled 4th row (Sixteenth finding).
    #[serde(rename_all = "camelCase")]
    PadView3Row { pad_names: [String; 12], pad_states: [PadState; 12] },
    /// 1 fader + a vertical bulleted list, hard-capped at 5 visible entries.
    List { items: [String; 5] },
    /// Plain 2x4 button grid, no fader/knob widgets.
    Grid { labels: [String; 8] },
    /// `L:`/`R:` locator bars + a 2x4 grid beneath.
    #[serde(rename_all = "camelCase")]
    TransportLauncher { loop_left: String, loop_right: String, labels: [String; 8] },
    /// Content-area widget never characterized -- only the (template-
    /// independent) bottom menu-button relabeling was ever tested against
    /// this template. Kept as a bare marker, no content fields offered yet.
    Menu,
    /// A list with one row shown highlighted; exact slot count/labeling
    /// scheme not characterized beyond "list-like" (Fifteenth finding) --
    /// kept as a generic Vec rather than a confirmed fixed size.
    ListHighlighted { items: Vec<String>, highlighted: usize },
    /// 4 scene buttons `S1`-`S4` (+ a "B" indicator whose own field is
    /// unidentified).
    SceneButtons { labels: [String; 4] },
    /// Up to 8 rows, each paired with a "Pre" label; this covers BOTH
    /// template 8 and 9, which render indistinguishably (Fifteenth finding).
    BrowserList { items: Vec<String> },
    /// Reset sentinel -- Content writes are a confirmed no-op here.
    Reset,
    /// A deliberately EMPTY content area -- reuses template 5 (`Grid`, the
    /// plainest layout tested: no fader/knob widgets at all) with nothing
    /// written to it. Distinct from `Reset` (template 0), which falls back
    /// to the device's own native default fader view rather than actually
    /// looking blank. Chrome (title_bar/big_font/etc) still renders on top
    /// regardless of Background -- pass empty chrome fields too via
    /// `DeviceState` for the closest thing to an actually black screen.
    Blank,
}

impl Background {
    fn page_template(&self) -> u8 {
        match self {
            Background::Mixer { .. } => 16,
            Background::FaderSplit { .. } => 18,
            Background::FaderRow { .. } => 19,
            Background::PadView { .. } => 21,
            Background::PadView3Row { .. } => 22,
            Background::List { .. } => 4,
            Background::Grid { .. } => 5,
            Background::TransportLauncher { .. } => 3,
            Background::Menu => 2,
            Background::ListHighlighted { .. } => 6,
            Background::SceneButtons { .. } => 7,
            Background::BrowserList { .. } => 8,
            Background::Reset => 0,
            Background::Blank => 5,
        }
    }

    /// Every variant's own fields, mapped onto the four Body-layer writes
    /// (displayId 6/7/0/5 -- `ctrl_element_name`/`ctrl_element_value`/
    /// `pad_state`/`page_labels`). `None` means this variant doesn't define
    /// that field AT ALL -- nullable, per protocol.json's own
    /// `persists_across_template_switch` facts: `ctrl_element_name` (6) is
    /// CONFIRMED to auto-clear on a genuine template switch (Fourteenth
    /// finding) without us sending anything, so a variant that doesn't use
    /// it can just omit the write rather than force a blank one. (The other
    /// three Body ids are `null`/unconfirmed there -- assumed to behave the
    /// same by analogy, not independently proven; re-verify with the
    /// webcam if that assumption ever turns out wrong, same as every other
    /// finding in this file.) Chrome (title_bar/big_font/current_value/
    /// tabs) is the opposite case -- confirmed STICKY (`true`, survives a
    /// switch untouched), which is exactly why those four are unconditional
    /// in `full_redraw` rather than nullable like these.
    fn body_fields(&self) -> BodyFields {
        let mut f = BodyFields::default();
        match self {
            Background::Mixer { param_names, param_values } => {
                f.names = Some(param_names.to_vec());
                f.values = Some(param_values.to_vec());
            }
            Background::FaderSplit { labels } => f.names = Some(labels.to_vec()),
            Background::FaderRow { labels } => f.names = Some(labels.to_vec()),
            Background::Grid { labels } => f.names = Some(labels.to_vec()),
            Background::PadView { pad_names, pad_states } => {
                f.names = Some(pad_names.to_vec());
                f.pad_state = Some(pad_states.iter().map(|s| s.as_byte()).collect());
            }
            Background::PadView3Row { pad_names, pad_states } => {
                f.names = Some(pad_names.to_vec());
                f.pad_state = Some(pad_states.iter().map(|s| s.as_byte()).collect());
            }
            Background::List { items } => f.names = Some(items.to_vec()),
            Background::TransportLauncher { loop_left, loop_right, labels } => {
                f.page_labels = Some(vec![loop_left.clone(), loop_right.clone()]);
                f.names = Some(labels.to_vec());
            }
            Background::Menu | Background::Reset | Background::Blank => {}
            Background::ListHighlighted { items, .. } => f.names = Some(items.clone()),
            Background::SceneButtons { labels } => f.names = Some(labels.to_vec()),
            Background::BrowserList { items } => f.names = Some(items.clone()),
        }
        f
    }
}

/// The four Body-layer writes, independent of which fields a given
/// `Background` variant's own typed schema happens to expose -- see
/// `Background::body_fields`. Each is `Option`: `None` means "this variant
/// doesn't define this field, omit the write entirely" (nullable/hidden-if-
/// omitted), not "send an empty one" -- see `body_fields`'s doc comment for
/// why that's safe here specifically.
#[derive(Default)]
struct BodyFields {
    names: Option<Vec<String>>,
    values: Option<Vec<String>>,
    pad_state: Option<Vec<u8>>,
    page_labels: Option<Vec<String>>,
}

/// Header layer (displayId 1/2/3 -- `title_bar`/`big_font`/`current_value`).
/// Confirmed STICKY: survives a Background switch untouched (Fourteenth
/// finding) -- which is exactly why it must be explicitly force-resent by
/// `full_redraw` rather than left alone; an overlay drawn over it leaves
/// stale content there until something rewrites it.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub title_bar: Vec<String>,
    pub big_font: String,
    pub current_value: String,
}

/// Footer layer (displayId 4 -- `menu_button`/tabs). Same stickiness as
/// Header, kept as its own type since it's a physically separate strip
/// (bottom of screen vs. top) -- see the widget-model doc's layer table.
/// Confirmed live (webcam, Thirty-eighth finding) that a popup's own
/// `Esc`/`Enter` buttons render through this SAME slot, which is exactly why
/// leaving it unresent left them permanently ghosted after dismissing one.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Footer {
    pub tabs: Vec<String>,
}

/// Tracks what's actually been sent, so `hide_popup`/`hide_message` can
/// restore the right thing -- see the widget-model doc's "trigger-only, no
/// clear primitive" class. There is no query/read-back on this device
/// (confirmed repeatedly throughout this project), so this struct IS the
/// only place "current state" exists at all; it's a plain in-memory model,
/// not derived from the device.
pub struct DeviceState {
    last_background: Option<Background>,
    header: Header,
    popup_visible: bool,
    message_visible: bool,
    // Last content handed to show_popup/show_message -- a SOFTWARE mirror
    // for reporting back to a UI (see `snapshot()`), not a device read-back
    // (none exists). Kept even while hidden; only *_visible says whether
    // it's actually on screen right now.
    last_popup_items: Vec<String>,
    last_popup_highlight: Option<u8>,
    last_message_text: Option<String>,
    footer: Footer,
}

impl Default for DeviceState {
    fn default() -> Self {
        DeviceState {
            last_background: None,
            header: Header::default(),
            popup_visible: false,
            message_visible: false,
            last_popup_items: Vec::new(),
            last_popup_highlight: None,
            last_message_text: None,
            footer: Footer {
                // 5 real (non-empty, so indexed_entries doesn't filter them
                // out and skip the slot) but visually blank placeholders --
                // we don't know what a caller wants shown here yet, and
                // leaving stale text is worse than genuinely blank.
                tabs: vec![" ".to_string(); 5],
            },
        }
    }
}

/// What `DeviceState` currently believes is showing -- for handing back to a
/// client (e.g. right after it sends a semantic command, or on connect) so
/// its UI can reflect the real in-memory model instead of re-deriving it.
/// Software mirror only, per `DeviceState`'s own doc comment -- not a device
/// read-back.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSnapshot {
    pub background: Option<Background>,
    pub header: Header,
    pub footer: Footer,
    pub popup_visible: bool,
    pub popup_items: Vec<String>,
    pub popup_highlight: Option<u8>,
    pub message_visible: bool,
    pub message_text: Option<String>,
}

impl DeviceState {
    pub fn popup_visible(&self) -> bool {
        self.popup_visible
    }

    pub fn message_visible(&self) -> bool {
        self.message_visible
    }

    pub fn snapshot(&self) -> DeviceSnapshot {
        DeviceSnapshot {
            background: self.last_background.clone(),
            header: self.header.clone(),
            footer: self.footer.clone(),
            popup_visible: self.popup_visible,
            popup_items: self.last_popup_items.clone(),
            popup_highlight: self.last_popup_highlight,
            message_visible: self.message_visible,
            message_text: self.last_message_text.clone(),
        }
    }

    /// THE verb that changes which Background is active -- also the only
    /// way to dismiss Message or an open popup, since both get marked
    /// hidden here (a real switch clears them as a side effect, confirmed
    /// -- Thirteenth/Thirty-first findings).
    pub fn switch_background(&mut self, bg: Background, title_bar: Vec<String>) -> Vec<Vec<u8>> {
        let msgs = self.full_redraw(&bg, &title_bar);
        self.last_background = Some(bg);
        self.header.title_bar = title_bar;
        self.popup_visible = false;
        self.message_visible = false;
        msgs
    }

    /// The complete "resting" screen state for one Background -- every
    /// displayId that's visible when NO overlay is showing (Header's
    /// title_bar/big_font/current_value, all four Body writes via
    /// `switch_background_messages`, and Footer's tabs). An overlay (popup
    /// or message) can cover any part of the screen, so tearing one down
    /// cleanly means redrawing ALL of these, not a hand-picked subset --
    /// direct instruction, after Body alone (Thirty-eighth finding) and then
    /// Body+Footer (same finding, follow-up) each turned out to still leave
    /// something orphaned. Used by both `switch_background` (a real,
    /// caller-requested change) and `resend_body` (restoring after an
    /// overlay, which additionally bounces through a different template
    /// first -- see that method's own doc comment).
    fn full_redraw(&self, bg: &Background, title_bar: &[String]) -> Vec<Vec<u8>> {
        let t = bg.page_template();
        let mut msgs = switch_background_messages(bg, title_bar);
        msgs.push(write_tabs(t, &self.footer.tabs));
        msgs.push(write_bigfont(t, &self.header.big_font));
        msgs.push(write_current_value(t, &self.header.current_value));
        msgs
    }

    /// Sets the Footer (tabs) content and re-sends it immediately against
    /// whatever Background is currently active (Footer is confirmed
    /// template-independent, so the exact `page_template` byte doesn't
    /// matter beyond being *a* valid one) -- also remembered so every
    /// subsequent `switch_background`/`resend_body` keeps re-asserting it,
    /// instead of leaving it to whatever last wrote that slot.
    pub fn set_footer(&mut self, tabs: Vec<String>) -> Vec<u8> {
        let t = self.last_background.as_ref().map(|b| b.page_template()).unwrap_or(16);
        let msg = write_tabs(t, &tabs);
        self.footer.tabs = tabs;
        msg
    }

    /// Same idea as `set_footer`, for the other two Header fields (displayId
    /// 2/3) -- generalizing the same "an overlay can cover any part of the
    /// screen, so recovery must redraw every displayId" lesson Footer
    /// taught, instead of waiting for a bug report against these two
    /// specifically.
    pub fn set_header_big_font(&mut self, text: String) -> Vec<u8> {
        let t = self.last_background.as_ref().map(|b| b.page_template()).unwrap_or(16);
        let msg = write_bigfont(t, &text);
        self.header.big_font = text;
        msg
    }

    pub fn set_header_current_value(&mut self, text: String) -> Vec<u8> {
        let t = self.last_background.as_ref().map(|b| b.page_template()).unwrap_or(16);
        let msg = write_current_value(t, &text);
        self.header.current_value = text;
        msg
    }

    /// Records what's already on the real device WITHOUT sending anything --
    /// for a caller that wrote a Background's content through some OTHER
    /// path (e.g. service.rs's raw per-field protocol at boot) and wants
    /// `hide_popup`/`hide_message` to have a correct restore target from the
    /// very first semantic command, instead of requiring an actual
    /// `switch_background` call first. A fresh `DeviceState` starts with
    /// `last_background: None`, and `hide_popup`/`hide_message` silently
    /// send nothing when there's nothing to restore to -- exactly the
    /// reported bug ("hiding popup does not really work") when the real
    /// device's boot state was never reflected here. Not a device read-back
    /// (none exists) -- purely "trust the caller, they just wrote this".
    pub fn seed_background(&mut self, bg: Background, title_bar: Vec<String>) {
        self.last_background = Some(bg);
        self.header.title_bar = title_bar;
        self.popup_visible = false;
        self.message_visible = false;
    }

    /// Same idea as `seed_background`, for a message already showing on the
    /// real device (e.g. the "hacpad" boot baseline) that this `DeviceState`
    /// never actually sent itself.
    pub fn seed_message_shown(&mut self, text: &str) {
        self.message_visible = true;
        self.last_message_text = Some(text.to_string());
    }

    /// Re-transmits the complete current Background (title_bar + all four
    /// Body fields) if one is known -- shared by `show_popup`/`show_message`
    /// AND `hide_popup`/`hide_message`.
    ///
    /// **Confirmed directly on hardware** (webcam, photographed): resending
    /// the SAME `page_template` value the device already has active --
    /// which is exactly what this restore always does, since it's
    /// re-sending `last_background` unchanged -- does NOT register as a
    /// real template switch. The Body content updates correctly underneath,
    /// but the popup overlay's own graphic layer (box border, `Esc`/`Enter`
    /// buttons, item list) stays ghosted on top of it, producing a garbled
    /// hybrid frame. Only bouncing through a genuinely DIFFERENT template
    /// value first, then back, actually tears the overlay down -- so this
    /// always inserts a `Blank` bounce before restoring. `switch_background`
    /// itself is untouched (already confirmed working with no bounce for a
    /// real caller-requested change to a different Background), this is
    /// specifically for the "restore what was already active" path.
    ///
    /// Still not perfect: even bounced, a residual pair of blank
    /// (unlabeled) button-shaped outlines was observed in the popup's old
    /// screen position afterward -- clearly better than the un-bounced
    /// garbled result, but not byte-for-byte identical to a Background that
    /// never had a popup opened on it. Recorded as open, not silently
    /// declared perfect.
    fn resend_body(&self) -> Vec<Vec<u8>> {
        match &self.last_background {
            Some(bg) => {
                // The bounce target's page_template must differ from bg's,
                // or the bounce is just as much a no-op as the restore it's
                // meant to fix -- Blank and Grid share template 5, so fall
                // back to Reset (template 0) in that one case.
                let bounce = if bg.page_template() == Background::Blank.page_template() {
                    Background::Reset
                } else {
                    Background::Blank
                };
                let mut msgs = self.full_redraw(&bounce, &self.header.title_bar);
                msgs.extend(self.full_redraw(bg, &self.header.title_bar));
                msgs
            }
            None => Vec::new(),
        }
    }

    /// Shows the popup -- draws on top of whatever Background is currently
    /// active without changing `last_background` at all (it's hardcoded to
    /// page_template=0 regardless, per `write_page_menu`'s doc comment).
    /// Also re-sends the full current Background first (see `resend_body`),
    /// so opening the popup doesn't depend on whatever was sent earlier
    /// having actually stuck.
    pub fn show_popup(&mut self, items: &[String]) -> Vec<Vec<u8>> {
        self.popup_visible = true;
        self.last_popup_items = items.to_vec();
        let mut msgs = self.resend_body();
        msgs.push(write_page_menu(items));
        msgs
    }

    /// Highlight is separate from show/hide -- a plain CC, only meaningful
    /// while the popup is actually showing.
    pub fn set_popup_highlight(&mut self, row: u8) -> [u8; 3] {
        self.last_popup_highlight = Some(row);
        cc_message(CC_MENU_HIGHLIGHT, row)
    }

    /// No-op (empty message vec) if already hidden or nothing to restore to
    /// yet -- there's no "clear the popup and show nothing," restoring
    /// `last_background` is the only dismiss mechanism that exists.
    pub fn hide_popup(&mut self) -> Vec<Vec<u8>> {
        if !self.popup_visible {
            return Vec::new();
        }
        self.popup_visible = false;
        self.message_visible = false; // a real switch clears this too
        self.resend_body()
    }

    /// Shows the message overlay -- independent of whatever Background is
    /// active (per direct request: "it may make sense to draw the message
    /// over a non-blank background"). Does NOT touch `last_background`, so
    /// `hide_message` restores whatever was really there, not a forced
    /// blank screen. Confirmed NOT to suppress an already-open popup
    /// (Thirty-first finding) -- the two can coexist, for better or worse.
    /// Also re-sends the full current Background first (see `resend_body`),
    /// same reasoning as `show_popup`.
    pub fn show_message(&mut self, text: &str) -> Vec<Vec<u8>> {
        self.message_visible = true;
        self.last_message_text = Some(text.to_string());
        let mut msgs = self.resend_body();
        msgs.push(write_message(text));
        msgs
    }

    /// Same no-op-if-already-hidden and "restore last_background" logic as
    /// `hide_popup` -- there is no dedicated message-clear primitive either.
    pub fn hide_message(&mut self) -> Vec<Vec<u8>> {
        if !self.message_visible {
            return Vec::new();
        }
        self.message_visible = false;
        self.popup_visible = false;
        self.resend_body()
    }
}

/// THE verb that changes which Background is active. Per the widget-model
/// doc, this is also the *only* way to dismiss Message or an open popup --
/// there is no separate "dismiss," dismissing IS switching. `title_bar` is
/// always resent here (chrome survives a switch untouched, so this is
/// purely to guarantee the switch actually reaches the wire: a page_template
/// change is a silent no-op unless piggybacked on some real field write --
/// confirmed directly, "Thirty-second finding" -- so Background variants
/// with no content fields of their own, like `Menu`/`Reset`, would otherwise
/// send nothing at all and the switch would never happen).
pub fn switch_background_messages(bg: &Background, title_bar: &[String]) -> Vec<Vec<u8>> {
    let t = bg.page_template();
    let body = bg.body_fields();
    // Body fields are nullable (see `BodyFields`'s doc comment): a variant
    // that doesn't define a given field omits its write entirely, trusting
    // the device's own confirmed auto-clear-on-genuine-switch behavior
    // (protocol.json: ctrl_element_name is confirmed `persists_across_
    // template_switch: false`; the other three are assumed the same by
    // analogy) instead of forcing an empty write ourselves.
    let mut msgs = vec![write_title_bar(t, title_bar)];
    if let Some(names) = &body.names {
        msgs.push(write_names(t, names));
    }
    if let Some(values) = &body.values {
        msgs.push(write_values(t, values));
    }
    if let Some(pad_state) = &body.pad_state {
        msgs.push(write_pad_state(t, pad_state));
    }
    if let Some(page_labels) = &body.page_labels {
        msgs.push(write_page_labels(t, page_labels));
    }
    msgs
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
    30, // Mute -- cursorTrack.getMute(), CONFIRMED from source: SAME CC drives both the input toggle
        // (`case CC.30: cursorTrack.getMute().toggle()`) and this LED feedback -- standard pattern,
        // found via a fuller re-grep of every literal sendChannelController call site.
    31, // Solo -- cursorTrack.getSolo(), same pattern as Mute (CC 30) above.
    25, // a genuine sendChannelController call site (tied to menuButtonLabel text), but NOT yet
        // characterized -- included here as a live-testable candidate, not a confirmed LED.
    // NOTE: 99/100/101/102 (the 4-LED status strip) are deliberately NOT in this list -- see
    // STATUS_LED_CCS/status_led_messages below. They are not 4 independent on/off bits like every
    // other entry here; folding them into this generic list would actively misbehave (see that doc
    // comment for why).
];

/// The 4-LED status strip above the screen -- ALL FOUR CCs CONFIRMED on hardware (Thirty-fifth
/// finding), each simultaneously the LED for one status position AND the input handler previously
/// tentatively labeled "browser_cancel_1/2/3"/"f_keys" in main.rs (same "one CC for input and LED"
/// pattern as everything else on this device). Index 0 = Status1 (CC 99, leftmost) ... index 3 =
/// Status4 (CC 102, rightmost).
///
/// **CONFIRMED HARDWARE MUTEX -- directly demonstrated, not assumed from testing each in isolation**:
/// set CC99=127 (position 1 lights), then set CC100=127 WITHOUT ever sending CC99=0 -- position 1 goes
/// dark and position 2 lights (confirmed for both an adjacent pair and a non-adjacent pair). **Further
/// confirmed**: sending 0 to ANY of these 4 CCs -- not just the one currently lit -- clears whichever
/// position is showing. So this is genuinely **one hardware register**, not 4 independent bits: "set
/// current status to position N" (write 127 to CC 98+N) and "clear" (write 0 to any of the 4), not
/// "toggle this specific LED". `led_cc_messages`'s generic "resend every LED_CCS entry, 127 or 0" loop
/// would misbehave applied to this group: whichever of these 4 CCs is iterated LAST with a 0 would
/// clear an already-applied 127 from earlier in the same batch, regardless of intent. Use
/// `status_led_messages` instead, which always clears first and sets second so the final state is
/// always correct regardless of prior state.
pub const STATUS_LED_CCS: &[u8] = &[99, 100, 101, 102];

/// `position`: `Some(1..=4)` to light that status position, `None` (or any other value) to clear all
/// four. Always emits a clear message first, then (if a valid position was given) the set message --
/// this ordering is what makes the result correct regardless of whatever was showing before, given the
/// confirmed "0 on any of the 4 clears the shared register" behavior documented above.
pub fn status_led_messages(position: Option<u8>) -> Vec<[u8; 3]> {
    let mut msgs = vec![cc_message(STATUS_LED_CCS[0], 0)]; // clear first, always
    if let Some(p @ 1..=4) = position {
        msgs.push(cc_message(STATUS_LED_CCS[(p - 1) as usize], 127));
    }
    msgs
}

/// One CC message per entry in `LED_CCS`, each set to on (127) if that CC
/// appears in `on`, else off (0) -- a full resync of every known LED rather
/// than a differential update, since there are only 18 of them and this way
/// there's no separate "which LEDs are currently lit" bookkeeping to drift.
pub fn led_cc_messages(on: &[u8]) -> Vec<[u8; 3]> {
    LED_CCS.iter().map(|&cc| cc_message(cc, if on.contains(&cc) { 127 } else { 0 })).collect()
}

// cursorTrack.getVolume() feedback -- NOT a simple on/off LED, unlike every
// entry in LED_CCS above. Confirmed from source (Thirty-third finding):
// `cursorTrack.getVolume().addValueObserver(1024, function(a){var b=a&7;
// a>>=3; sendChannelController(0,CC.15,b); sendChannelController(0,CC.47,a)})`
// -- a 0-1023 value split across two CCs (low 3 bits on CC 15, remaining 7
// bits on CC 47), almost certainly driving a segmented level-meter bargraph
// for the currently-focused track. Kept separate from LED_CCS/led_cc_messages
// since it isn't a plain on/off set.
pub const CC_CURSOR_VOLUME_LOW: u8 = 15;
pub const CC_CURSOR_VOLUME_HIGH: u8 = 47;

/// `value` is clamped to 0-1023 (the real observer's own scale, per
/// `addValueObserver(1024, ...)`) before being split the same way the driver
/// does: low 3 bits to CC 15, the rest to CC 47.
pub fn cursor_volume_messages(value: u16) -> [[u8; 3]; 2] {
    let v = value.min(1023);
    let low = (v & 7) as u8;
    let high = (v >> 3) as u8;
    [cc_message(CC_CURSOR_VOLUME_LOW, low), cc_message(CC_CURSOR_VOLUME_HIGH, high)]
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

// --- Device -> host: control input -----------------------------------------
// Moved here from src/main.rs (the original, confirmed-on-hardware home of
// this decoding) so it's shared library code, not a one-off demo-binary copy
// -- service.rs (the actual persistent bridge) had none of this until now,
// which was exactly the "we have NOT worked on the semantics for values
// flowing FROM the device" gap. See the "Device -> host: input semantics"
// section in research/panorama-p1-widget-model.md for what's confirmed vs.
// still tentative here (transport/nav CC-to-button assignments in
// particular are sourced from the Bitwig driver's JS, not all individually
// pressed-and-observed on this hardware).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CcKind {
    Fader,
    Encoder,
    Button,
    Unknown,
}

/// Confirmed against real hardware this session (fader/encoder/button CCs
/// verified live); transport/nav ordering within their ranges is still an
/// unverified guess in places (the reference only gave the range and named
/// functions, not the exact per-CC assignment) -- see the inline citations.
pub fn cc_name(cc: u8) -> String {
    match cc {
        0..=7 => format!("fader_{}", cc + 1),
        14 => "fader_master".to_string(),
        16..=23 => format!("select_{}", cc - 16 + 1),
        48..=55 => format!("pan_encoder_{}", cc - 48 + 1),
        64..=71 => format!("param_encoder_{}", cc - 64 + 1),
        // Transport row and friends -- CORRECTED AGAIN (Twenty-ninth finding): the first pass at
        // this (photo-of-finger-position based) turned out to have a one-step lag between the
        // physical press and the log line/photo landing during rapid sequential presses -- the
        // webcam attribution below was consistently off by one position. Re-derived from the
        // actual PANORAMA_P1.control.js CC enum + each case body's real Bitwig API call
        // (transport.play(), .stop(), .record(), .rewind(), .fastForward(), .toggleLoop()), which
        // is authoritative and not subject to that lag at all. Trust this over any earlier photo-
        // based guess for these six.
        80 => "loop".to_string(),    // transport.toggleLoop()
        81 => "rewind".to_string(),  // transport.rewind()
        82 => "forward".to_string(), // transport.fastForward()
        83 => "stop".to_string(),    // transport.stop() / transport.setPosition(0)
        84 => "play".to_string(),    // transport.play()
        85 => "record".to_string(),  // transport.record()
        86 => "loop_in".to_string(), // transport.getInPosition().set(...)
        87 => "loop_out".to_string(), // transport.getOutPosition().set(...)
        89 => "click".to_string(), // transport.toggleClick()/toggleMetronomeTicks()
        // 88, 90, 93-95, 97-98, 100-102, 104-105: resolved from a fuller re-grep of PANORAMA_P1.control.js
        // ("Thirty-third finding" in the protocol notes) -- the earlier pass's case-body grep missed these
        // because they span more of the minified line than that grep searched.
        88 => "undo_redo".to_string(), // Shift-gated: shift=application.redo(), plain=application.undo()
        // CORRECTED (Thirty-ninth finding): reported directly ("mode -> f-key, and so on"),
        // then confirmed with a deliberate two-button live test + webcam photo taken the
        // instant each CC arrived. Pressing the physical button silkscreened "Mode" (bottom-
        // right of the F-Keys/F1-F11 grid, next to Record) produced CC 90, not the
        // Bitwig-driver-sourced "overdub" this was previously guessed to be -- ruled out
        // "the finger was actually on Record" by elimination, since Record is independently
        // confirmed as CC 85 from source (Twenty-ninth finding) and did NOT fire here. The
        // retracted `Shift-gated overdub/automation-write` behavior may still be real -- it's
        // very plausibly what a Bitwig session specifically reassigns THIS SAME physical
        // button to do once its driver is running (Bitwig has no use for a raw "Mode" toggle
        // the way Nektar's own firmware does) -- but that's a DAW-side semantic layered on
        // top of the hardware identity, not the hardware identity itself, which is what this
        // name is for. Kept the old comment's source citation below for that reason.
        90 => "mode".to_string(), // silkscreened label; Bitwig driver source: Shift-gated shift=transport.toggleWriteArrangerAutomation() ("Automation:"), plain=transport.toggleOverdub() ("Overdub:") -- a DAW-side reassignment of this button, not its hardware identity
        // 93/94: both literally share ONE case body (`case CC.93: case CC.94: PATCH_PRESSED=0<e`) --
        // a fallthrough that just sets a shared "patch browsing" gate flag, not two separately-handled
        // buttons at this switch. Labeled patch_minus/patch_plus from the physical button row
        // (Shift/Track-/Track+/Patch-/Patch+/View, confirmed live via a webcam photo showing the
        // printed labels) -- matches an EARLIER, separate finding that attributed CC 94 to
        // application.zoomIn()/arrowKeyDown()/preset-scroll depending on Shift/browser state (that
        // logic lives elsewhere, likely reading PATCH_PRESSED alongside encoder direction, not at this
        // exact dispatch site) -- the two findings aren't fully reconciled yet, treat the exact
        // patch_minus-vs-patch_plus split as tentative.
        93 => "patch_minus".to_string(),
        94 => "patch_plus".to_string(),
        95 => "view".to_string(), // onView(), or (unshifted, some states) sends the 0x0B "Launcher" SysEx family documented elsewhere
        // RENAMED (Thirty-ninth finding, by elimination, not a fresh live+photo test):
        // the official Nektar Panorama P1 manual's own labeled diagram confirms this row of 6
        // physical buttons reads left-to-right as Shift/Track-/Track+/Patch-/Patch+/View.
        // 93/94/95/96 are independently confirmed as patch_minus/patch_plus/view/shift, which
        // leaves exactly these 2 CCs for the row's remaining 2 buttons -- Track-/Track+. Kept
        // the CC-ordering convention already established for Patch- < Patch+ (93 < 94) to pick
        // which of 91/92 is which; this specific pair hasn't been individually re-confirmed
        // with its own live press + photo the way 90/99/103 just were.
        91 => "track_minus".to_string(),
        92 => "track_plus".to_string(),
        // RETRACTED (Thirty-ninth finding): this was labeled "f_keys" on the theory that
        // pressing it opened a device-native F-Keys page -- but a deliberate, photographed
        // live test of the REAL physical F-Keys button (top-left of the transport grid)
        // produced CC 103, never CC 99. The device-native-page-switch behavior itself was real
        // and reproducible when this was first found -- only the attribution to "F-Keys"
        // specifically is now known wrong.
        //
        // A real lead, found elsewhere in THIS SAME file rather than guessed fresh: CC 99 is
        // also one of the four CONFIRMED status-LED-strip CCs (99-102, "Thirty-fifth finding",
        // see STATUS_LED_CCS below) -- Status1, leftmost of the strip above the screen. Per the
        // "one CC for input and LED" pattern already established for every other on/off button
        // on this device (select_N, menu_button_0/screen_button_N, etc.), CC 99 as an INPUT is
        // most likely whatever physical action lights that same Status1 LED position, not a
        // transport-grid button at all -- not yet isolated which physical control that is.
        99 => "status_led_1_or_unconfirmed".to_string(),
        // 100-102, 104: all resolved as browser/patch-menu "cancel"-shaped handlers (gBrowserOpen=false,
        // setActiveDisplayPage/SurfaceStatus changes) but not individually distinguished as specific
        // physical buttons yet -- kept generic and source-quoted rather than over-claiming a name.
        100 => "browser_cancel_1".to_string(), // gBrowserOpen=false; shift: application.createInstrumentTrack(-1)
        101 => "browser_cancel_2".to_string(), // gBrowserOpen=false; setActiveDisplayPage + nek_set_nektarine_instance_active(0)
        102 => "browser_cancel_3".to_string(), // gBrowserOpen=false; softTakeoverReset(); setActiveDisplayPage(internalPage)
        104 => "surface_status".to_string(), // SurfaceStatus/SURFACE.connected-state related -- plausibly not a normal user button, not yet confirmed live
        105 => "automation_write".to_string(), // transport.toggleWriteArrangerAutomation(), unconditional (unlike CC 90's Shift-gated version) -- likely the button whose LED is CC 29
        // Confirmed from PANORAMA_P1.control.js's onMidi CC dispatch (Z811481AF53E7994F1),
        // then verified live via the physical device (Twenty-eighth finding):
        96 => "shift".to_string(), // momentary; source sets a boolean gate flag on value>0/0
        // 97: CORRECTED -- source is `case CC.97: TOGGLE_MUTE_PRESSED=0<e` (a mode-gate flag), NOT the
        // jog wheel's push/click as previously guessed. Distinct from CC 30, which directly toggles
        // cursorTrack's mute AND drives its own LED -- CC 97 is plausibly a pad/drum-mode mute-select
        // button instead, not yet confirmed live which physical control this is.
        97 => "toggle_mute_pressed".to_string(),
        98 => "toggle_view_pressed".to_string(), // source: TOGGLE_VIEW_PRESSED=0<e; onToggleView() on press
        // CORRECTED (Thirty-ninth finding): confirmed live with a deliberate press + webcam
        // photo taken the instant the CC arrived -- the finger was clearly on the physical
        // "F-Keys" button (top-left of the transport grid, its own dedicated SHIFT per the
        // official Nektar Panorama P1 manual), not whatever this device-side source-comment
        // guess ("setActiveDisplayPage(internalPage)") had assumed. Swapped with CC 90 (see
        // its own comment above) -- the two were reversed.
        103 => "f_keys".to_string(),
        106 => "menu_button_0".to_string(), // 5th of the "menu buttons" LED range (106-110); not otherwise distinguished from 107-110
        107 => "screen_button_1".to_string(),
        108 => "screen_button_2".to_string(),
        109 => "screen_button_3_exit".to_string(), // confirmed live: closes the popup menu (onMenuCancel) when one is open
        110 => "menu_enter".to_string(), // confirmed live: onMenuEnter when a popup menu is open
        111 => "jog_wheel".to_string(), // confirmed live: relative encoder ticks; also reused as the popup-menu highlight-index CC in the output direction (Twenty-seventh finding)
        _ => format!("cc_{cc}"),
    }
}

pub fn cc_kind(cc: u8) -> CcKind {
    match cc {
        0..=7 | 14 => CcKind::Fader,
        48..=55 | 64..=71 | 111 => CcKind::Encoder,
        16..=23 | 80..=90 | 91..=103 | 106..=110 => CcKind::Button,
        _ => CcKind::Unknown,
    }
}

/// One entry in the canonical "which controls exist" list -- see
/// `known_input_controls`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownControl {
    pub cc: u8,
    pub name: String,
    pub kind: CcKind,
}

/// Every CC this project has assigned a real semantic name to (i.e. NOT
/// `cc_name`'s `cc_N` catch-all) -- the single source of truth for "which
/// controls exist," derived directly from `cc_name`/`cc_kind` rather than
/// hand-duplicated a second time by any client. Used to render one fixed
/// field per control ("just show ALL of them as fields... I do not care
/// much for the log style") instead of a dynamic list that only grows as
/// controls happen to get touched.
pub fn known_input_controls() -> Vec<KnownControl> {
    (0u8..=127)
        .filter_map(|cc| {
            let name = cc_name(cc);
            if name.starts_with("cc_") {
                None
            } else {
                Some(KnownControl { cc, name, kind: cc_kind(cc) })
            }
        })
        .collect()
}

/// A decoded device-originated control input -- semantically what the
/// physical control IS on the hardware itself (`fader_3`, `pan_encoder_5`,
/// `play`, `shift`, ...), deliberately NOT tied to whatever `Background` is
/// currently on screen. Confirmed shape for Fader/Encoder/Button live on
/// hardware; `Unknown` is deliberately a catch-all rather than a guess.
/// `#[serde(tag = "kind")]` gives e.g. `{"kind":"Fader","cc":3,
/// "name":"fader_4","value":85,"normalized":0.669...}` on the wire, so a
/// client gets a real structured event, not a Debug-formatted string.
/// Nothing downstream of this (a "which DAW parameter does fader_3 mean"
/// mapping layer, or feeding a value back into `DeviceState`) exists yet --
/// this is only the byte-level decode, on purpose: "I do not know yet if
/// it makes sense to map them to what's currently displayed on the
/// screen... perhaps just semantically what they ARE on the actual
/// hardware" -- direct instruction to keep this layer independent of
/// Background/DeviceState, not a gap to fill in.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum InputEvent {
    Fader { cc: u8, name: String, value: u8, normalized: f32 },
    Encoder { cc: u8, name: String, delta: i8 },
    Button { cc: u8, name: String, pressed: bool },
    Unknown { cc: u8, name: String, value: u8 },
}

impl InputEvent {
    /// The control's own name (`cc_name(cc)`) -- e.g. `"fader_4"`,
    /// `"play"` -- shared accessor so a caller doesn't need to match on
    /// every variant just to key a map by it (see service.rs's
    /// `last_input`).
    pub fn name(&self) -> &str {
        match self {
            InputEvent::Fader { name, .. }
            | InputEvent::Encoder { name, .. }
            | InputEvent::Button { name, .. }
            | InputEvent::Unknown { name, .. } => name,
        }
    }
}

pub fn decode_cc(cc: u8, value: u8) -> InputEvent {
    let name = cc_name(cc);
    match cc_kind(cc) {
        CcKind::Fader => InputEvent::Fader {
            cc,
            name,
            value,
            normalized: value as f32 / 127.0,
        },
        CcKind::Encoder => {
            // relative 2's-complement: 1..=63 = +delta, 65..=127 = -delta
            let delta: i8 = if value < 64 {
                value as i8
            } else {
                -(128 - value as i16) as i8
            };
            InputEvent::Encoder { cc, name, delta }
        }
        CcKind::Button => InputEvent::Button {
            cc,
            name,
            pressed: value == 127,
        },
        CcKind::Unknown => InputEvent::Unknown { cc, name, value },
    }
}

/// One semantic input control's tracked state -- the mapping from raw CC to
/// semantic identity (`decode_cc`) is step one; this is step two, "remember
/// what value each semantic input's last value is."
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputControlState {
    /// The most recent decoded event for this control, verbatim.
    pub last_event: Option<InputEvent>,
    /// A single 0..=127 "current value" for this control, so a caller
    /// doesn't have to match on `last_event`'s kind just to get a number:
    /// Fader mirrors the raw absolute value; Button is 127 while pressed,
    /// 0 while released (matching the raw CC convention); Encoder has no
    /// absolute value of its own on the wire (relative 2's-complement
    /// deltas) -- this is `InputState`'s OWN running accumulation of every
    /// delta seen so far, clamped to 0..=127, a soft position estimate we
    /// invent, not a hardware fact (there is no read-back for input either,
    /// same honesty as `DeviceState`).
    pub value: u8,
}

/// Device -> host state: the last known value/event for every semantic
/// input control, keyed by its own hardware identity (`cc_name()` --
/// `"fader_3"`, `"shift"`, `"pan_encoder_5"`, ...). Mirrors `DeviceState`'s
/// role on the output side -- both are "there's no read-back, so this
/// struct IS the current state" -- but deliberately kept independent of
/// `DeviceState`/`Background`: "I do not know yet if it makes sense to map
/// them to what's currently displayed on the screen... perhaps just
/// semantically what they ARE on the actual hardware." No coupling exists,
/// and none is assumed.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct InputState {
    controls: HashMap<String, InputControlState>,
}

impl InputState {
    /// Decodes one raw CC message, updates that control's tracked state,
    /// and returns the decoded event (same value `decode_cc` alone would
    /// give -- this just also remembers it).
    pub fn observe(&mut self, cc: u8, value: u8) -> InputEvent {
        let event = decode_cc(cc, value);
        let entry = self.controls.entry(event.name().to_string()).or_default();
        match &event {
            InputEvent::Fader { value, .. } => entry.value = *value,
            InputEvent::Button { pressed, .. } => entry.value = if *pressed { 127 } else { 0 },
            InputEvent::Encoder { delta, .. } => {
                entry.value = (entry.value as i16 + *delta as i16).clamp(0, 127) as u8;
            }
            InputEvent::Unknown { value, .. } => entry.value = *value,
        }
        entry.last_event = Some(event.clone());
        event
    }

    /// The tracked state for one control, if anything has been observed for
    /// it yet this session.
    pub fn get(&self, name: &str) -> Option<&InputControlState> {
        self.controls.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ack_matches_captured_hardware_reply() {
        let sent = sysex(&INIT_2);
        let ack = expected_ack(&sent).expect("INIT_2 is a 0x09-family message");
        // The literal bytes captured live from the device (see the ACK test
        // in the protocol notes) -- built by hand, NOT via sysex(), since
        // that helper always emits the host-to-device prefix (sub-id 0x01)
        // and this is a device-to-host reply (sub-id 0x02).
        let captured: Vec<u8> = vec![0xF0, 0x00, 0x01, 0x77, 0x7F, 0x02, 0x09, 0x03, 0x00, 0x00, 0x01, 0x3E, 0x33, 0xF7];
        assert_eq!(ack, captured, "expected_ack(INIT_2) must match the live-captured device reply");
    }

    #[test]
    fn no_ack_expected_for_0x08_or_0x06() {
        assert!(expected_ack(&sysex(&INIT_1)).is_none());
        assert!(expected_ack(&write_bigfont(16, "X")).is_none());
    }

    /// Reproduces the reported bug ("hiding popup does not really work"):
    /// `hide_popup`/`hide_message` restore `last_background`, but a fresh
    /// `DeviceState` starts with `last_background: None` -- so calling
    /// `show_popup` before ANY `switch_background` leaves nothing to
    /// restore, and `hide_popup` silently sends zero bytes even though it
    /// still (incorrectly, from the caller's perspective) reports
    /// `popup_visible() == false` afterward. This is exactly the trap
    /// service.rs fell into: it never seeded `device_state` at boot, so any
    /// semantic popup/message shown before the first semantic
    /// `switchBackground` command was permanently stuck -- fixed there by
    /// seeding `last_background` at startup to match what was actually
    /// written to the real device.
    #[test]
    fn hide_popup_is_a_silent_noop_without_a_prior_switch_background() {
        let mut state = DeviceState::default();
        state.show_popup(&["A".to_string(), "B".to_string()]);
        assert!(state.popup_visible());
        let msgs = state.hide_popup();
        assert!(msgs.is_empty(), "no last_background to restore to -- nothing can be sent");
        // The flag flips anyway, which is the misleading part: a caller
        // reading popup_visible() alone would believe the dismiss worked.
        assert!(!state.popup_visible());
    }

    /// Direct instruction after two live reports that show/hide was still
    /// broken from the browser: stop hand-picking which 1-2 fields a given
    /// `Background` variant "should" need and always transmit all four Body
    /// writes (names/values/pad_state/page_labels) on every switch. Checks
    /// this for a variant with a rich Body schema (`Mixer`) AND for one with
    /// NONE at all (`Menu`) -- the latter previously sent title_bar alone.
    #[test]
    fn switch_background_omits_body_fields_a_variant_doesnt_define() {
        let title_bar = vec!["T1".to_string(), "T2".to_string(), "T3".to_string()];

        // Mixer defines names+values only (no pad_state/page_labels) --
        // title_bar + 2, not title_bar + all 4. Nullable per BodyFields'
        // doc comment: omitting an unused field is safe because a genuine
        // template switch auto-clears ctrl_element_name (protocol.json,
        // confirmed) and the rest are assumed the same by analogy.
        let mixer = Background::Mixer {
            param_names: std::array::from_fn(|i| format!("N{i}")),
            param_values: std::array::from_fn(|i| format!("V{i}")),
        };
        assert_eq!(switch_background_messages(&mixer, &title_bar).len(), 3, "title_bar + names + values only");

        // Menu/Reset/Blank define NO Body fields at all -- title_bar alone.
        assert_eq!(switch_background_messages(&Background::Menu, &title_bar).len(), 1);
        assert_eq!(switch_background_messages(&Background::Reset, &title_bar).len(), 1);
        assert_eq!(switch_background_messages(&Background::Blank, &title_bar).len(), 1);

        // PadView defines names+pad_state (no values/page_labels) -- a
        // different 2-of-4 subset, confirming this isn't hardcoded to
        // Mixer's particular pair.
        let pad_view = Background::PadView {
            pad_names: std::array::from_fn(|i| format!("P{i}")),
            pad_states: [PadState::Default; 16],
        };
        assert_eq!(switch_background_messages(&pad_view, &title_bar).len(), 3, "title_bar + names + pad_state only");
    }

    /// show_popup/show_message/hide_popup/hide_message must ALL resend the
    /// complete Body state (via `resend_body`), not just fire their own
    /// specific write -- direct instruction ("including when showing/hiding
    /// the menu and messages"). Confirms each returns the full 5-message
    /// switch_background_messages() bundle plus its own extra write (or, for
    /// hide_*, exactly that bundle).
    #[test]
    fn show_and_hide_overlay_verbs_resend_the_full_body_state() {
        let mut state = DeviceState::default();
        let bg = Background::Mixer {
            param_names: std::array::from_fn(|i| format!("N{i}")),
            param_values: std::array::from_fn(|i| format!("V{i}")),
        };
        state.switch_background(bg, vec!["T1".into(), "T2".into(), "T3".into()]);

        // resend_body() bounces through a different template first (see its
        // doc comment) via full_redraw, which is: switch_background_messages
        // (title_bar + only the Body fields this variant actually defines)
        // + tabs + bigfont + current_value. Bounce target is Blank (no Body
        // fields at all): 1 + 3 = 4. Real target is Mixer (names+values
        // only, no pad_state/page_labels): 3 + 3 = 6. Total resend_body =
        // 4 + 6 = 10, plus the verb's own extra write for show_popup/
        // show_message.
        let show_popup_msgs = state.show_popup(&["A".to_string()]);
        assert_eq!(show_popup_msgs.len(), 11, "10 bounce+restore messages + write_page_menu");

        let hide_popup_msgs = state.hide_popup();
        assert_eq!(hide_popup_msgs.len(), 10, "just the bounce+restore resend");

        let show_message_msgs = state.show_message("hi");
        assert_eq!(show_message_msgs.len(), 11, "10 bounce+restore messages + write_message");

        let hide_message_msgs = state.hide_message();
        assert_eq!(hide_message_msgs.len(), 10, "just the bounce+restore resend");
    }

    /// The bounce target itself must never equal the real target's own
    /// template value -- `Blank` and `Grid` share page_template 5, so
    /// bouncing FROM/TO either of those must use `Reset` (0) instead, or
    /// the bounce would be exactly as much a no-op as the bug it exists to
    /// fix.
    #[test]
    fn resend_body_bounce_target_never_matches_the_real_background() {
        let mut state = DeviceState::default();
        state.switch_background(Background::Blank, vec!["T1".into(), "T2".into(), "T3".into()]);
        state.show_popup(&["A".to_string()]);
        let msgs = state.hide_popup();
        // First message is the bounce's title_bar write -- byte index 7 is
        // page_template (SYSEX_PREFIX is 6 bytes, then CMD_WRITE_DISPLAY,
        // then page_template -- see compose_write). Must be Reset's 0, not
        // Blank/Grid's shared 5.
        assert_eq!(msgs[0][7], 0, "bounce target must be Reset (0) when restoring Blank (page_template 5)");
    }

    /// Direct answer to "does that mean we also have to resend the footer?":
    /// yes -- confirmed live (webcam: sending CC 109, the input CC a popup's
    /// own Esc-button press reports, revealed stale "Tab4" text underneath
    /// it) that the popup's overlay chrome shares the SAME Footer slot
    /// (displayId 4) `switch_background_messages` never touched. Checks
    /// that `switch_background`/`set_tabs` actually emit a real,
    /// non-filtered `write_tabs` entry (not an empty/no-op one -- see
    /// `indexed_entries`' `!s.is_empty()` filter) using the tracked
    /// `last_tabs`, and that `resend_body` includes it on BOTH the bounce
    /// and the real restore.
    #[test]
    fn switch_background_and_resend_body_both_include_a_real_tabs_write() {
        let mut state = DeviceState::default();
        let bg = Background::Mixer {
            param_names: std::array::from_fn(|i| format!("N{i}")),
            param_values: std::array::from_fn(|i| format!("V{i}")),
        };
        let switch_msgs = state.switch_background(bg, vec!["T1".into(), "T2".into(), "T3".into()]);
        // full_redraw appends tabs/bigfont/current_value in that order after
        // switch_background_messages' own writes -- tabs is no longer
        // necessarily last (current_value is), so find it by displayId.
        let tabs_msg = switch_msgs.iter().find(|m| m[8] == DISPLAY_ID_MENU_BUTTON).expect("switch_background must include a tabs write");
        // Default last_tabs is 5 non-empty " " placeholders -- confirm at
        // least one indexed entry actually made it into the message (i.e.
        // indexed_entries did NOT filter everything out as empty).
        assert!(tabs_msg.len() > 10, "a genuinely blank (filtered-out) tabs write would be much shorter than one with 5 real entries");

        state.show_popup(&["A".to_string()]);
        let hide_msgs = state.hide_popup();
        let tabs_writes = hide_msgs.iter().filter(|m| m[8] == DISPLAY_ID_MENU_BUTTON).count();
        assert_eq!(tabs_writes, 2, "one tabs write for the bounce template, one for the real restore");
    }

    /// Device -> host semantic identity, spot-checked against CCs this
    /// project directly confirmed live on hardware earlier in the session
    /// (see cc_name's own inline citations) -- not the whole map, just
    /// enough to catch a decode_cc/cc_kind regression, same spirit as the
    /// ACK test above checking real captured bytes rather than trusting the
    /// implementation to keep matching itself.
    #[test]
    fn decode_cc_matches_confirmed_hardware_identities() {
        // Fader 1 (CC 0), absolute value -- confirmed live.
        match decode_cc(0, 100) {
            InputEvent::Fader { name, value, normalized, .. } => {
                assert_eq!(name, "fader_1");
                assert_eq!(value, 100);
                assert!((normalized - 100.0 / 127.0).abs() < 0.001);
            }
            other => panic!("expected Fader, got {other:?}"),
        }

        // Pan encoder 1 (CC 48), relative 2's-complement -- confirmed live
        // ("Own capture confirms CC 48-55 = pan/rotary encoders").
        match decode_cc(48, 5) {
            InputEvent::Encoder { name, delta, .. } => {
                assert_eq!(name, "pan_encoder_1");
                assert_eq!(delta, 5, "values 1..=63 decode as a positive delta");
            }
            other => panic!("expected Encoder, got {other:?}"),
        }
        match decode_cc(48, 127) {
            InputEvent::Encoder { delta, .. } => assert_eq!(delta, -1, "127 decodes as -1 (2's-complement wraparound)"),
            other => panic!("expected Encoder, got {other:?}"),
        }

        // Shift (CC 96), a momentary button -- confirmed live (Twenty-eighth
        // finding).
        match decode_cc(96, 127) {
            InputEvent::Button { name, pressed, .. } => {
                assert_eq!(name, "shift");
                assert!(pressed);
            }
            other => panic!("expected Button, got {other:?}"),
        }
        match decode_cc(96, 0) {
            InputEvent::Button { pressed, .. } => assert!(!pressed),
            other => panic!("expected Button, got {other:?}"),
        }

        // Play (CC 84) -- confirmed from source + physical relabel, not
        // guessed (Twenty-ninth finding corrected an earlier photo-lag
        // misattribution here specifically).
        match decode_cc(84, 127) {
            InputEvent::Button { name, .. } => assert_eq!(name, "play"),
            other => panic!("expected Button, got {other:?}"),
        }
    }

    /// Regression test for the Thirty-ninth finding: CC 90 and CC 103 were
    /// reversed (labeled "overdub"/"mode" respectively, should be "mode"/
    /// "f_keys") -- confirmed with a deliberate two-button live test, each
    /// photographed the instant its CC arrived. Locks in the swap so it
    /// can't silently regress back to the old (wrong) names.
    #[test]
    fn cc_90_and_103_are_mode_and_f_keys_not_reversed() {
        match decode_cc(90, 127) {
            InputEvent::Button { name, .. } => assert_eq!(name, "mode", "CC 90 is the physical Mode button"),
            other => panic!("expected Button, got {other:?}"),
        }
        match decode_cc(103, 127) {
            InputEvent::Button { name, .. } => assert_eq!(name, "f_keys", "CC 103 is the physical F-Keys button"),
            other => panic!("expected Button, got {other:?}"),
        }
    }

    /// The wire shape a client actually receives -- `#[serde(tag = "kind")]`
    /// means a real client (index.html) can dispatch on `.kind` directly
    /// instead of getting a Debug-formatted string (what service.rs did
    /// before this).
    #[test]
    fn input_event_serializes_as_a_tagged_json_object() {
        let event = decode_cc(0, 100);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "Fader");
        assert_eq!(json["name"], "fader_1");
        assert_eq!(json["value"], 100);
    }

    #[test]
    fn input_state_tracks_fader_and_button_as_their_raw_value() {
        let mut state = InputState::default();
        state.observe(0, 42); // fader_1
        assert_eq!(state.get("fader_1").unwrap().value, 42);

        state.observe(96, 127); // shift, pressed
        assert_eq!(state.get("shift").unwrap().value, 127);
        state.observe(96, 0); // shift, released
        assert_eq!(state.get("shift").unwrap().value, 0);

        assert!(state.get("never_touched").is_none());
    }

    /// Encoders have no absolute value on the wire (relative 2's-complement
    /// deltas) -- `InputState` invents one by accumulating every delta seen,
    /// clamped to 0..=127 so it can't run away in either direction.
    #[test]
    fn input_state_accumulates_encoder_deltas_clamped_to_0_127() {
        let mut state = InputState::default();
        state.observe(48, 10); // pan_encoder_1, +10
        assert_eq!(state.get("pan_encoder_1").unwrap().value, 10);
        state.observe(48, 5); // +5 -> 15
        assert_eq!(state.get("pan_encoder_1").unwrap().value, 15);
        state.observe(48, 127); // -1 -> 14
        assert_eq!(state.get("pan_encoder_1").unwrap().value, 14);

        // Drive it below 0 and above 127 -- must clamp, not wrap or panic.
        for _ in 0..20 {
            state.observe(48, 127); // -1 each time
        }
        assert_eq!(state.get("pan_encoder_1").unwrap().value, 0, "clamped at the floor");
        for _ in 0..200 {
            state.observe(48, 1); // +1 each time
        }
        assert_eq!(state.get("pan_encoder_1").unwrap().value, 127, "clamped at the ceiling");
    }

    #[test]
    fn input_state_serializes_transparently_as_a_flat_map() {
        let mut state = InputState::default();
        state.observe(0, 42);
        let json = serde_json::to_value(&state).unwrap();
        // #[serde(transparent)] -- no wrapping "controls" key, matches the
        // existing inputSnapshot wire shape service.rs already sends.
        assert_eq!(json["fader_1"]["value"], 42);
        assert_eq!(json["fader_1"]["lastEvent"]["kind"], "Fader");
        assert!(json.get("controls").is_none());
    }

    #[test]
    fn known_input_controls_excludes_the_unnamed_catch_all_and_includes_confirmed_ones() {
        let controls = known_input_controls();
        assert!(controls.iter().all(|c| !c.name.starts_with("cc_")), "every entry must have a real name, not the cc_N fallback");
        let names: Vec<&str> = controls.iter().map(|c| c.name.as_str()).collect();
        for expected in ["fader_1", "fader_master", "pan_encoder_1", "param_encoder_1", "shift", "play", "jog_wheel"] {
            assert!(names.contains(&expected), "expected {expected:?} in known_input_controls()");
        }
        // CC 12/13/15 etc. (truly unassigned) must NOT appear.
        assert!(!controls.iter().any(|c| c.cc == 12));
    }
}
