# Panorama P1 widget model — working document

Distinct from `panorama-p1-protocol-notes.md` (narrative findings, chronological) and
`protocol.json` (structured byte-level facts): this file is the **model** we're building
*from* those findings, built by re-reading what's already established rather than running new
hardware tests — nearly every cell of the matrix below turned out to already have direct
evidence somewhere in the findings log; the gap was synthesis, not data. Started because
repeated ad-hoc testing kept re-deriving pieces of this model instead of building on a shared
one.

## Two orthogonal axes

Every addressable "thing" on this device sits somewhere on two independent axes: **which
layer** it belongs to, and **what kind of mutability** it has.

### Axis 1: layer

| Layer | Members | Behavior |
|---|---|---|
| **Header** (top strip) | `title_bar`(1), `big_font`(2), `current_value`(3) | State-class, template-independent. Survives a Background switch untouched (Fourteenth finding), confirmed unaffected across **every** Background tested (Tenth finding's per-template probes). |
| **Footer** (bottom strip) | `menu_button`(4) | Same State-class persistence as Header, kept as its own row since it's a physically separate strip (bottom of screen vs. top) that happens to share Header's mutability — "Chrome" as a single lumped category was conflating position with mutability class; split per direct request. |
| **Background** | `page_template`: see the full table below | Mutually exclusive — exactly one is "active." Switching to a different one clears Body, whether or not the switching write touches it (Fourteenth finding). |
| **Body** (the Background's own content area) | `ctrl_element_name`(6), `ctrl_element_value`(7), `pad_state`(0), `page_labels`(5) | Content-class: cleared by any real Background switch. Rendered differently by every Background (that's *what makes* a Background visually distinct) — see table. `page_labels` renders "near the first knob position" (Nineteenth finding), i.e. inside the Body area, not Header/Footer, despite being chrome-*adjacent* in earlier framing. |
| **Overlay** | popup menu (displayId 8, hardcoded `page_template=0`) | Draws on top of whatever Background+Header+Footer+Body is showing, without changing which Background is "official." Confirmed to overlay `drum_pads`(21) cleanly without disturbing it (Twenty-seventh finding). **Its own `Esc`/`Enter` buttons render through the Footer's own displayId (4, `menu_button`) rather than an independent slot** (Thirty-eighth finding) -- so Overlay isn't purely additive on top of Footer, it temporarily *owns* it, and dismissing the popup requires re-asserting Footer content, not just Background/Body. |
| **Message** (exclusive mode) | `pageTemplate=1` one-shot write | Own separate SysEx pathway (Seventh finding), not a compose-path Background at all. Used dozens of times this whole project as the "force back to a clean state" trick, successfully, from every kind of prior state — confirms Message suppresses Header/Footer/Background/Body regardless of what preceded it. **Its interaction with an open Overlay is unsettled, contradictory evidence**: the Thirty-first finding saw the popup's highlighted bars still drawn on top of "hacpad" text (Message *below* Overlay in z-order); a later, cleaner test (`verb_test overlaytest`: switch_background → show_popup → show_message, in that exact order) showed Message rendering **completely alone**, no popup bleed-through at all. Not reconciled — may be order/prior-state-dependent (the earlier test's popup may have been "stale" from much earlier in that session rather than freshly shown right before Message, unlike the later test). Treat as open, not settled either direction. |

**Background table** (all confirmed on hardware, Tenth/Fifteenth/Sixteenth findings):

