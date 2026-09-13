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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone)]
pub enum Background {
    /// 4x2 knob grid (the default page on connect).
    Mixer { param_names: [String; 8], param_values: [String; 8] },
    /// 8 faders split into two groups of 4; 16 labels, two stacked per fader.
    FaderSplit { labels: [String; 16] },
    /// 8 faders in one continuous row.
    FaderRow { labels: [String; 8] },
    /// 4x4 pad grid, rows A-D, all 16 pads individually labelable.
    PadView { pad_names: [String; 16], pad_states: [PadState; 16] },
    /// 3x4 pad grid, rows A-C only -- genuinely distinct from PadView, not
    /// just an unlabeled 4th row (Sixteenth finding).
    PadView3Row { pad_names: [String; 12], pad_states: [PadState; 12] },
    /// 1 fader + a vertical bulleted list, hard-capped at 5 visible entries.
    List { items: [String; 5] },
    /// Plain 2x4 button grid, no fader/knob widgets.
    Grid { labels: [String; 8] },
    /// `L:`/`R:` locator bars + a 2x4 grid beneath.
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
}

/// Tracks what's actually been sent, so `hide_popup`/`hide_message` can
/// restore the right thing -- see the widget-model doc's "trigger-only, no
/// clear primitive" class. There is no query/read-back on this device
/// (confirmed repeatedly throughout this project), so this struct IS the
/// only place "current state" exists at all; it's a plain in-memory model,
/// not derived from the device.
pub struct DeviceState {
    last_background: Option<Background>,
    last_title_bar: Vec<String>,
    popup_visible: bool,
    message_visible: bool,
}

impl Default for DeviceState {
    fn default() -> Self {
        DeviceState { last_background: None, last_title_bar: Vec::new(), popup_visible: false, message_visible: false }
    }
}

impl DeviceState {
    pub fn popup_visible(&self) -> bool {
        self.popup_visible
    }

    pub fn message_visible(&self) -> bool {
        self.message_visible
    }

    /// THE verb that changes which Background is active -- also the only
    /// way to dismiss Message or an open popup, since both get marked
    /// hidden here (a real switch clears them as a side effect, confirmed
    /// -- Thirteenth/Thirty-first findings).
    pub fn switch_background(&mut self, bg: Background, title_bar: Vec<String>) -> Vec<Vec<u8>> {
        let msgs = switch_background_messages(&bg, &title_bar);
        self.last_background = Some(bg);
        self.last_title_bar = title_bar;
        self.popup_visible = false;
        self.message_visible = false;
        msgs
    }

    /// Shows the popup -- draws on top of whatever Background is currently
    /// active without changing `last_background` at all (it's hardcoded to
    /// page_template=0 regardless, per `write_page_menu`'s doc comment).
    pub fn show_popup(&mut self, items: &[String]) -> Vec<u8> {
        self.popup_visible = true;
        write_page_menu(items)
    }

    /// Highlight is separate from show/hide -- a plain CC, only meaningful
    /// while the popup is actually showing.
    pub fn set_popup_highlight(&self, row: u8) -> [u8; 3] {
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
        match self.last_background.clone() {
            Some(bg) => switch_background_messages(&bg, &self.last_title_bar),
            None => Vec::new(),
        }
    }

    /// Shows the message overlay -- independent of whatever Background is
    /// active (per direct request: "it may make sense to draw the message
    /// over a non-blank background"). Does NOT touch `last_background`, so
    /// `hide_message` restores whatever was really there, not a forced
    /// blank screen. Confirmed NOT to suppress an already-open popup
    /// (Thirty-first finding) -- the two can coexist, for better or worse.
    pub fn show_message(&mut self, text: &str) -> Vec<u8> {
        self.message_visible = true;
        write_message(text)
    }

    /// Same no-op-if-already-hidden and "restore last_background" logic as
    /// `hide_popup` -- there is no dedicated message-clear primitive either.
    pub fn hide_message(&mut self) -> Vec<Vec<u8>> {
        if !self.message_visible {
            return Vec::new();
        }
        self.message_visible = false;
        self.popup_visible = false;
        match self.last_background.clone() {
            Some(bg) => switch_background_messages(&bg, &self.last_title_bar),
            None => Vec::new(),
        }
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
    let mut msgs = vec![write_title_bar(t, title_bar)];
    match bg {
        Background::Mixer { param_names, param_values } => {
            msgs.push(write_names(t, param_names));
            msgs.push(write_values(t, param_values));
        }
        Background::FaderSplit { labels } => msgs.push(write_names(t, labels)),
        Background::FaderRow { labels } => msgs.push(write_names(t, labels)),
        Background::Grid { labels } => msgs.push(write_names(t, labels)),
        Background::PadView { pad_names, pad_states } => {
            msgs.push(write_names(t, pad_names));
            msgs.push(write_pad_state(t, &pad_states.iter().map(|s| s.as_byte()).collect::<Vec<_>>()));
        }
        Background::PadView3Row { pad_names, pad_states } => {
            msgs.push(write_names(t, pad_names));
            msgs.push(write_pad_state(t, &pad_states.iter().map(|s| s.as_byte()).collect::<Vec<_>>()));
        }
        Background::List { items } => msgs.push(write_names(t, items)),
        Background::TransportLauncher { loop_left, loop_right, labels } => {
            msgs.push(write_page_labels(t, &[loop_left.clone(), loop_right.clone()]));
            msgs.push(write_names(t, labels));
        }
        Background::Menu | Background::Reset | Background::Blank => {} // no content schema (Blank deliberately) -- title_bar alone still performs the switch
        Background::ListHighlighted { items, .. } => msgs.push(write_names(t, items)),
        Background::SceneButtons { labels } => msgs.push(write_names(t, labels)),
        Background::BrowserList { items } => msgs.push(write_names(t, items)),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        90 => "overdub".to_string(), // Shift-gated: shift=transport.toggleWriteArrangerAutomation() ("Automation:"), plain=transport.toggleOverdub() ("Overdub:")
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
        91 | 92 => format!("nav_{}", cc - 91 + 1), // still not individually resolved from source
        99 => "f_keys".to_string(), // confirmed live: opens a distinct, DEVICE-NATIVE "F-KEYS" page (F1-F11/P5/P11 grid) -- rendered by the P1 itself, not by anything we (or a DAW driver) send over SysEx; source's handler just does setActiveDisplayPage/gBrowserOpen bookkeeping on the Bitwig-driver side, which isn't even running in our setup. Confirmed momentary (releases when the button is released). The P1 also exposes a genuine USB HID keyboard interface (class 3, standard boot-keyboard report descriptor, separate from MIDI) -- plausibly what "F-Keys" actually drives, but no HID report was captured yet to confirm the link empirically.
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
        103 => "mode".to_string(), // source: setActiveDisplayPage(internalPage) on press
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

/// A decoded device-originated control input. Confirmed shape for Fader/
/// Encoder/Button live on hardware; `Unknown` is deliberately a catch-all
/// rather than a guess. Nothing downstream of this (a "which DAW parameter
/// does fader_3 mean" mapping layer, or feeding a value back into
/// `DeviceState`) exists yet -- this is only the byte-level decode.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Fader { cc: u8, name: String, value: u8, normalized: f32 },
    Encoder { cc: u8, name: String, delta: i8 },
    Button { cc: u8, name: String, pressed: bool },
    Unknown { cc: u8, name: String, value: u8 },
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
}
