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
| **Chrome** | `title_bar`(1), `big_font`(2), `current_value`(3), `menu_button`(4) | Template-independent. Survives a Background switch untouched (Fourteenth finding). Directly confirmed unaffected across **every** Background tested (Tenth finding's per-template probes: title bar rendered identically on templates 0,2,3,4,5,16,18,19,20,21,22 and every subsequent test). |
| **Background** | `page_template`: see the full table below | Mutually exclusive — exactly one is "active." Switching to a different one clears Content, whether or not the switching write touches it (Fourteenth finding). |
| **Content** | `ctrl_element_name`(6), `ctrl_element_value`(7), `pad_state`(0), `page_labels`(5) | Cleared by any real Background switch. Rendered differently by every Background (that's *what makes* a Background visually distinct) — see table. |
| **Overlay** | popup menu (displayId 8, hardcoded `page_template=0`) | Draws on top of whatever Background+Chrome+Content is showing, without changing which Background is "official." Confirmed to overlay `drum_pads`(21) cleanly without disturbing it (Twenty-seventh finding). |
| **Message** (exclusive mode) | `pageTemplate=1` one-shot write | Own separate SysEx pathway (Seventh finding), not a compose-path Background at all. Used dozens of times this whole project as the "force back to a clean state" trick, successfully, from every kind of prior state (compose templates, F-Keys' device-native page, mid-test garbage) — that repeated, always-successful practical use **is** the evidence that Message suppresses Background+Content the same way regardless of what preceded it, even though no one ever tabulated it as a formal test. Does **not** suppress the Overlay layer: the popup's highlighted bars stayed drawn on top of "hacpad" text (Thirty-first finding) — Message sits *below* Overlay in z-order. |

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
| **3. Trigger-only, no clear primitive** | Message, popup menu (Overlay layer) | No device-tracked "current value" to diff against. Every send is a fresh one-shot "show this now." | **No targeted clear exists.** Only: (a) never send it, or (b) a real Background switch, which resets Background+Content (and, per the z-order finding above, Message — but not a currently-open popup, which needs its own switch too since it's a *different* trigger). |

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
independently verified yet.

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

## Open questions this model doesn't answer yet

- Does `page_labels` actually behave as Content (cleared on Background switch), or is it
  chrome-like? Never directly isolated, only assumed by analogy to `ctrl_element_name`.
- Cosmetic overlap between the popup box and a Background's own content region, for
  Backgrounds other than `drum_pads` — low priority, not a correctness question.
- Does the popup's own pagination (>8 items, `offset` stepping) interact with `switch_background`
  at all, or is it purely an Overlay-layer concern reachable only via `show_popup`?