| id | Content widget shape | Chrome behavior |
|---|---|---|
| 0 | reset sentinel — ignores Content writes entirely, falls back to native default fader view | title bar still works |
| 2 | `menu` — body untested directly, but displayId 4 (menu_button) confirmed to relabel independent of it | title bar untouched |
| 3 | `transport_launcher` — `L:`/`R:` locator bars + a 2×4 grid of `ctrl_element_name` | title bar untouched |
| 4 | `list` — 1 fader + vertical bulleted list (5 visible entries, hard cap) | title bar untouched |
| 5 | `grid5` — plain 2×4 button grid, no fader/knob widgets | title bar untouched |
| 6 | `list_highlighted` — list with one row shown selected/highlighted, row numbers on the left | title bar untouched |
| 7 | `scene_buttons` — 4 buttons `S1`-`S4` + a `B` indicator | title bar untouched |
| 8/9 | `browser_list` — vertical list of up to 8 rows, each paired with a `Pre` label; 8 and 9 render indistinguishably | title bar untouched |
| 16/17 | `mixer` — 4×2 knob grid (default page on connect); 16/17 indistinguishable | title bar untouched |
| 18 | `instrument_layer_container` — 8 faders split into two groups of 4, 16 `ctrl_element_name` slots (2 stacked per fader) | title bar untouched |
| 19/20 | `faders_row` — 8 faders, one continuous row; 19/20 indistinguishable | title bar untouched |
| 21 | `drum_pads` — 4×4 pad grid, rows A-D, all 16 individually labelable | title bar untouched |
| 22 | `drum_pads_3row` — genuinely distinct from 21 (Sixteenth finding): 3×4 grid, rows A-C only, no 4th row at all | title bar untouched |

So the Chrome × Background and Content × Background interactions are **fully characterized**
— every Background was individually probed and Chrome was confirmed identical every single
time. What was never tabulated as a matrix is Overlay × Background and Message × Background.

### Axis 2: mutability class

| Class | Members | What "unset" means | How to actually clear it |
|---|---|---|---|
| **1. State** | Chrome, Content, independent LEDs (select/menu buttons, Play/Record/Loop/Mute/Solo/etc.) | A real, stable "current value" exists on the device. Not sending it leaves it as-is. Idempotent. | Send the new value. |
| **2. Select-or-clear register** | Status LED strip (CC 99-102, Thirty-fifth finding) | One of N positions active, or none. A genuine clear primitive exists. | Write 127 to the CC for the desired position, or 0 to *any* of the group's CCs to clear all. **Must be sent as clear-then-set as one atomic unit** — diffing per-CC independently breaks it. |
| **3. Trigger-only, no clear primitive** | Message, popup menu (Overlay layer), popup highlight (CC 111) | No device-tracked "current value" to diff against. Every send is a fresh one-shot "show this now." | **No targeted clear exists.** Only: (a) never send it, or (b) a real Background switch, which resets Background+Content (and, per the z-order finding above, Message — but not a currently-open popup, which needs its own switch too since it's a *different* trigger). The highlight is the same story one level down: confirmed directly (Thirty-seventh finding, protocol notes) that 0/127/255 all leave the last-set row highlighted rather than clearing it — dismissing the *whole popup* is the only way to make a highlight disappear, there's no way to un-highlight a row while leaving the popup open. |

**The correction this document exists to capture**: the simulator's "differential push"
architecture (diff against last-sent value, resend only on change) implicitly treats
everything as Class 1. `dismissOverlay()`, as originally built, doesn't hide an overlay — it
fires the *Background-switch verb* (see below) and the overlay vanishing is a side effect of
that, not a targeted "hide."

## Verb layer: confirmed on hardware

`switch_background_messages()` (lib.rs) and its `Background` enum are no longer just a
design on paper — three variants tested directly against real hardware via
`src/bin/verb_test.rs` and photographed (after fixing an unrelated camera-tuning problem
that was initially misread as a rendering bug — the webcam's exposure was clipping the
lighter end of the pad grid's own brightness gradient to solid white; resolved with
live manual tuning via the new camera-control panel, not a device/protocol issue):

- **`Mixer`**: 8 knobs, `param_names`/`param_values` both rendered correctly
  (CUT/RES/.../MIX with 1.2k/45%/.../wet).
- **`PadView`**: all 16 pad slots individually labeled correctly (P1-P16, right rows:
  A=1-4, B=5-8, C=9-12, D=13-16).
- **`TransportLauncher`**: `loop_left`/`loop_right` rendered as the `L:`/`R:` bars,
  `labels` rendered as the 2×4 grid beneath.

Not yet exercised: `FaderSplit`, `FaderRow`, `List`, `Grid`, `PadView3Row`, `Menu`,
`ListHighlighted`, `SceneButtons`, `BrowserList`, `Reset`. High confidence these also work
given the schema was derived directly from each one's own confirmed hardware probe
(Tenth/Fifteenth/Sixteenth findings) and the lowering logic is uniform, but not
independently verified yet. Also added `Background::Blank` (reuses template 5/`Grid`
with nothing written to it) as a genuine "nothing showing" Background, distinct from
`Reset`(0) which falls back to the device's own native default view instead of looking
blank -- not yet independently tested either.

### Show/hide semantics for Trigger-only members (`DeviceState`)

Per the mutability-class table, Message and the popup have no clear primitive of their
own -- the only way to make either stop showing is a real Background switch. Added
`DeviceState` (lib.rs), a small in-memory tracker (there is no read-back from the device at
all, confirmed repeatedly -- this struct **is** "current state", not a mirror of it) that:
- Remembers `last_background` + the title bar it was switched with, so `hide_popup()`/
  `hide_message()` can restore *whatever was actually there*, not a hardcoded fallback --
  directly addressing "we may want to draw the message over a non-blank background" (i.e.
  Message and the currently-active Background are controlled independently; `Blank` is
  just one Background a caller can choose, never forced by the message/popup verbs).
- Marks both `popup_visible` and `message_visible` false whenever a real
  `switch_background` happens (confirmed: a real switch clears both as a side effect), and
  `hide_popup()`/`hide_message()` are each other's twin for this reason -- both restore
  `last_background`, both mark both flags false, since there's no way to clear just one.
- No-ops (empty message vec) when asked to hide something already hidden.

**Verified directly on hardware** (`verb_test overlaytest`): `switch_background(Mixer)` →
`show_popup` → `show_message` → `hide_message`. Message rendered cleanly (see the z-order
note in the layer table above -- this specific run showed NO popup bleed-through, contrary
to the Thirty-first finding). `hide_message()` correctly restored the full Mixer view
(all 8 knobs, names and values both correct) with the popup also gone, matching the model
exactly.

**Root-caused a real bug this session**: the webcam-viewer's *raw* dismiss path
(`dismissOverlay()` in index.html) only ever re-sent `title_bar` -- never a Background's own
content field -- so unticking the popup/message checkboxes visibly did nothing. Exactly the
"page_template change is a silent no-op unless piggybacked on a real content write" behavior
this section already documented; `DeviceState` never had this problem because
`switch_background_messages` always sends both. `DeviceState` was proven correct in
`verb_test` but had never actually been reachable from the WebSocket bridge until this
session -- `service.rs` now holds one and exposes it via a second, additive
`{"semantic": {...}}` envelope on the same socket (raw `ScreenUpdate` still works unchanged,
by deliberate choice -- see the file's own top doc comment: "we lose something if not
allowing the raw perspective"). index.html's new "Semantic commands" panel calls this for
Show/Hide Popup and Show/Hide Message, and confirmed correct end-to-end this session
(scripted WS client, real hardware): switchBackground → showPopup → hidePopup (restores
Mixer, clears popup) → showMessage → hideMessage (restores Mixer again). The raw checkboxes
above are intentionally left with their old, honest limitation rather than papered over.

## Interaction matrix — Background × Overlay/Message

Confidence key: **D** = directly tested and photographed. **I** = not directly tested for this
specific cell, but inferable with high confidence from the *mechanism* (the popup is hardcoded
to `page_template=0` and drawn as a fixed-position overlay box regardless of what's
"officially" active underneath, so nothing about its rendering logic depends on which
Background happens to be showing; Message is its own totally separate SysEx pathway with no
Background-conditional logic anywhere in source). **?** = genuinely unknown.

| Background | + popup overlay | + Message |
|---|---|---|
| Any of the 13 (0,2-9,16-22) | **I** — renders as the same fixed-position box regardless, per the mechanism; the only real variable is *cosmetic overlap* with whatever content that Background draws in the same screen region (confirmed only for `drum_pads`/21 — box covered part of rows C/D; a Background whose own content lives in that same region, e.g. `browser_list`'s vertical list, might visually collide more than `drum_pads` did — worth a look if it ever matters, not worth blocking the model on) | **I** — Message is Background-agnostic by construction; its repeated successful use as a universal "reset to clean" trick across every kind of prior state this whole project *is* the practical confirmation |
| `drum_pads`(21) specifically | **D** (Twenty-seventh finding) | — |

**Answer to "does a combination render inconsistently, and is it worth allowing"**: based on
the mechanism, no combination is expected to be structurally inconsistent — Overlay and
Message are both designed to be Background-agnostic. The only realistic failure mode is
*cosmetic* (the popup box visually overlapping content that a specific Background also wants
that screen region for), which is a rendering-quality question, not a protocol question, and
not worth pre-emptively blocking any combination over.

## Verbs — the operations the model actually implies

This is the payoff: instead of a pile of independently-diffed fields, the device exposes
exactly these operations. Everything above exists to justify why these are the right verbs
and not others.

1. **`switch_background(template, content)`** — sets the active Background AND resupplies its
   Content in one call. This is the *only* verb that changes which Background is official, and
   as a side effect it's also the *only* way to dismiss Message or an open popup (there is no
   separate "dismiss" verb — dismissing **is** switching). `content` should always be resupplied
   here since the switch clears whatever was there.
2. **`set_chrome(title_bar?, big_font?, current_value?, tabs?)`** — safe to call anytime,
   regardless of Background; never needs to be resent after a switch (it survives).
3. **`show_message(text)`** — one-shot trigger. No corresponding `hide_message()`; call
   `switch_background` to get back to compose content.
4. **`show_popup(items, highlight)`** — one-shot trigger, independent of `switch_background`'s
   `template` argument (always hardcoded to `page_template=0` internally). No
   `hide_popup()` either, same reasoning.
5. **`set_led(cc, on)`** — Class 1, independent on/off LEDs.
6. **`set_status_led(position: Option<1..=4>)`** — Class 2, already implemented as
   `status_led_messages` (clear-then-set as one unit).
7. **`set_cursor_volume(0..=1023)`** — the segmented CC 15/47 pair.

## Session verification — real read-back, not just documentation

Direct answer to "the schema has no error/rejection modeling at all": partially closed.
Found (and unit-tested against the exact live-captured bytes) that the device replies to
`0x09`-family lifecycle SysEx specifically with an ACK -- identical bytes except the
manufacturer sub-id (`0x01`→`0x02`, host-to-device→device-to-host) and the final content
byte decremented by 1. Confirmed directly (separate live test) that `0x08`-family lifecycle
commands and ordinary `0x06` compose writes get **no** reply at all -- so this cannot verify
an arbitrary content write, only whether a session got established.

`expected_ack`/`is_expected_ack` (lib.rs) implement this; `service.rs`'s `Device::connect()`
now opens a brief input connection, sends INIT_2, and waits up to 400ms for the matching ACK
before proceeding -- `session_verified: bool` on `Device`, logged either way ("Session
verified: device ACKed INIT_2." or a warning explaining the write-may-be-silently-ignored
risk). This is the first piece of the "session invalidation is completely unmodeled" gap
that's actually closed: we can now know, at startup, whether the device is really listening,
rather than assuming every write works. **Still open**: nothing currently re-verifies
mid-session (a session could still go bad after startup and nothing would notice until a
write visibly fails to render), and there's still no way to verify an ordinary content write
landed -- only the lifecycle handshake itself.

## Device → host: input semantics (the other half of the bridge)

Everything above this line is one direction only: host writes, device renders. Flagged as a
real gap ("we have NOT worked on the semantics for values flowing FROM the device") because it
was true — physical fader/encoder/button input had a confirmed byte-level decode (`main.rs`,
`decode_cc`/`cc_name`/`cc_kind`), but it lived only in a demo binary that printed to stdout; the
actual persistent bridge (`service.rs`, the one thing anything else talks to) had **no input
code at all**. Partially closed this session:

- `CcKind`/`cc_name`/`cc_kind`/`InputEvent`/`decode_cc` moved into `lib.rs` — shared, not a
  binary-local copy — so `service.rs` (or anything else) can use the same confirmed decode
  `main.rs` already had, without re-deriving or drifting from it.
- `service.rs` now opens a **permanent** MIDI input connection (`start_input_listener`,
  distinct from `Device::connect()`'s temporary ACK-verification one, which sends INIT_2 and
  drops itself before this starts), decodes every incoming Control Change, and keeps the latest
  decoded value per control name (`LastInput`, keyed by `cc_name()` — `"fader_3"`,
  `"jog_wheel"`, etc.) in shared memory for as long as the process runs.
- A newly-connecting WebSocket client is now handed that snapshot as
  `{"inputSnapshot": {...}}`, a separate message from the existing screen-state sync, so an
  existing client that doesn't know the key (index.html doesn't yet) just ignores it.

**Still open / NOT done, to avoid overclaiming**:

- **No live push to already-connected clients.** `handle_client`'s per-client loop only reads
  (`socket.read()`, blocking) — there's no writer channel per client yet, for input OR for
  screen-state edits from another tab. A fresh-connecting client sees the latest known input;
  an already-open one does not get updated in real time. Same pre-existing limitation noted in
  `handle_client`'s own doc comment, now also true for input.
- **No semantic/business-logic layer above the raw decode.** `InputEvent` says "fader_3 moved
  to 96/127" — nothing maps that to a DAW parameter, a `Background` field, or a `DeviceState`
  action (e.g. jog-wheel deltas driving `set_popup_highlight`, or a fader move updating
  `ctrl_element_value` in the currently-displayed `Background`). That coupling is unbuilt.
  `InputEvent` and `Background`/`DeviceState` are two disconnected models right now.
- **The F-Keys button (CC 99) drives a device-native page, not our SysEx display at all** (see
  `f_keys`'s doc comment in lib.rs) — a reminder that not every physical control's effect is
  reachable or overridable through this MIDI-CC decode path.
- **Most of the CC map is reference-sourced, not individually pressed-and-confirmed on this
  hardware.** Faders (0–7/14), pan encoders (48–55), and a handful of buttons (shift 96,
  menu/jog 106–111) are directly confirmed live; the transport row, nav row, and most of the
  88–105 range are sourced from re-grepping the Bitwig driver's JS (see the inline citations in
  `cc_name` — "Thirty-third finding" etc.) and some are explicitly marked tentative or
  unreconciled in the code comments themselves (patch_minus/patch_plus, CC 97). Treat `cc_name`
  as "our best current attribution," not "hardware-verified for every branch."
- **Buttons are decoded as bare press/release (127/0), not debounced or edge-detected** — a
  held button re-sends 127 repeatedly on some controls (not characterized here); nothing
  distinguishes "pressed" from "still pressed."

## Open questions this model doesn't answer yet

- Does `page_labels` actually behave as Content (cleared on Background switch), or is it
  chrome-like? Never directly isolated, only assumed by analogy to `ctrl_element_name`.
- Cosmetic overlap between the popup box and a Background's own content region, for
  Backgrounds other than `drum_pads` — low priority, not a correctness question.
- Does the popup's own pagination (>8 items, `offset` stepping) interact with `switch_background`
  at all, or is it purely an Overlay-layer concern reachable only via `show_popup`?
- Message-vs-popup z-order is still contradictory (see the layer table) — not re-isolated yet.
- 10 of 13 `Background` variants remain schema-only, never exercised through the verb layer.
- No mid-session re-verification exists yet — only a one-shot check at `Device::connect()`.
- The raw and semantic perspectives (service.rs's `ScreenUpdate`/`apply()` vs `DeviceState`)
  are deliberately never reconciled — a raw-panel edit doesn't update `device_state`, so a
  semantic `hidePopup`/`hideMessage` after mixing the two can restore a Background that no
  longer matches what a raw edit put on screen. Boot state is seeded correctly (Thirty-seventh
  finding), but nothing keeps the two in sync after that.
- ~~Residual blank button outlines after hiding a popup~~ — root-caused and fixed: the
  popup's own `Esc`/`Enter` buttons share the Footer slot (displayId 4, `menu_button`/tabs)
  with the normal tab row, which nothing was resending. `DeviceState` now tracks/resends
  `last_tabs` alongside every Background write. Confirmed clean on hardware (Thirty-eighth
  finding, protocol notes).
- Footer (`tabs`) now defaults to 5 blank placeholders and is fully caller-controllable
  (`set_tabs`/`SetTabs`), but nothing has driven real tab labels through this path yet —
  untested whether a Background switch's bounce-then-restore sequence looks visually
  jarring (two full-screen flips) if real (non-blank) tab content is showing throughout.
