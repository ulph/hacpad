# Nektar Panorama P1 — protocol notes

> **Current status (read this first): BOTH DIRECTIONS CONFIRMED WORKING, FULL PAGE-TEMPLATE MAP DONE.**
>
> - **Input** (CC decoding) and **output** (screen writes) are both confirmed live against real
>   hardware. The original blocker was an inverted `Internal`/`Instrument` port assignment — see
>   "Fifth finding" — not a byte-level protocol error. `main.rs` (the real `panorama-bridge` binary)
>   has the fix and is smoke-tested end-to-end. See `prototypes/panorama-p1/README.md` to run it.
> - **All 13 page-template values** (`0`-`5`, `16`-`22`) have been probed directly on hardware with a
>   title-bar write plus an 8-entry name-field write each — see "Tenth finding" for templates 2/16/17
>   and the "Summary" table (in the Tenth finding, after template 0) for the complete map. Each real
>   template (2-22) shows a *different widget* (knobs, faders — split or one row —, a 4×4 pad grid, or
>   a bulleted list) but **the same two content fields behave identically across all of them**: the
>   3-segment title bar (`displayId 1`) and the up-to-8-slot name field (`displayId 6`). Template `0`
>   is confirmed as a genuine "reset" sentinel, not a real page. Template `1` is the simpler
>   one-shot message overlay used throughout most of this investigation (plain multi-line text box —
>   see the correction below).
> - The big-font single-line readout seen in every native screenshot (`MIDI CC N`) is a real, writable
>   field — `displayId 2` — confirmed with our own text ("Twelfth finding"). Its real semantic role is
>   "name of whatever was just touched", not a permanent slot, per the user's correction.
> - **What must match the Bitwig driver exactly vs. what's just its own software choice** is written up
>   as the "Eleventh finding" — short version: match the wire protocol (prefixes, command bytes,
>   template/displayId IDs, message shapes, the ~127-byte length ceiling) precisely; everything else
>   (what text, when to send it, which page to show when) is free design space for hacpad's own
>   implementation. Ordinary page navigation is local/button-driven, not DAW-orchestrated (same finding).
> - A pure client-side **screen simulator** (`tooling/webcam-viewer/simulator.html`, served at
>   `/simulator.html`) mocks up all the widget layouts found above, for previewing a planned write
>   before spending a hardware round-trip on it.
> - **Still open**: `displayId` values `0`, `3`, `8` untested; `faderElementValue`'s index-9-17 range
>   (shares `displayId 7` with `ctrlElementValue`) untested; whether the firmware needs periodic
>   traffic as a keepalive over a long session is unknown; a real Bitwig ground-truth capture (scripts
>   prepared in `prototypes/panorama-p1/sniff_bitwig.py` / `aconnect_fallback.py`, meant to be run
>   outside this sandbox) has not yet happened.
>
> **Correction (important, kept for the record):** the "Seventh finding" below, claiming the message
> write is a "column-major rotated character grid" needing transposed ASCII art, **was wrong** — it was
> an artifact of the debugging webcam being mounted 90° off from the device, not accounted for at the
> time. Corrected: it's an ordinary top-to-bottom, left-to-right multi-line text box, nothing rotated or
> transposed. See the correction under "Seventh finding" for how this was caught.

Device confirmed connected: `Nektar Technology / PANORAMA P1`, USB VID:PID `2467:2025` (`/sys/bus/usb/devices/1-1/`).

Three sources feed this doc, in order of how much we've actually leaned on them: (1) a community
reimplementation, used as an early orientation reference; (2) `ulph`'s own 2017 extension project,
useful for the mapping-schema model, not the low-level protocol; (3) **Nektar's own official driver**,
obtained directly from Nektar (Copyright 2015 Nektar Technology, Inc.) — this is the actual ground
truth and supersedes the other two wherever they conflict. See below for details on each.

## Source credit

The facts below (CC map, SysEx structure, byte values) come from reading the source of a community
Bitwig extension, not from our own capture yet:

**[LukeLandry/nektar-panorama-p1-bitwig](https://github.com/LukeLandry/nektar-panorama-p1-bitwig)** —
"a reimplementation of the Nektar Panorama P1 control surface script for Bitwig ... simplifying the
device, and fixing a number of defects that have cropped up in Nektar's aging implementation." WIP,
last pushed July 2025, one star, no license file.

**No license declared** — treat this as a reference to verify independently, not code to copy. Protocol
facts (byte values, CC numbers) aren't copyrightable; the author's actual Java source is. Our own bridge
implementation should be written from scratch against these facts, re-verified by our own USB/SysEx
capture, not lifted from his repo.

Also note: since this is a *reimplementation* fixing "defects" in Nektar's official script, it may
deviate from the official protocol in places — our own capture against Nektar's official software is
the actual ground truth once we get to that step.

## Finding: control input is plain MIDI CC, not proprietary

Every physical control (faders, encoders, buttons, transport, nav) is a standard MIDI CC message — no
SysEx, no raw HID needed for input. This narrows the real "USB device bridge" work considerably: input
is already the MIDI Bridge's job (per `DESIGN.md`'s architecture diagram); the only proprietary surface
is the **display** (SysEx-based).

CC map (per `ControllerHardware.java`):
- Faders: CC 0–7 (channels), CC 14 (master)
- Parameter encoders: CC 64–71
- Pan encoders: CC 48–55
- Select buttons: CC 16–23
- Transport (play/stop/record/rewind/forward): CC 81–85
- Navigation (track/patch select, view toggle): CC 91–95
- Buttons: value 127 = pressed, 0 = released
- Encoders: relative 2's-complement CC encoding (incremental, not absolute)
- Faders: absolute CC value (0–127)

**Own capture confirms CC 48–55 = pan/rotary encoders** (matches the reference exactly — moving a knob
showed live "MIDI CC 48"..."MIDI CC 55" readouts on the device's own screen for each of the 8 knobs).

**Not in the reference, seen in our own capture** — the map above is incomplete:
- CC 37: smooth absolute-looking ramp when moving a fader (not CC 0–7 as documented — needs
  reconciling, possibly a different fader or a bank-shifted CC)
- CC 111: clean `127`/`0` pair on press/release — a button, channel matches the others (status byte
  `0xB1` throughout, i.e. MIDI channel 2)
- All channel bytes observed so far are `0xB1` (Control Change, channel 2), not stated explicitly in
  the reference

## Finding: display write protocol (SysEx)

Manufacturer prefix: `F0 00 01 77 7F 01` — `00 01 77` is Nektar's registered 3-byte MIDI SysEx
manufacturer ID; `7F 01` is likely device/model + unit byte (unverified).

Lifecycle:
```
init:        F0 00 01 77 7F 01 08 02 00 00 01 01 73 F7
init2:       F0 00 01 77 7F 01 09 03 00 00 01 3E 34 F7
linux-only:  F0 00 01 77 7F 01 08 01 00 00 01 01 75 F7   (sent only when host is Linux — worth noting, we ARE on Linux)
exit1:       F0 00 01 77 7F 01 09 00 00 00 01 00 75 F7
exit2:       F0 00 01 77 7F 01 08 02 00 00 01 00 74 F7
```

Write-message shape: `prefix + 06 (write) + layoutByte + partByte + [index, textLength, asciiBytes, 00, ...] + F7`
- `layoutByte`: which screen layout/mode (P1 has at least two — MIXER and CONTROLS)
- `partByte`: which display region — the source has two overlapping numbering schemes for this
  (headerLine/messageLine/messageValue/buttonLabels/toggleModeDisplays = `01`–`05` in one place,
  pageTitle/controlNames/controlValues/menu = `05`–`08` in another) — **needs our own capture to
  disambiguate**, likely context-dependent on layout mode.
- Text: length-prefixed US-ASCII (one length byte, then that many ASCII byte values)
- Per-field character limits observed: 4 chars (track names), 8 chars (button labels), 12 chars
  (device/parameter names) — real hardware display field widths.

Two full example messages, usable as a first replay test against the live device:
```
F0 00 01 77 7F 01 06 02 06 01 00 00 02 00 00 03 00 00 04 00 00 05 00 00 06 00 00 07 00 00 08 00 00 09 01 4D F7
F0 00 01 77 7F 01 06 02 04 00 05 00 00 00 00 00 00 01 01 2B 00 02 07 42 72 6F 77 73 65 72 00 03 06 50 72 65 73 65 74 00 04 06 52 65 6D 6F 74 65 00 05 05 50 61 67 65 73 F7
```
(the second one spells out button labels "Browser", "Preset", "Remote", "Pages" in ASCII — visible by eye in the hex if you decode the byte values)

## Not yet covered by this reference

- Motorized fader position feedback (host → device) — not seen in what we pulled
- LED/button illumination beyond the two-state (127/0) action-button case
- Exact `partByte`/`layoutByte` disambiguation (see above)
- Anything specific to the official Nektar driver that this reimplementation deliberately changed

## Second source: `ulph`'s own PANOMOD project (~2017)

Own prior work (10 years old, no licensing concern — same author). Found as `PANOMOD.control.js` plus
per-plugin map files (`bazille.js`, `repro1.js`, `_map_files.js`) in a personal zip archive. Turned out
to be a **high-level extension layer only**: it `load()`s the real official driver
(`PANORAMA_P1.control.js`, a Bitwig-bundled file, not included in the archive) and monkey-patches its
*mapping* API (obfuscated internal function/property names like `Z81133A8B43E8E6FC1`) to let custom
per-plugin parameter maps override or extend Nektar's built-in ones. Useful as a model for our own
mapping-markup schema (page styles `Flat`/`Knobs`/`FaderLow`/`FaderHigh`, subpages, value-dependent
dynamic ranges) — **not useful** for the low-level SysEx/display protocol we actually need, since that
lives inside the official driver file this project never had to touch or include.

## Confirmed: device responds to standard MIDI Identity Request

Sent the generic (non-Nektar-specific) MIDI Universal Non-realtime Identity Request:
```
send: F0 7E 7F 06 01 F7
recv: F0 7E 7F 06 02  00 01 77  67 48  25 40  30 31 30 30  F7
```
Decoded: manufacturer ID `00 01 77` (matches the Nektar prefix used everywhere else), family/member
codes `67 48` / `25 40` (uninterpreted so far), and `30 31 30 30` is literally ASCII **"0100"** — a
version string. This proves the device's MIDI parsing is alive and correct; we'd never sent this
standard handshake before.

## Ruled out: the HID interface

The P1 does enumerate a separate USB HID interface (interface 02,
`/dev/input/by-id/usb-Nektar_Technology_PANORAMA_P1-if02-hidraw`) distinct from the MIDI/audio-class
interface. Once permission-unblocked (it's `crw-------`, needs a one-time `chmod`/udev rule with real
root from outside any sandbox) and its HID report descriptor read via `ioctl(HIDIOCGRDESC)`, it decodes
to a **standard USB HID boot keyboard** (Usage Page "Keyboard/Keypad", 8-byte report: modifier + reserved
+ 6 keycodes) — exactly why Linux also creates a `-event-kbd` symlink for it. Some physical buttons
apparently send literal keyboard keypresses (a common pattern for generic DAW-shortcut buttons). **Not**
a vendor display channel. Dead end for this specific question.

## Third source: the actual official driver, found

Found the real thing: a "Panorama_P1_P4_P6_Bitwig_Studio_Integration_Files_Linux_2023-06" support
package (Nektar's own official download, obtained directly — no Bitwig install needed) containing
`PANORAMA_P1.control.js` itself (508 lines, minified but not obfuscated beyond short symbol names —
readable), plus shared files `pnx1.js` (8226 lines) and `pnx2.js` (837 lines) that it `load()`s.
Copyright header: `Copyright 2015 Nektar Technology, Inc. v2.1.4`.

Confirmed byte-for-byte against our own reverse-engineered messages — **we already had these right**:
```js
var LINUX_INIT = "F0 00 01 77 7F 01 08 01 00 00 01 01 75 F7";
var INIT2      = "F0 00 01 77 7F 01 09 03 00 00 01 3E 34 F7";  // (P1-specific variant; P4/P6 differ)
```

### Real write-function definitions (verbatim, deobfuscated by hand from short symbol names)

```js
function setPageTemplate(layout){
  sendSysex("F0 00 01 77 7F 01 06 " + uint7ToHex(layout) + "F7");
  previousTemplate = lastPageTemplate;
  lastPageTemplate = layout;   // <-- STATEFUL: subsequent writes read this global, it's not a per-call arg
}

function writeToDisplay(index, part, text){
  var msg = "F0 00 01 77 7F 01 06 "
    + uint7ToHex(lastPageTemplate)      // the layout byte we'd been passing as a literal
    + uint7ToHex(index) + uint7ToHex(part)
    + uint7ToHex(text.length) + " " + text.toHex(text.length) + " F7";
  sendSysex(msg);
}

function writeMessageToDisplay(text){   // single-line "message" convenience wrapper
  var msg = "F0 00 01 77 7F 01 06 " + uint7ToHex(SOME_CONST.PART) + uint7ToHex(0) + uint7ToHex(0)
    + uint7ToHex(text.length) + " " + text.toHex(text.length) + " 04 F7";  // note trailing "04" before F7
  sendSysex(msg);
}

// multi-entry "compose" form -- this is what our replayed button-labels example matches exactly
function composeStart(layout, part){
  sysexComposeString = "F0 00 01 77 7F 01 06 " + uint7ToHex(layout) + uint7ToHex(part);
}
function textEntry(index, text){
  if (sysexComposeHadEntries) sysexComposeString += "00 ";
  sysexComposeString += uint7ToHex(index) + uint7ToHex(text.length) + text.toHex(text.length);
  sysexComposeHadEntries = true;
}
```

**This confirms our documented write-message shape was correct**: `prefix + 06 + layoutByte + partByte +
[index, len, ascii, 00, ...] + F7`. What we had wrong: **`layoutByte` isn't a literal you pick per
write** — it's a stateful global (`lastPageTemplate`) that only changes when `setPageTemplate()` is
called, which we never did. We'd been hand-picking `02` as a guess.

### The real init sequence calls a *second MIDI port*

```js
function nektarinit(){
  plxLin && host.getMidiOutPort(1).sendSysex("F0 00 01 77 7F 01 08 01 00 00 01 01 75 F7");  // <-- port 1, explicitly
  ...
}
```
This is the **only** explicit `getMidiOutPort()` call in the whole file — every other `sendSysex(...)`
call (including all display writes) is the bare global, which defaults to port 0. The Linux driver
declares `host.defineMidiPorts(2,2)` — 2 MIDI in, 2 MIDI out. Confirmed on this machine: ALSA sequencer
client 20 ("PANORAMA P1") exposes **4 separate ports** — `Internal` (0), `Instrument` (1), `Mixer` (2),
`ReWire Host` (3) — all invisible through the plain rawmidi character device (`/dev/snd/midiC1D0`), which
only reaches port 0. Confirmed via `/proc/asound/card1/midi0`: our whole session's traffic (1118 bytes)
went out **Output 0** only; **Output 1 had 0 bytes**, ever, before we found this.

**Fix applied and tested**: used `python-rtmidi` (installed via pip, no system package needed) to open
port `PANORAMA P1:PANORAMA P1 Instrument 20:1` specifically for the linux-init message, keeping
everything else on port 0 as before. Result: **one genuine, unexplained display change** ("MUTED"
appeared in a button-tab slot that hadn't shown that text before) but a follow-up decisive test (a
unique, unmistakable string written to all 8 part-bytes) still did **not** appear anywhere. So the port
fix alone isn't sufficient — it may still be a necessary *part* of the real sequence, just not the whole
of it.

### Why we still don't have pixels on screen

The actual content-writing calls (`writeToDisplay`/`textEntry`) only happen inside each `DisplayPage`
object's `updateOutputState()` method, triggered via `setActiveDisplayPage(page)` → `page.onActivated()`
→ (mostly just sets an `outputState.forceUpdate` flag) → the real update loop calls
`updateOutputState()`, which pulls live values from actual Bitwig API objects (`primaryInstrument`,
`cursorTrack`, real track/device names) that don't exist without a running Bitwig session. This is a
real, non-trivial state machine — not a simple standalone function we can lift out. `setPageTemplate()`
itself is defined but **never called anywhere in `PANORAMA_P1.control.js`** with a literal argument in
what we've grepped so far — it's likely invoked from within `updateOutputState()` implementations we
haven't fully traced, or from `pnx1.js`/`pnx2.js` (not yet searched as thoroughly as the main file).

## Fourth finding: the "quick message" shortcut, confirmed byte-exact, still zero effect

Found `OutputState.prototype.send` (`PANORAMA_P1.control.js:64`) — the real flush/dispatch method,
called from a top-level `flush(){ ...; activePage.updateOutputState(); activePage.outputState.send() }`.
Its very first branch is a dedicated **one-shot "message" path** that completely bypasses the whole
per-field `DisplayPage`/`updateOutputState()` machinery:

```js
OutputState.prototype.send = function(){
  var a = currentOutputState;
  if (this.pageTemplate == PAGE_TEMPLATE.MESSAGE /* == 1 */) {
    if (a.message != this.message || this.forceUpdate)
      writeMessageToDisplay(this.message), a.message = this.message, a.pageTemplate = this.pageTemplate;
  } else {
    // ... the full per-field page composition path (needs live Bitwig session objects)
  }
};
```

And the pageTemplate enum (`PANORAMA_P1.control.js:10`) gives the literal value:
`{ ..., MESSAGE: 1, ... }` — **not `02`**, which is what we'd been guessing this whole time. Combined
with `writeMessageToDisplay`'s real body, the exact bytes for a one-shot text overlay are:

```
F0 00 01 77 7F 01 06 01 00 00 <len> <ascii bytes...> 04 F7
```

(`06` = write-display command, `01` = MESSAGE page-template constant, `00 00` = two fixed zero fields,
`<len>` = text length, then ASCII bytes, then a trailing `04` before `F7` — this is genuinely simpler
than the full page-composition path and, unlike it, needs **no live DAW session state** to construct.)

**Tested against real hardware** (Rust, using the `midir` crate for named ALSA-sequencer port access —
`prototypes/panorama-p1/src/bin/msg_test.rs`): sent the confirmed real init sequence (linux-init on the
`Instrument` port, then `INIT_1`/`INIT_2` on the `Internal` port), then this exact byte-for-byte message
write, held for 8 seconds, and photographed the screen mid-hold (well before any exit sequence). Result:
**zero visible change** — the screen still showed the device's own native standalone-mode UI (its
built-in `Faders`/`Encoders`/`Cntrl Edit`/`Global`/`Setup` tab display), completely unaffected by our
SysEx traffic.

This rules out "we had the wrong bytes" as the explanation — the bytes now match the official driver
exactly, sent on the confirmed-correct ports, and still nothing happens.

### Two more hypotheses tested and ruled out

- **`setPageTemplate()` is dead code for this model.** Grepped all three files for
  `setPageTemplate(` call sites with any argument: the *only* match in 8,571 combined lines is the
  function's own definition (`PANORAMA_P1.control.js:50`). It is never called anywhere. Its real body,
  found for completeness:
  ```js
  function setPageTemplate(a){
    var b = "F0 00 01 77 7F 01 06 " + uint7ToHex(a) + "F7";   // just prefix + 06 + layout + F7
    Z81136C4863E8BC4AF(b);
    previousTemplate = lastPageTemplate; lastPageTemplate = a;
  }
  ```
  Neither branch of `OutputState.prototype.send()` calls it either — the message path calls
  `writeMessageToDisplay` directly, and the full page path builds its own `06 <layout> <displayId>`
  header inline via `Z810A561E13F1C28DD` (composeStart), never through this function. So the "layout
  mode must be separately latched via SysEx first" theory is wrong — ruled out, not just untested.

- **Bidirectional MIDI (opening input, not just output) makes no difference.** The real
  `nektarinit()` also opens MIDI **input** connections (`getMidiInPort(0)/(1).setMidiCallback(...)`),
  which our test had never done — only output had ever been opened. Extended `msg_test.rs` to open
  live input connections on both `Internal` and `Instrument` (logging anything received), re-ran the
  full init + message-write sequence with those connections held open throughout, re-photographed
  mid-hold. **No change** — same native standalone screen, and the device sent nothing back on either
  input port during the whole run. Ruled out.

- **Repeating/refreshing the write doesn't help either.** Real Bitwig never sends a display write
  once — `flush()` re-runs `OutputState.send()` continuously (every parameter tick, transport change,
  100ms blink timer, etc.). Tested whether the device needs a refreshed write rather than a one-shot
  message: resent the same message-write every 200ms for 8s (instead of once), held, photographed
  mid-hold. **No change.** Ruled out.

- **It isn't specific to the "quick message" shape either.** Added a `--raw <hex>` mode to
  `msg_test.rs` and replayed, verbatim, the community reimplementation's second full example message
  from earlier in this doc (layout `02`, the full compose form spelling out "Browser"/"Preset"/
  "Remote"/"Pages") after the same real init sequence. **Also zero effect.** Three structurally
  different message shapes (our real-driver message-path bytes, a raw replay of someone else's
  captured-working compose-form bytes, and the init/exit lifecycle bytes themselves) now all
  triangulate on the same conclusion: **the device is not rendering any vendor SysEx we send it at
  all right now**, independent of exact byte content — strongly consistent with it simply not being in
  a state where it listens to display writes yet, rather than any remaining byte-level mistake.

### Command-byte space enumerated — no missing handshake found

Grepped all three files for every distinct byte following the `F0 00 01 77 7F 01` prefix in a literal
SysEx string, to check for an unsent "enable"/"handshake" command we might be missing: only **`06`**
(display write), **`08`**/**`09`** (mode/connection state — both already covered by our init/exit
sequence), and one new one, **`0B`**. Read all of `0B`'s call sites: they cluster entirely around
browser/patch-menu open/close logic (`gBrowserOpen`, `application.focusPanelAbove()`, menu
highlight indices), and one payload spells literal ASCII `"Launcher"`
(`0B 00 0F 00 08 4C 61 75 6E 63 68 65 72 00 01 02 0F 00 F7`). This looks like an LED/indicator-ring
control (most likely the jog-wheel or a browser-mode indicator light), unrelated to the text display.
No additional handshake/enable command exists in the shipped protocol — the command space really is
just these four bytes, and we've already sent every one relevant to display state.

### Working hypothesis: the device needs to be switched into a DAW-control mode first

The screen we keep photographing is labeled with its own `Setup` tab — this looks like the P1's
**standalone firmware UI**, the same UI it shows with no computer attached at all, used to browse live
MIDI CC values (`MIDI CC 63`, fader/encoder numbers, etc.). Real Nektar Panorama hardware is known to
have a DAW-profile select mechanism (pick "Bitwig Studio" vs "generic MIDI" vs other DAWs) — it's
plausible the device **ignores all display SysEx entirely** while it's in standalone/generic mode, and
only honors it once switched to a specific DAW profile via its own on-device `Setup` menu (a manual,
physical action — something only reachable by pressing the actual `Setup` button/encoder on the unit,
not by anything we can do over MIDI). This has not been tried yet this session.

**Update: this hypothesis was directionally right (a device-mode gate) but pointed at the wrong
button.** See "Fifth finding" below for the actual mechanism, confirmed from the official guide.

## Fifth finding: it's not a byte problem at all — the device is in "Internal" mode

Per the user's prompt, went back to the driver sources with fresh eyes and cross-referenced against
the *actual* official documentation (found and read this session, not previously located):

- **`uint7ToHex`, `String.prototype.toHex`, `sendSysex`, `sendMidi`, `sendChannelController`,
  `isNoteOn`, `isChannelController`** are called throughout all 3 driver files but defined in *none*
  of them (confirmed with both `function name(...)` and `name = function(...)` search patterns, and by
  confirming there is no fourth JS file anywhere in the official support package). These are genuine
  Bitwig Controller API v1 globals (`loadAPI(1)`, declared at the top of all three `.control.js`
  files) — legacy convenience helpers Bitwig's scripting sandbox provides directly, not
  Nektar-specific code. This closes the loop on "maybe we're missing a shared utility file somewhere."
- Grepped for any raw-USB/HID access path in case the real gate lives below MIDI entirely (`usb`,
  `hid`, `vendor`, `endpoint`, `control transfer`, etc.) — the only hit was `sendRawMidiEvent`, a real
  documented Bitwig `NoteInput` API method for note feedback, unrelated to USB. **There is no raw-USB
  path anywhere in this driver** — everything genuinely goes through standard MIDI/SysEx.
- Fetched Nektar's own **"Using Panorama P-Series with Bitwig Studio"** guide (bundled in the same
  official support package as the driver — `Bitwig_Studio_Setup_&_User_Guide_for_Panorama_P-series.pdf`,
  read directly via `pymupdf` since it wasn't OCR'd/text-searchable through a plain fetch). Page 10,
  "Modes & Display":
  > "Each of the Mode navigation buttons will configure Panorama to control different aspects of
  > Bitwig Studio. It's like having four control surfaces in one: **Mixer** ... **Instrument** ...
  > **Transport** ... **Internal**: Uses Panorama's internal MIDI controller functions so you can
  > **jump out of our dedicated Bitwig Studio protocol** and use Panorama as a traditional MIDI
  > controller."

**This is the real answer.** The screen we've been photographing all session — `MIDI CC 63`, raw
fader/encoder value readouts, tabs labeled `Faders`/`Encoders`/`Cntrl Edit`/`Global`/`Setup` — is not a
"standalone/no-DAW-detected" fallback UI at all. It's **Internal mode**, one of four physical
mode states the hardware can be in *regardless of whether Bitwig is running and correctly
connected*, entered via the physical `Mode` button, whose entire documented purpose is to bypass the
DAW display protocol. No SysEx we send — correct or not, on any port, one-shot or repeated — was ever
going to render while the device is deliberately in this mode. Every prior "still zero effect" result
in this doc is fully consistent with that, not evidence against our bytes.

The same page (page 6 of the guide, Linux setup instructions) also reveals a second, independent bug:
the official manual port config table lists, for the Linux manual-add flow:
```
Output1: Panorama Instrument
Output2: Panorama Internal
```
If Bitwig's Controller Settings UI numbers ports 1-based matching `getMidiOutPort(0)`/`getMidiOutPort(1)`,
this means **our Internal/Instrument port assignment has been backwards all session**: the bare
default `sendSysex()` (implicit port 0) is `Instrument`, and the one explicit
`host.getMidiOutPort(1)` call (used for the Linux-only init message) is `Internal` — the reverse of
what we assumed from the very first port-mapping test. This wasn't reachable by testing alone (the
mode gate would have hidden its effect regardless of which port we used) but must be fixed before the
next round of hardware testing.

### Confirmed: the port fix alone was sufficient

Fixed `msg_test.rs` to send the Linux-only init message on `Internal` and everything else (`INIT_1`,
`INIT_2`, the message write, exit) on `Instrument` — the reverse of every prior test this session.
Result, immediately, on the very first run:
- The device **replied** to our init handshake for the first time all session:
  `<< default IN: F0 00 01 77 7F 02 09 03 00 00 01 3E 33 F7` (an echo/ack of our `INIT_2`, on the
  `Instrument` port) and again on exit. Zero replies had ever been seen on any port before this fix.
- The device's screen **rendered our literal text**, `"HACPAD FIX"`, confirmed by webcam photo —
  overwriting the native standalone UI directly, regardless of whatever Mode-button state the device
  was already in. The physical Mode-button press the user tried earlier turned out not to be
  necessary once the port was correct.

This is the actual fix. Both directions of the bridge (CC input, confirmed earlier; display write,
confirmed now) are working against real hardware.

## Next steps

1. **Fold the corrected port mapping into `main.rs`** (the main `panorama-bridge` binary), which still
   has the old backwards Internal/Instrument assignment from before this fix.
2. Build a real full-page write (the `composeStart`/`textEntry`/flush compose path, not just the
   one-shot "message" shortcut) using the corrected ports, now that we have a confirmed-working
   baseline to build on.
3. Investigate the device's reply bytes further (`7F 02` instead of the `7F 01` we sent, and the
   checksum-like last byte shifting by 1) — may be worth understanding, though not blocking.

## Sixth finding: multi-line text confirmed -- correction below (see Seventh finding)

Once the port fix landed, tested further directly on hardware:
- **Embedded `\n` (0x0A) bytes are honored** as a segment delimiter (not dropped, not shown as a
  literal control character).
- **Resending the same write on a timer causes a visible flicker for no benefit** — a single send
  persists on screen fine (this was already known from the repeated-write hypothesis test earlier, but
  is now confirmed as the right behavior going forward rather than a leftover test artifact).
  `msg_test.rs` was changed from "resend every 200ms" back to "send once, hold" accordingly.
- `main.rs` (the real `panorama-bridge` binary) was updated with the corrected port mapping and
  smoke-tested end-to-end: init → write "hacpad bridge" → confirmed rendered on screen via webcam,
  CC-input listener still running normally alongside it.

**Correction:** an early 7-line bordered banner test ("HACPAD" / "usb bridge" framed in `#` borders)
was initially described here as "rendered correctly and legibly" in a normal horizontal multi-line
layout. Re-examined more carefully against later tests (below): it was actually rendering in the same
per-segment rotated-column layout as everything else, and only *looked* like a normal horizontal block
because the symmetric `#`-border padding lines made the rotation easy to miss at a glance. See the
Seventh finding for the corrected, actual layout model.

## Seventh finding (RETRACTED — see correction below)

~~The "message" write is a column-major character grid, not a text paragraph~~ — **this entire
conclusion was wrong**, caused by an unaccounted-for camera mount rotation, not any real device
behavior. Left in place (struck through) rather than deleted, as a record of the mistake and how it
was found, per the reasoning below.

**What actually happened**: the webcam used to photograph the P1's screen throughout this whole
investigation is mounted **physically rotated 90°** relative to the device — a fact about the camera
rig, not the device, that hadn't been accounted for when reading any of the photos. Every screenshot in
this document up to this point was interpreted at the wrong orientation. Once the user pointed this
out and every affected photo was re-rotated 90° for review, the "column-major, rotated-per-segment"
model collapsed completely: physical button labels (`Shift`, `Mode`, `Undo`, etc.) only read correctly
upright after this correction, and — decisively — text that had looked like separate rotated columns
(`LINE-01`..`LINE-16`, `ABCDEFGHIJKLMNOPQRSTUVWXYZ0123`) turned out to be perfectly ordinary,
non-rotated, non-mirrored text, reading left-to-right, stacked top-to-bottom, exactly like a normal
text box. **`\n` just means "next line", the way it does everywhere else.** There is no column-major
grid, no per-segment rotation, nothing exotic — the "message" write is a plain multi-line text field.

The parts of the original finding that don't depend on the (wrong) rotation model still stand:
- `.message` is genuinely never set to real content anywhere in `PANORAMA_P1.control.js` — this write
  path is real but unexercised by Nektar's own P1 driver.
- The two always-zero bytes in the write are fixed placeholders, not a coordinate/index.
- A single line holds at least 30 characters without truncation; at least 16 lines have been shown to
  stack without an obvious cutoff (exact upper bounds on either axis still not pinned down precisely,
  and not very important now that the addressing model is simple).
- The **total message length is still capped near ~127 bytes** (the observed corruption/rejection at
  ~400+ bytes stands on its own, independent of the rotation mistake — see below).
- No evidence of a size/scale parameter, raw pixel/framebuffer command, custom glyph range, or color
  field anywhere in this write's byte layout or the wider command-byte space (`06`/`08`/`09`/`0B` only).
  These remain genuinely unexplored/unsupported as far as this write path goes.

**Consequence for ASCII art**: a normal row-major ASCII-art string (one line per row of the picture,
read and written exactly as you'd expect) renders correctly, right side up, no transposition needed.
Confirmed working: a small (20×6, then a proportionally-corrected 26×4) row-major bitmap conversion of
`assets/logo.png`, resized/thresholded with Pillow and sent verbatim, rendered as a recognizable (if
low-resolution) dot-matrix version of the logo. At this scale (~100-125 total characters) a
faithful reproduction of a curvy wordmark isn't really achievable — this is a hardware/byte-budget
ceiling, not a conversion-quality problem. See `assets/logo_20x72.txt` (human-readable reference size,
not sent to the device) vs. the much smaller device-constrained attempts.

### Two more things ruled out while investigating this

- **The single-byte length ceiling is real and firmware-level, not a driver convention we can work
  around.** Tried sending the same `pageTemplate=1` write using the *general* per-field compose shape
  (`<index><len><text>`, each entry with its own independent length byte, no shared total-length field)
  instead of the fixed shortcut shape. The device replied with a short, distinctly different message
  (`F0 00 01 77 7F 7E F7`) that looks like an error/NAK rather than its usual ack — this specific
  firmware routine only understands its one fixed shape; it does not accept the general compose
  format. Sending several *separate* `pageTemplate=1` writes in sequence was also tried: each new write
  fully replaces the previous one rather than layering/accumulating, so there's no way to build a
  bigger image out of several small writes through this specific mechanism. Any real "chunking" would
  have to target the general per-field compose path (see the Ninth finding below) instead.
- **Injecting a synthetic Control Change value does *not* move the native on-screen fader/knob
  widgets.** The Internal-mode "monitor" screen (the one showing `MIDI CC n`, live fader/knob graphics)
  only reflects MIDI the device itself sends *out* from physical touches — it's a read-only status
  display of the hardware's own state, not a receiver for externally-injected values. Sending our own
  CC into the device (confirmed via the same mechanism used successfully for LEDs) had no visible
  effect on it at all; the "last touched" readout kept showing whatever the user had actually pressed,
  never our injected CC number. Real widget-driving (if possible at all) would have to go through the
  per-field page-composition values below, not this monitor screen.

### A tentative probe of the full page-composition path (pageTemplate 16+)

Tried a minimal, hand-built full-page write (`composeStart`/`textEntry` shape, not the message
shortcut): `06 10 01 01 06 <"HACPAD"> F7` (pageTemplate `0x10`=16, displayId `0x01`, one text entry).
Result: the on-device UI's active tab switched from `Faders` to `Encoders` (still within the same
native-style chrome — tabs, colors, layout all otherwise unchanged) and `"HACPAD"` appeared briefly in
a small status-label area near a live CC readout. This is a real, contained effect (not damaging, no
runaway state), but far short of a distinct "Bitwig Mixer page" with its own colors/widgets — more
likely we just poked one field of whatever the firmware's Internal-mode UI already renders, since
`06`'s payload has no field wide enough to select a whole alternate rendering (no color/coordinate
bytes exist anywhere in the protocol we've read). The working theory here — that the rich visuals
(colored tabs, backgrounds, slider-widget graphics, larger value fonts) are native firmware chrome tied
to which page/tab is active, and the DAW-facing protocol only ever fills in *text* within fields the
firmware has already decided how to draw — is now substantially **confirmed** by tracing the real data
pipeline below (Ninth finding): every write the real driver ever does is *text into a named field*,
never a color, coordinate, or widget-selection byte.

## Ninth finding: where plugin-parameter text comes from, and how it reaches the screen

Traced the full pipeline the real driver uses to put live Bitwig content (e.g. a plugin's parameter
names and values) on screen, end to end, from the Bitwig Controller API to the literal SysEx bytes:

1. **Subscribe to the Bitwig API**, once, in `nektarinit()` — for each of the 8 remote-control "pages"
   slots of the currently selected instrument:
   ```js
   InstParameter = primaryInstrument.getParameter(b);
   InstParameter.addNameObserver(8, "", instParameterNames.setter(b));          // e.g. "Cutoff"
   InstParameter.addValueDisplayObserver(8, "", instParameterDisplays.setter(b)); // e.g. "1.2 kHz"
   InstParameter.addValueObserver(127, parameterValues.setter(b));               // raw 0-127
   ```
   Bitwig calls these observer callbacks itself, live, whenever the selected plugin, page, or parameter
   value changes — this is push-based, not polled.
2. **Local cache**: each observer just writes into a plain `BufferedElementArray("", 8)` (`
   instParameterNames`, `instParameterDisplays`, `parameterValues` — simple JS arrays-with-a-setter),
   giving the driver a live local mirror of "the 8 parameter names/value-strings/raw-values" at all
   times, independent of anything screen-related.
3. **Page-local aliasing**: whichever source is actually relevant right now (instrument parameters vs.
   macros vs. the cursor-device's parameters) gets pointed at by a shared pair of local variables,
   e.g. `parameterNames = instParameterNames; parameterDisplays = instParameterDisplays`. This is just
   so the next step doesn't need to know *which* source it's showing.
4. **Copied into `OutputState` by `updateOutputState()`** (called every `flush()`, i.e. on basically
   any relevant change): `b.ctrlElementName[a] = parameterNames.values[a]; b.ctrlElementValue[a] =
   parameterDisplays.values[a]; b.encoderValue[a] = parameterValues.values[a]` for each of the 8 slots.
5. **Diffed and flushed by `OutputState.prototype.send()`**: compares each `ctrlElementName[b]`/
   `ctrlElementValue[b]` against what was last actually sent to the device; only slots that changed (or
   a forced full refresh) get written, via `composeStart(pageTemplate, DISPLAY_ID.ctrlElementName)` /
   `textEntry(1+b, text)` / finish — i.e. real per-field diffing, unlike our one-shot `pageTemplate=1`
   message write which always resends everything.
6. **Actual bytes on the wire**: `F0 00 01 77 7F 01 06 <pageTemplate> <displayId> <index><len><text>
   [00 <index><len><text> ...] F7` — this is the *same* general compose shape as the "Browser / Preset
   / Remote / Pages" example message found early in this investigation (from the community reference),
   and structurally distinct from the `pageTemplate=1` message shortcut we've been using all session
   (different header shape, per-entry length bytes instead of one combined length, no trailing `04`).

**In short**: plugin-parameter text on screen is never anything more exotic than a plain string,
sourced from a Bitwig API observer callback, copied through a couple of local caches, and written into
a named field slot via the general compose SysEx shape. There is no glyph-selection, color, or
coordinate mechanism anywhere in this pipeline — "which glyphs get drawn" is entirely determined by
*what text Bitwig's API handed the driver*, not by anything screen/graphics-specific in the protocol.

**Why views differ per-DAW** (Bitwig vs. Reaper vs. Cubase, per the user's own recollection of using
this hardware): the whole *page layout* — which page templates exist, which fields each one has, when
to switch pages, what text goes in which field and when — is a property of **which DAW's own control
script is running**, not the device. `PANORAMA_P1.control.js` is Bitwig-specific; a Reaper integration
is a structurally different script (very likely built on Reaper's own ReaScript/control-surface API,
which we have not examined at all this session — only the Bitwig driver is in hand). The device
firmware just renders whatever page template + field-text instructions it's sent; it has no built-in
notion of "the Bitwig view" vs. "the Reaper view" beyond whatever fixed set of page-template IDs its
firmware happens to support, which each DAW's script then uses however its author chose to.

## Eighth finding: LED on/off feedback confirmed working

Per the official driver, LED state for illuminated buttons is set via plain **Control Change**
messages (`sendChannelController`, a normal `0xBn` status byte) — no SysEx at all — sent on the **same
CC number as that control's own input** (standard MIDI feedback convention). Extracted every
`Z81134B25E3E8D3DA8(0, CC.<name>, ...)` (the driver's `sendChannelController` wrapper) call site from
`PANORAMA_P1.control.js` to get the candidate list:

```
16-23   select/track buttons (indexed +0..7)
106-110 menu buttons (indexed +0..4)
84      transport Play
80      transport Loop/Cycle
85      transport Record
29      arranger automation write
99      sent unconditionally =127 during nektarinit's post-init task (static "connected" indicator)
```
(64-71 and 48-55, the param/pan encoder CCs, are also written back by the real driver, but to set an
LED **ring position** (0-127), not a simple on/off — untried here.)

Built `prototypes/panorama-p1/src/bin/led_test.rs`: sends all of the above CCs to value 127 (`on`) or 0
(`off`) as plain Control Change messages on the default port, no SysEx involved. **Confirmed on real
hardware**: the Loop and Play buttons lit **green**, and the Record button lit **red** (from their idle
blue state), fully reversible back to blue with `off`. The select/track and menu button CCs showed no
visible change in this test — either the P1 doesn't have physically wired LEDs for those specific
buttons (plausible; some of this driver is shared across P1/P4/P6 which differ in physical controls),
or they need a different value/channel nuance not yet tried.

## Tenth finding: probing page templates one at a time, on real hardware

Per the user's request, testing each page-template ID directly against the device (not just reading
source), one at a time, to build a concrete "what does this one actually do" catalog.

### Template 2 — sets the bottom menu-button row

Replayed the known-real community-reference example verbatim: `06 02 04 00 05 00 00 00 00 00 00 01 01
2B 00 02 07 "Browser" 00 03 06 "Preset" 00 04 06 "Remote" 00 05 05 "Pages" F7` (pageTemplate `02`,
displayId `04`, 5 text entries). **Confirmed on real hardware**: the bottom row of 5 buttons, which
normally reads the page tabs (`Faders`/`Encoders`/`Cntrl Edit`/`Global`/`Setup`), now reads `+` /
`Browser` / `Preset` / `Remote` / `Pages` instead — a clean, direct, reproducible effect. Reran twice
for confirmation, identical result both times. This is genuinely useful: **template 2 + displayId 4 is
how the driver relabels the 5 bottom menu buttons**, independent of whatever page/tab is otherwise
active (the fader bars and live CC readout above it were untouched).

### Template 16 — sets the top title-bar row

Hand-built (no known-real example for this one): `06 10 01 <3 indexed entries>` — pageTemplate `10`
(16), displayId `01`, entries at index 1/2/3 with 4-char strings `"TTL1"`/`"TTL2"`/`"TTL3"`.
**Confirmed on real hardware**: the top title-bar row, which normally reads `Nektar` / `Panorama P1` /
`1-NEKTAR 1`, now reads `TTL1` / `TTL2` / `TTL3` in the same three colored segments. The bottom tab row
had reverted to its normal `Faders`/`Encoders`/`Cntrl Edit`/`Global`/`Setup` labels (i.e. the previous
template-2 test's custom labels don't persist across a new write — each write only touches its own
targeted field, nothing else carries over). **Template 16 + displayId 1 = the 3-segment title bar.**

For reference, the full `DISPLAY_ID` enum (confirmed by direct extraction, not memory):
```
0 = padState/padValue      1 = titleBar (3 slots)      2 = currentParameterInfo (1)
3 = currentParameterValue  4 = menuButtonLabel/Type (5) 5 = pageLabels (3)
6 = ctrlElementName (8)    7 = ctrlElementValue AND faderElementValue (8/9, shared id)
8 = unidentified so far
```

### Template 16, displayId 6 — sets the 8 knob NAME labels (real plugin-parameter-name field)

`06 10 06 <8 indexed entries, one per encoder>` — `CUT`/`RES`/`ATK`/`DEC`/`SUS`/`REL`/`DRV`/`MIX`
(deliberately synth-parameter-shaped names). **Confirmed on real hardware, cleanly**: all 8 knob
labels changed to exactly this text, laid out in the same 4×2 grid the native "Encoders" view already
uses. This is the exact field the traced Ninth-finding pipeline (`ctrlElementName = parameterNames`)
writes plugin parameter names into — now demonstrated directly, with our own arbitrary text, no live
Bitwig session needed. **Template 16 + displayId 6 = the 8 encoder name labels.**

### Template 16, displayId 7, indices 1-8 — the paired VALUE field — composes correctly with displayId 6

First attempt sent the 8 value-shaped strings (`1.2k`/`45%`/`12ms`/`80ms`/`0dB`/`200ms`/`30%`/`wet`) via
`06 10 07 <8 entries>` as a **separate `msg_test` process run** from the displayId-6 (name) test above.
Result looked like the value strings had simply overwritten the name strings in the same position —
but that was misleading: each separate process run does its own init sequence from scratch, which
resets `OutputState` and clears whatever the previous run had drawn, so this wasn't a real "do these
two fields share one slot" test at all.

**Corrected test**: added a `--raw2 <msg1> <msg2>` mode to `msg_test.rs` to send both writes in the
*same* session (one init, two SysEx messages ~100ms apart, no reset in between) — the same message
order as before (displayId 6 = names, then displayId 7 = values). **Result: both rendered together,
simultaneously** — the value string above each knob, the name string below it (`1.2k` above `CUT`,
`45%` above `RES`, etc.) — exactly matching how a real Bitwig parameter display should look. So
`ctrlElementName` and `ctrlElementValue` (indices 1-8) are indeed two independent, simultaneously
visible fields, each keeping its own last-written content — the earlier "they overwrite each other"
read was an artifact of testing across separate process restarts, not real coexistence behavior.
(The real driver's source shows `ctrlElementValue` and `faderElementValue` further sharing this *same*
displayId 7, distinguished only by index range — `ctrlElementValue` uses indices 1-8,
`faderElementValue` uses indices 9-17 (`1+b+8`) — the 9-17 range almost certainly renders near the
fader bars instead; not yet tested directly.)

### Template 17, displayId 1 — same 3-segment title bar as template 16

`06 11 01 <3 entries>` — `T17A`/`T17B`/`T17C`. **Confirmed on real hardware**: same effect as template
16's displayId 1 test, the 3-segment title bar changed to this text. Suggests `displayId 1` (titleBar)
is a shared/universal field across the real DAW page templates (16-22), not something unique to
template 16 specifically — plausible, since every Bitwig page would want a common title-bar region
regardless of which specific page is active. Not yet tested whether templates 17's *other* fields
(ctrlElementName/Value, etc.) behave identically to 16's or differ in slot count/meaning — paused here
for now, one template at a time as requested; templates 3/4/5 and 18-22 remain unexplored.

## Eleventh finding: what we owe the Bitwig driver's exact approach, vs. what's its own choice

A question worth answering explicitly before building hacpad's own equivalent: how closely must our
own implementation mimic `PANORAMA_P1.control.js`'s specific approach?

**Firmware-level contract — must match exactly, no freedom:**
- The manufacturer prefix and command bytes (`06`/`08`/`09`/`0B`).
- The port assignment (Instrument = default/port0, Internal = port1) and the init sequence — confirmed
  required; nothing renders without it.
- The page-template IDs (1, 2, 16-22...) and `displayId` numbers (1=titleBar, 6=name, 7=value...) —
  firmware-defined enum values, not a Bitwig convention.
- The message *shape* per template — template 1's fixed `00 00 <len> <text> 04` shape vs. the general
  `<index><len><text>` compose shape are genuinely different firmware parsers (confirmed: sending the
  wrong shape to template 1 got a NAK-like reply).
- The ~127-byte single-message length ceiling (a firmware buffer limit).

**The driver's own software choices — we owe none of this:**
- *What text* goes in a field is arbitrary (confirmed repeatedly with made-up content).
- *When* to send updates — the driver diffs against previously-sent state and only resends changed
  fields on its own `flush()` cadence; there's no need to replicate any of that bookkeeping, we can just
  send whenever we want.
- *Where* field content comes from (Bitwig API observers, `BufferedElementArray` caching) is pure
  JS-side plumbing, irrelevant to us — hacpad can source field content however it likes.
- *Which* page to show and when is the driver's own UX design (see below) — not something we need to
  copy.

**Open/untested**: whether the firmware expects periodic traffic as a keepalive over a genuinely
long-running session (only short bursts tested so far).

### Where does page-navigation actually come from? (mostly not the DAW)

Cataloged every `setActiveDisplayPage(...)` call site in the driver to see what actually triggers a
page switch:
- **Nearly all of them fire from inside the physical Control-Change/button handler** — switching to
  Mixer / Instrument / Transport / Internal mode happens because the user pressed a specific physical
  Mode button, handled entirely locally in the driver's own input-handling code. Bitwig is not
  proactively pushing these changes.
- **One genuine exception**: a Nektar-specific "Nektarine" plugin-browser page auto-switches based on
  live Bitwig session state (`gNektarineInstance` changing) — real DAW-driven switching, but scoped to
  that one special feature.
- **The only other DAW-driven case**: the one-time default page on connect —
  `host.scheduleTask(() => setActiveDisplayPage(Mixer), 500ms)` in `nektarinit()`.

**Conclusion**: ordinary page navigation is a local, physical-button-driven concern, not something the
DAW actively orchestrates moment to moment. hacpad doesn't need to replicate any "when should the page
change" logic from Bitwig's side — we can just handle the same physical Mode-button CCs ourselves and
send whichever pageTemplate SysEx we want in response, entirely our own design.

## Twelfth finding: the "big font" readout is real, writable, and confirmed with our own text

Every native screenshot this whole session has shown a noticeably larger-font single-line readout in
the top-left corner (`MIDI CC 3`, updating live as physical controls are touched). Hypothesis: this is
`displayId 2` (`currentParameterInfo`) from the traced pipeline (`Z810A716763F1A2A63` /
`lastCurrentParameterInfo`) — normally driven by the firmware's own live-touch monitor in the absence
of an explicit override, but a real writable field like any other.

Tested: `06 10 02 <one entry, index 1, "HACPAD">`. **Confirmed, cleanly**: the big-font readout changed
from `MIDI CC 3` to `HACPAD`, in the exact same large font and position. `displayId 3`
(`currentParameterValue`, the paired big-font value readout) not yet tested.

**Caveat on using this field for branding**: per the traced Ninth-finding pipeline, `displayId 2`'s
real semantic role is "name of whatever was just touched/adjusted" (`Z810A716763F1A2A63`, set on
interaction to things like `"Set Loop Start: "` or a track/clip name) — a contextual touch-feedback
readout, not a permanent slot. It renders our arbitrary text fine right now only because nothing else
is actively writing to it. If hacpad ever drives its own live touch-feedback through this same field,
a static logo written here would get overwritten by the next interaction. The title bar (`displayId
1`) is the better fit for anything meant to stay put; this one is better reserved for its intended
live-feedback purpose, or used only transiently (e.g. a startup splash before real use begins).

### Template 18 — a genuinely different layout: faders, not knobs

`06 12 01 <3 title-bar entries>` then `06 12 06 <8 ctrlElementName entries, "N1".."N8">`, sent together
via the new `--rawN` mode. **Confirmed on real hardware**: the title bar behaves identically to
templates 16/17 (`T18A`/`T18B`/`T18C`). But the main content area is different — **8 vertical fader
bars** (split visually into two groups of 4), with our `ctrlElementName` text (`N1`-`N8`) rendered as
small labels beneath each fader, not next to a knob. This is a distinct widget from template 16/17's
4×2 knob grid, using the *same* `displayId 6` field. Consistent with template 18 being **Instrument
Layer Container** (channel-strip/layer semantics naturally fit a fader-per-layer UI). The bottom tab
row wasn't visible in this framing — not clear whether it's actually absent for this template or just
cropped out of frame.

### Template 19 — fader layout again, continuous row (not split), bottom tabs visible

Same test pattern (title bar + 8 `ctrlElementName` entries). **Confirmed**: title bar works
identically (`T19A`/`T19B`/`T19C`). Content area shows all 8 faders in one continuous row (not split
into two groups of 4 like template 18), with `N1`-`N8` labels beneath each. The bottom tab row
(`Faders`/`Encoders`/`Cntrl Edit`/`Global`/`Setup`) and the big-font `MIDI CC 3` readout are both
visible here (confirming template 18's missing tab row was just camera framing, not a real absence —
untouched fields like the big-font readout keep showing their native live value when we don't write to
them). Broadly similar to 16/17/18: title bar and name-label fields behave consistently, only the
specific widget/layout cosmetics differ per template.

### Template 20 — same as 19

Same test pattern, essentially identical result to template 19 (continuous 8-fader row, title bar and
name labels both working, bottom tabs and big-font readout visible). No distinguishing cosmetic
difference found from this probe alone.

### Template 21 — a 4×4 pad grid (Drum Machine Container)

Same test pattern. **Confirmed, distinctly different again**: the content area shows a **4×4 grid of
pads**, rows labeled `A`/`B`/`C`/`D` (standard drum-machine bank convention), 4 columns. Our 8
`ctrlElementName` entries (only indices 1-8 sent) filled the **bottom two rows only** (`A`: `N1 N2 N3
N4`, `B`: `N5 N6 N7 N8`), leaving rows `C`/`D` empty — consistent with a real 16-pad grid where we just
didn't supply entries for indices 9-16. Title bar worked identically. This is a strong match for
**Drum Machine Container** (16 pads = `padState`/`padValue`, `displayId 0`, matches the driver's own
pad-page field sizing).

### Follow-up on template 21: `displayId 0` (padState) does NOT label the other 8 pads

Good question raised: only 8 of the 16 pads got labels (`ctrlElementName`, `displayId 6`, only has 8
slots) — is there a way to label the other 8? Tested the obvious candidate: `displayId 0` (`padState`,
confirmed 16 slots from the driver's own `initArray(-1, 16)` reset code) with 16 text entries
(`P1`..`P16`). **Result: no visible effect at all** — the pads kept showing whatever `ctrlElementName`
had last set (still `N1`-`N8` from an earlier test, unchanged), not our new `P1`-`P16` text anywhere.
So `padState` is not a text field — consistent with its name, it's almost certainly a boolean-ish
on/off/lit indicator per pad, not a label. **Confirmed conclusion: this protocol has no discovered way
to put text on the top 8 pads** — only the bottom two rows (indices 1-8, via `ctrlElementName`) are
labelable at all.

### Template 22 — same pad-grid family as 21

Same test pattern, same pad-grid layout as template 21 (rows labeled, our 8 entries filling two rows,
title bar identical). Not distinguished from 21 by this probe; likely a closely related pad-based page
(e.g. a second drum-container variant, or Clip Launcher reusing the same grid widget for clip
slots — the source's `Z810DEA1F33EE35023` page object had both `CLIPS`/`SCENES` sub-modes and pad
handling, consistent with 21/22 being two faces of the same grid-based page).

### Template 3 — Transport Launcher (confirmed identity from source)

Same test pattern. **Confirmed, different again**: content area shows two labeled bars `L:` / `R:`
(likely loop left/right locator positions, matching the Transport page's known `pageLabels` fields —
`transportInPosition`/`transportOutPosition`/`transportPosition` from the earlier-read
`updateOutputState`), with our 8 `ctrlElementName` entries filling a 2-row × 4-column grid beneath.
Title bar identical. This is the one template whose real identity we already had direct source
evidence for (`a.pageTemplate=Z8114CB0CF3E757C0C.Z81081794D3F409C65` inside the Transport Launcher's
`updateOutputState`) — the rendered layout is consistent with that.

### Template 4 — a single fader plus a vertical bulleted list

Same test pattern. **Confirmed, distinctly different**: one tall fader bar on the left, and our
`ctrlElementName` entries rendered as a **vertical bulleted list** on the right (`N1`-`N5` visible,
each with a small dot indicator; `N6`-`N8` likely present but cropped below frame). Reads like a
menu/preset-list style page — single-value-plus-list, unlike any of the grid/fader-bank layouts seen
so far. Title bar identical.

### Template 5 — clean 2×4 button grid, no fader/knob widgets

Same test pattern. **Confirmed, different again**: a simple 2-row × 4-column grid of flat button-style
labels (`N1`-`N4` top row, `N5`-`N8` bottom row), no fader bars or knob dials visible at all — the
plainest layout tested so far. Matches the earlier source grep showing template 5 used inside the
Instrument page's macro/envelope sub-view branch.

### Template 0 — confirmed genuine "reset" sentinel, not a real page

Same test pattern. Title bar still worked (`TT0A`/`TT0B`/`TT0C`). But the `ctrlElementName` write (8
entries, `N1`-`N8`) had **no visible effect at all** — the content area just showed the native default
fader view with plain channel numbers (`1`-`8`), no custom text anywhere. Confirms template 0 really is
just the "unknown/reset" sentinel its name suggests, not a displayable page — the firmware falls back
to its own default rather than honoring composed field content for this one value.

### Summary: all 13 page-template values now probed

| Template | Layout confirmed | Likely identity |
|---|---|---|
| 0 | reset sentinel, ignores field writes | none (internal state only) |
| 1 | one-shot message overlay (see Seventh finding) | `MESSAGE`, unused by real driver |
| 2 | (title bar untested) — sets bottom menu-button row | generic browser/menu |
| 3 | `L:`/`R:` bars + 2×4 grid | Transport Launcher (confirmed from source) |
| 4 | 1 fader + vertical bulleted list | menu/preset-list style |
| 5 | plain 2×4 button grid, no fader/knob widgets | Instrument macro/envelope sub-view |
| 16 | 4×2 knob grid | Mixer (default page on connect) |
| 17 | same knob grid as 16 | (not distinguished from 16 yet) |
| 18 | 8 faders split into two groups of 4 | Instrument Layer Container |
| 19 | 8 faders, one continuous row | (not distinguished from 20) |
| 20 | same as 19 | (not distinguished from 19) |
| 21 | 4×4 pad grid (rows A-D) | Drum Machine Container |
| 22 | same pad-grid family as 21 | Clip Launcher / second drum variant |

`displayId 1` (titleBar, 3 segments) and `displayId 6` (ctrlElementName, up to 8 slots) behave
consistently across every real template (2-22) — only the surrounding widget chrome (knobs vs. faders
vs. pads vs. list) differs per template, confirming the Eleventh-finding theory: content fields are
uniform, the DAW-facing protocol never touches which *widget* gets drawn, only what text goes in it.

### Follow-up on template 18: it actually has 16 name slots, not 8

Good catch raised: the first template-18 test only sent 8 `ctrlElementName` entries — was that the
real limit, or an artifact of only testing 8? Resent with 16 entries (`X1`..`X16`, indices 1-16).
**Result: all 16 rendered** — each of the 8 fader bars now shows **two stacked labels** (`X1` above
`X5`, `X2` above `X6`, `X3`/`X7`, `X4`/`X8` in the left group; `X9`/`X13`, `X10`/`X14`, `X11`/`X15`,
`X12`/`X16` in the right group). So template 18's `ctrlElementName` field genuinely has **16 usable
slots**, not 8 — the earlier test simply hadn't sent enough entries to reveal the second label row.
Correcting the summary table below accordingly.

### Follow-up on template 4: genuinely capped at 5, not under-tested

Same check as template 18: resent with 16 entries (`Y1`..`Y16`). **Result: still only 5 visible**
(`Y1`-`Y5`), unchanged from the original 8-entry test. Unlike template 18, this is a **real capacity
limit** for this widget (or at least a fixed visible-window size with no observed way to scroll it via
plain field writes) — not an artifact of under-testing.

### Thirteenth finding: writes are partial updates — unaddressed slots stay stale, not blanked

A pattern visible across several tests now, worth stating explicitly: **writing to a field only ever
touches the indices you actually send** — every other slot keeps showing whatever it last held,
whether that's real content from an earlier write in the same session or the template's own default.
Concretely: the `displayId 0` (padState) test above left the pads showing `N1`-`N8` from a *previous,
unrelated* test on the same template, completely unaffected by the new (ineffective) write. This is
consistent with the driver's own diffing design (`OutputState.prototype.send()` only sends indices
that changed since the last flush) — but importantly, **this is real, observable firmware behavior,
not just a JS-side optimization we could ignore**: the device itself appears to hold independent
per-slot state, and a write that doesn't mention a slot leaves it untouched.

**Correction, after more careful isolated testing (see below): switching `pageTemplate` does NOT reset
everything indiscriminately.** The original claim here was based on `pageTemplate=1` (the message
overlay) wiping a `pageTemplate=16` title bar — but `pageTemplate=1` is a special, separate rendering
pathway (its own fixed byte shape, see "Seventh finding"), not representative of switching between the
*real* content templates (2, 3, 4, 5, 16-22). Tested cleanly, isolating exactly one field per message
(so a template-switching message never also happens to rewrite the field being checked):
- `06 16 01 <title bar A>` then `06 18 02 <bigfont B>` (a **different** template, touching a
  **different** field, never touching the title bar again) → **the title bar still read exactly what
  message A set**, unchanged, even though the active template and widget genuinely changed (knobs →
  faders, correctly reflecting template 18).
- `06 16 06 <8 names>` then `06 18 02 <bigfont>` (same shape, but the first message set *names* instead
  of the title bar) → **the names were completely gone** — the fader label area was totally blank
  after the switch to template 18.

**So there are two different kinds of fields**: `displayId 1` (title bar) behaves like **global
chrome** that survives a switch between real content templates untouched, while `displayId 6`
(names — and very plausibly 0/7/8, the other per-widget content fields, untested directly but assumed
by analogy) is **per-template content** that gets cleared whenever the active template changes to a
different one, even when the switching message never touches that field. Within the *same* template,
the original "partial write, stale slots persist" observation still holds for content fields (see the
`displayId 0` padState example above, still valid — that test never actually changed templates).

**Consequence for hacpad**: yes, genuine differential/partial updates work — you can update just the
title bar, or just one content field, without resending the whole screen. But a template switch does
clear per-template content fields (names, values, pad labels, etc.) even if the switching message
doesn't touch them, while it leaves the title bar (and probably other "chrome" fields) alone. Design
around that: treat the title bar as safe to set once and leave alone across template changes, and
always replan content-field writes fresh after switching templates.

### `main.rs` integration note: switching pageTemplate resets the page

Tried wiring `write_title_bar()` into the real `panorama-bridge` binary as its startup branding,
followed immediately by the existing `write_message()` (pageTemplate 1) call. Result: only the
`write_message` text rendered — the title bar was gone. **Switching pageTemplate mid-session resets
the whole page context** (matches the real driver's `resetOutputToUnknown()` call whenever
`pageTemplate` changes in `OutputState.prototype.send()`), so a pageTemplate-1 write right after a
pageTemplate-16 write wipes out what the 16-write drew. These two mechanisms don't currently coexist
in one session; `main.rs` was left using `write_message` only for its default screen text pending
further mapping of whether any single template offers both a title bar and a message-shaped field.

### Follow-up: `displayId 4` (menu buttons) works with ANY template, not just template 2

The original template-2 test used `displayId 4` together with `pageTemplate 2` specifically, leaving
an open question: does relabeling the bottom menu-button row require switching to template 2 (which
would conflict with whatever content template is active, since switching templates resets everything
— see "Thirteenth finding"), or is `displayId 4` just an ordinary field addressable within *any*
template? Tested: `06 10 04 <5 entries>` (template `16`, i.e. Mixer/knobs, not `2`). **Confirmed: this
works** — the bottom row changed to the new labels while the title bar, big-font readout, and all 8
knobs stayed completely intact. So `displayId 4` composes freely with content fields on the same
template; there was never a need to switch to template 2 for this at all. This is now wired into the
webcam-viewer's screen simulator and the Rust service (see `write_tabs` in `src/lib.rs`) — the bottom
tabs input was previously cosmetic-only in the browser mockup, never actually pushed.

### `displayId` addressing is template-independent, confirmed across the board

Also tested `displayId 2` (big font) on template `21` (the pad grid) — a template about as visually
different from `16` (knobs) as any two tested. **Confirmed working**: `"PADFONT"` rendered in the same
big-font slot, everything else (title bar defaulting back to `Nektar`/`Panorama P1`/`1-NEKTAR 1` since
untouched, and stale pad labels left over from an earlier test) exactly as expected. Combined with the
title-bar (7 templates), names (6 templates), and menu-button (2 templates) results above, every
`displayId` tried so far works identically regardless of which template is active — strong evidence
that `displayId` is a genuinely universal addressing scheme, and the page template *only* determines
which widget renders the content fields (knobs vs. faders vs. pads vs. list), not which fields exist.
Not exhaustively proven for every template × displayId combination (13 × 9 = 117 possible pairs, a
few dozen tried), but no exception found yet.

### Fourteenth finding: title bar isn't the only "chrome" field — big font and menu buttons persist too

The Thirteenth finding's "title bar is chrome, everything else is per-template content" rule was
itself under-tested — it only ever compared title bar (persists) against names (cleared). Ran a
clean, three-step isolated test through the new webcam-viewer/service architecture (`ws://…:8091`),
photographing the real screen after each step:
1. **Full-state baseline** (one WebSocket frame, every field at once — the "Send full state" button /
   the underlying `sendFullState()` path in `index.html`): title bar `HACPAD`/`DIFF TEST`/`BASE`,
   big font `BASELINE`, 8 knob names+values, 5 tabs, on template 16 (knobs). Photo confirms all of it
   rendered.
2. **Template-only switch**: sent `{"layout": "faders-split", "titleBar": [...same 3 segments...]}`
   only — no bigfont, names, values, or tabs in this message at all. Photo shows: title bar unchanged
   (as expected), **but also big font (`BASELINE`) and all 5 tabs (`Faders`/`Encoders`/`Cntrl
   Edit`/`Global`/`Setup`) still showing, untouched** — only the fader labels went blank (8 empty fader
   tracks, template 18's widget correctly rendered but with no `ctrlElementName` content).
3. **Isolated names-only resend**: sent `{"layout": "faders-split", "names": ["X1".."X16"]}` (16
   entries, to also re-confirm the stacked-label mapping). Photo shows the labels appear correctly
   (`X1`-`X4`/`X5`-`X8` stacked on the left group of 4 faders, `X9`-`X12`/`X13`-`X16` on the right),
   while title bar, big font, and tabs are still exactly what they were in step 1 — completely
   unaffected by either the switch or this resend.

**Revised model**: it's not just `displayId 1` that's "global chrome" — `displayId 1` (title bar),
`displayId 2` (big font), and `displayId 4` (menu buttons/tabs) all persist across a switch between
real content templates. Only `displayId 6` (`ctrlElementName`) is confirmed **per-template content**
that gets cleared on switch even when untouched by the switching message. `displayId 7`
(`ctrlElementValue`, paired with names) is still not directly isolated — the knobs-only values field
was never present during the faders-split leg of this test — but by analogy to names (its paired
field, same per-widget-content role) it's assumed to be per-template too, not chrome.

**Consequence for hacpad**: the safe-to-set-once "chrome" set is bigger than first thought — title
bar, big font, *and* the bottom tab row can all be set once and left alone across template switches,
not just the title bar. Only the actual per-widget content (names, presumably values, pad labels)
needs replanning after every switch. This is exactly the differential-update model now implemented in
`tooling/webcam-viewer/index.html`'s `pushToDevice()`: editing any one field sends only that field's
write; changing the template dropdown alone sends only a title-bar-carried switch message (since title
bar is guaranteed chrome and is enough to carry the new template id); a dedicated "Send full state"
button bypasses all of this and sends every field in one frame, for a well-defined known-good baseline
(used to reset the device between tests above) or a manual resync if a client's diff bookkeeping and
the device's real state have drifted apart.

### Fifteenth finding: a full numeric sweep finds 4 new page_templates and a hard validity ceiling, plus 1 new displayId

Prompted by "there are still unexplored templates and displayIds likely... by all, I mean more than the
numbers found in Bitwig" — rather than only testing the ids named in the driver's source, swept the
actual byte ranges directly against hardware.

**`displayId` sweep** (template fixed at 16, one message per id, ids 0-32 plus edge bytes 40-255, all in
one session): only **`displayId 3`** produced new visible content, beyond the already-confirmed
0/1/2/4/6/7. It renders in the **top-right of the big-font row**, directly paired with `displayId 2`
(which sits top-left of that same row) — i.e. `displayId 2` is the parameter *name* and `displayId 3`
is the parameter *value*, a matched pair for the "currently touched" readout (`currentParameterInfo` /
`currentParameterValue` in the driver's own naming). No other id in the swept range (5, 9-32, or any
edge byte up to 255) produced any new visible content anywhere on screen. (Two entries in this same
sweep, `displayId 1` and `6`, unexpectedly showed no effect despite being independently confirmed
reliable elsewhere this session — most likely an isolated dropped message at this sweep's 600ms
cadence, not a real behavior change; not treated as a finding.)

**Correction: displayId 8 was wrongly cleared here.** This same combined-sweep photo also included
`displayId 8`'s write, and at the time nothing distinctive was noticed for it. It was, in fact, already
producing visible content (see the Nineteenth finding) — just missed on a single quick visual pass
through a busy combined photo. Both 5 and 8 are confirmed real, working fields; see the Nineteenth
finding for 8 and its own entry for 5.

**`page_template` sweep** (`displayId` fixed at 1/title-bar, one message per template id, ids 6-15,
23-32, and edge bytes up to 255): the first several new values worked and rendered genuinely distinct,
previously unseen widgets — **6** (a list with one row highlighted/selected, small row-number labels on
the left), **7** (four buttons labeled `S1`-`S4` plus a `B` indicator — plausibly a scene/clip-launch
row), and **8/9** (a vertical list of up to 8 rows, each paired with a `Pre` label — plausibly a
browser/preset list; 8 and 9 rendered indistinguishably in every test, not yet conclusively split
apart). Every value from **10 through 255** tried after that (10-15, 23-32, 40, 64, 96, 127, 160, 200,
255) showed **no further change at all** — the display stayed frozen on template 9's list widget for
the rest of that sweep, all the way through byte value 255.

That raised an obvious question: is 10+ really invalid, or did the device just get confused by rapid
consecutive template changes and freeze? Ruled out with two **isolated, single-message, fresh-session**
tests (fresh init, one write, nothing else touched): sending `page_template = 23` alone rendered the
**default post-connect mixer screen** (as if no custom template had been set at all this session), and
`page_template = 10` alone did exactly the same. Neither was a leftover cached photo — both were
captured live, synced directly to the test binary's own "holding for photograph" stdout line (see the
methodology note below), not to an external timer.

**Conclusion**: `page_template` is not a free 0-255 byte — only a specific allow-listed set of values is
actually recognized by the firmware (confirmed so far: 0-9, 16-22). Writing an unrecognized value is a
**silent no-op**: it does not error, does not glitch, and does not reset anything — the display simply
keeps showing whatever the *last valid* template rendered (or the untouched default screen, if no valid
template has been set yet this session). This is a materially different failure mode from the earlier
"switching pageTemplate resets the whole page" behavior (Thirteenth/Fourteenth findings) — that only
applies to switching between two *valid* templates.

**Methodology note (a real gotcha, not just a footnote)**: capturing a photo by waiting on an external
signal (a background-task completion notification, a fixed sleep) is not reliable proof of what a test
rendered, because **the EXIT sequence sent at the end of every `msg_test` run resets the display back to
its own default appearance** (default title bar, mixer/knobs widget, generic CC numbering). By the time
an external notification round-trip resolves, the whole process — including its exit — may have already
finished, and the "photo" ends up showing the post-exit default, not the test's actual write. The fix:
have the *same* script that sends the SysEx also read the test binary's stdout live and capture the
instant it prints "Holding for 8s" (or "Sending message N of M" for a multi-step sweep), so the photo is
synced to a real event in the process itself, never to wall-clock guesses or asynchronous notifications.
This is also why a naive one-photo-per-value loop that fully restarts the binary per value (paying the
whole init/hold/exit cost each time) is much slower than necessary for no benefit: a single session can
send many independent field writes back to back (they don't interfere, per the Thirteenth/Fourteenth
findings) and only needs one final photo, while a template sweep specifically needs one photo *per*
step (since template changes visibly replace the whole screen) but still only needs one process/session
for the whole sweep, not one per value.

### Sixteenth finding: templates 21 and 22 are genuinely distinct, not the same pad-grid family

Asked directly: "is template 21 and 22 separable?" The original tests only ever sent 8
`ctrlElementName` entries to either template and saw the same-looking 4x4 grid with the bottom two rows
filled — not enough to tell them apart, so they'd been noted as "same family, not distinguished."
Re-tested with a fuller isolated probe per template (fresh session each: title bar + big font + tabs +
**16** names, mirroring the template-18 stacked-label test): **21 renders a full 4x4 grid, rows A-D, all
16 pads individually labeled** (`X1`-`X16`, one label per pad, not stacked); **22 renders only a 3x4
grid, rows A-C** — `X13`-`X16` don't appear anywhere, there's no 4th row rendered at all, not just an
unlabeled one. So they ARE separable: 21 is the full 16-pad grid, 22 is a distinct 12-pad variant.
Corrected `protocol.json`'s templates 21/22 entries and the webcam-viewer mockup accordingly (it
previously had one shared "pads" option assuming 8, then 16, slots for both).

### Seventeenth finding: the page_template validity ceiling — confirmed for 0-243 (CORRECTED, see below)

The Fifteenth finding sampled representative values (0-32, plus edge bytes up to 255) and inferred a
validity ceiling around template 22. Asked directly whether that was a real exhaustive search "all the
way to 255" — it wasn't, so ran the actual full sweep: **every single byte value 0-255**, one
`page_template` write each (`displayId` fixed at title-bar), photographing after every step in one
continuous session.

**Result** (at the time): the content area changes correctly through templates 0-9 and 16-22 (each
rendering its own confirmed widget), and every value from 23 onward appeared to be a no-op — spot-checked
photos at 23, 24, 30, 100, 150, and 200 were all pixel-identical (frozen on template 22's 3x4 pad grid).
This was originally reported as exhaustively confirmed through 255.

**Correction (see Eighteenth finding): that claim was wrong.** The sweep script only checked whether each
photo capture succeeded — it never checked the child process's exit code or scanned for an error line.
The underlying `msg_test` process actually died silently partway through (an ALSA transmission error at
byte 244), and the wrapper's loop simply exited via EOF and printed "DONE" as if the sweep had finished
normally. The spot-checked values (up to 200) remain validly confirmed as no-ops — those sends
genuinely happened before the crash — but **245-255 were never actually tested** by that run. See the
Eighteenth finding for the real cause and the follow-up test that actually covers that range.

**Confirmed valid `page_template` set (still holds): {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 16, 17, 18, 19, 20,
21, 22}.** Nothing else in 0-243 does anything; 244-255 covered separately, see below.

(A same-style exhaustive sweep of `displayId` 0-255 was run alongside this and hit the exact same silent
crash — see the Eighteenth and Nineteenth findings.)

### Eighteenth finding: two SysEx bytes (0xF4/0xF5) are untransmittable, and my sweep scripts weren't checking for that

Prompted by being caught not noticing the above: "so the process died and you did not notice?" — yes,
exactly that. Every sweep script up to this point only checked whether the HTTP photo capture succeeded,
never the child process's exit code or its output for an error line. Fixed going forward: check
`returncode` and scan for an `Error:`/panic line before ever trusting a "DONE" as real completion.

Re-tested the tail of the range (245-255) properly this time: a **fresh process per value**, blank
(reset) first, exit code and stderr checked explicitly. Result: **byte 245 (`0xF5`) fails identically**
to 244 (`0xF4`) — `Error: InvalidData("ALSA encoder reported invalid data")` — while **246 through 255
all transmit successfully** (`returncode=0`, no error). `0xF4` and `0xF5` are the only two byte values in
the entire 0-255 range where the ALSA rawmidi encoder itself refuses to send the message — not a device
no-op, a hard failure at the transport layer, before the bytes ever reach the P1. (Notably, `0xF7` (247) — the literal SysEx end-of-message byte — transmits FINE when
embedded as a `page_template`/`displayId` data byte; so does `0xF6` (246) and every other realtime status
byte tried up to 255. Whatever `0xF4`/`0xF5` trip in the encoder isn't simply "any byte >= 0x80", since
most such bytes pass through without complaint.) Practical upshot: `page_template`/`displayId` values 244 and 245 cannot be used at
all via this raw-MIDI path; 0-243 and 246-255 can be sent (though per the Seventeenth finding, only
{0-9, 16-22} of them do anything on this device).

### Nineteenth finding: a new field exists at displayId 8 — but a false trail at 33/34 got there first, and the lesson matters

A full-range `displayId` sweep (all values 0-255 set to their own number, on template 16 in one session)
revealed a genuinely new UI element never seen before: a box on the right side of the content area, white
border, an up-triangle near the top and a down-triangle near the bottom — a scroll/increment affordance
of some kind. Tried to isolate exactly which id caused it by **range-halving bisection** — test 0-127,
then 0-63, then 33-63, narrowing down to a final "33 or 34" — each step a fresh `msg_test` process, no
explicit reset in between steps.

**That whole bisection was invalid, and re-testing 33 and 34 in isolation (each freshly blanked first)
proved it: neither one, alone or together, reproduces the box at all.** The real cause, found only once
told directly to blank the screen before every single test ("ensure to FIRST blank the screen... do what
I said. set all the display fields to the matching number. then, cycle through all the templates with a
clean slate inbetween"), is **displayId 8** — confirmed with a proper blank-then-set-one-id-then-photograph
cycle across ids 0-127, no state ever carried between steps. Adjacent-photo diffing across that clean run
shows a sharp, unambiguous jump exactly at id=8, and the box (with an "8" label rendered inside it) is
plainly visible in that single isolated photo.

**Why the bisection was fooled**: none of the range-halving steps ever reset the device to a clean slate
before testing a new range — each step was just a fresh `msg_test` *process* (a fresh MIDI *session*),
which is not the same thing as a fresh *display state*. We had already established elsewhere in this
document that displayed content survives across process restarts (title bar, big font, etc. all persist
that way) — that same persistence is exactly what makes an unblanked bisection unsound: whatever set the
box the first time (id 8, sent once, early, in the very first giant 0-255 combined sweep) just sat there,
completely undisturbed, through every subsequent range test, because none of them ever contained an id
capable of clearing it. The bisection kept "confirming" the box's presence in ever-narrower ranges purely
because the ranges kept being subsets of a session that had never been reset — it would have "confirmed"
presence in literally any range tested after the original id-8 write, including ranges that do nothing at
all. The fix, exactly as instructed: blank first, set exactly one thing, observe, repeat — never let two
different steps share undisturbed state.

**Consequence for future testing on this device**: never trust a multi-step isolation test (bisection,
range-narrowing, or any "vary one thing, hold the rest" protocol) unless every step starts from an
explicitly-cleared, verified-blank state. A fresh process is not a fresh screen.

### Twentieth finding: command byte 0x06 appears to be the only display-write command in the low range (negative result)

Item 3 of the ongoing checklist: only `0x06` (display write) has ever been used deliberately; `0x08`/`0x09`
(lifecycle) and `0x0B` (LED, from much earlier this session) are the only other confirmed command bytes.
Probed the rest of the low range directly: for each of `0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x07, 0x0A,
0x0C, 0x0D, 0x0E, 0x0F`, sent a blank (reset, via the confirmed `0x06`) then a test message shaped exactly
like a normal display write but with that byte substituted for the command byte (`<cmd> <template=16>
<displayId=1> <index=1> <len> "Cn"`), each in its own fresh process, exit code and stderr checked.

**Result: no candidate produced any new visible text anywhere on screen.** No `"C0"` through `"C15"`
label ever appeared; the screen only ever showed pre-existing stale content (from far earlier in the
session) or the device's own native readouts. All 12 processes exited cleanly (`returncode=0`, no error)
— these bytes are transmittable and don't crash anything, they just don't do a display write. Confirms
(does not newly discover) that `0x06` is the display-write command; no evidence of another one in this
range.

**A real confound worth flagging**: during this probe, the big-font readout visibly changed on its own
(`MIDI CC 3` → `MIDI CC 40`) and a burst of real Control Change input arrived (`B0 61 00` / `B0 61 7F` /
`B0 61 00`, i.e. CC 97) — neither of these was caused by anything sent. The Panorama P1 sits physically on
the desk during all of this testing, live and connected; its own knobs/encoders can be nudged (by hand,
vibration, or just resting near vibrating/moving hardware) at any time, and per the Twelfth finding, the
big-font field is genuinely wired to reflect "whatever was last physically touched" — so a real touch
during a test will legitimately change what's on screen, with no SysEx involved at all. Any future test
that includes physical-interaction as a signal (item 9 of the checklist) needs to control for this by
capturing a `known-quiet` baseline immediately before deliberately touching a control, since ambient
interaction can and does produce CC traffic and screen changes at any moment.

### Twenty-first finding: item 4 (0x0B LED control) — one correction, one negative, one inconclusive

Prompted directly: "for 4, we need the camera to see the whole controller!!" The earlier LED test (Ninth
finding-era `led_test.rs` work) was framed tightly on just the screen; widened the physical camera
position/angle so the full control surface — encoders, faders, the row of round buttons beneath them,
and the transport section — is in frame, then re-ran all three untested LED angles from the checklist.

**Correction: the select/track button LEDs (CC 16-23) DO work.** Previously reported as "no visible
change" — that was simply because those 8 round buttons sit below the fader row, outside the old tight
camera crop. With the wider framing, `led_test on` clearly lights all 8 with a ring around each button
(reported by direct visual inspection as **red**, not the pink/magenta the photo's color cast suggested
— consistent with the established camera-color-miscalibration pattern from the screen work). `led_test
off` fully reverts them, confirmed reversible. Loop/Play (green) and Record (red) LEDs reconfirmed
working as before, now also clearly visible in the wider frame.

**Negative result: encoder ring-position CCs (64-71, 48-55) show no visible effect.** Sent CC 64 = 100
(a `widget`-mode single-CC test) — no lighting change on any of the colored encoder knobs. Physically,
these knobs show no visible LED-ring hardware at all (plain colored plastic caps) — consistent with the
source comment that this driver is shared across P1/P4/P6, which differ in physical controls; this P1
unit most likely lacks the LED rings those CCs are meant to drive on other models in the family.

**Inconclusive: the 0x0B SysEx "Launcher" message.** Replayed the exact byte sequence extracted from
source (`0B 00 0F 00 08 "Launcher" 00 01 02 0F 00`) against real hardware for the first time this
session. Transmitted cleanly (no ALSA error, clean process exit) but produced **no observable effect**
anywhere in the now-wide camera frame — screen, all LEDs, and button states all identical before and
after. Since source ties `0x0B`'s call sites to `gBrowserOpen`/menu-focus state, it's plausible this
message only does something when a specific precondition is met that a bare replay doesn't reproduce
(e.g. the device believing it's mid-browser-navigation, a state we have no way to induce over MIDI
alone) — left as a genuine open question, not resolved either way.

### Twenty-second finding: 0x0B's real source context, and a clean negative result for every variant tried

Asked directly where the "Launcher" 0x0B string actually came from. Precise citation: **`PANORAMA_P1.control.js`**
(Bitwig's own bundled Panorama P1 driver, from Nektar's official
`Panorama_P1_P4_P6_Bitwig_Studio_Integration_Files_Linux_2023-06` package), found on disk at
`~/Downloads/support_871eda6c83a540c70/.../Nektar/PANORAMA_P1.control.js`. The exact call site (verbatim,
appears 3 times — lines 135, 159, 415):
```js
Z810DEA1F33EE35023.onView=function(){Z81136C4863E8BC4AF("F0 00 01 77 7F 01 0B 00 0F 00 08 4C 61 75 6E 63 68 65 72 00 01 02 0F 00 F7")};
```
`Z81136C4863E8BC4AF` is confirmed (line 12) to be a thin wrapper: `function Z81136C4863E8BC4AF(a){sendSysex(a)}` —
a genuine outbound-to-hardware call, not internal-to-Bitwig.

**Correction to the working theory**: 0x0B is not fundamentally an "LED" command — grepping every literal
0x0B string in the file turns up a dozen near-identical short payloads
(`0B 00 0F 00 00 00 01 02 <selector> 00`, selectors seen: `05, 07, 28, 29, 2B(00), 2B(02), 4F, 50, 51`,
plus one on a different sub-header `0B 00 2B 00` with selector `51`, and two that carry literal text
instead of just a selector byte — `"Launcher"` and `"Tab"` (`54 61 62`)) scattered through
`focusPanelAbove`/browser-open/browser-close/macro-view-toggle logic. "Launcher" is the odd one out in
carrying text; everything else is a bare selector byte. This looks like a general **navigation/panel-
coordination signal** to the device, not specifically an LED command.

**Tested all 11 distinct variants against real hardware** (blank-first, fresh process per value, wide
camera framing, exit code checked — all `returncode=0`, no errors, so every one transmits cleanly).
**Result: no variant produced any observable effect** — screen content, LED states, and button states
were pixel-identical across every single test (confirmed by direct photo comparison, e.g. `sel07` and
`tab` are indistinguishable). The only thing visible in these photos (a stray `MIDI CC 40` readout and
an odd vertical-bar widget) was leftover residual state from earlier incidental physical interaction with
the device during testing (see the Twentieth finding's confound note), not caused by anything sent here.

**Working conclusion**: either (a) 0x0B drives a physical element this specific P1 unit doesn't have —
plausible, since the source is shared across P1/P4/P6 and higher-tier siblings may have hardware (a
browser-encoder LED ring, a dedicated indicator) the P1 lacks — or (b) these messages only have an
effect when paired with internal driver state we can't reproduce via a bare replay (e.g. the actual
Bitwig browser panel being genuinely open in the DAW, `gBrowserOpen` mirroring real host state). Left
open; not resolved either way. Item 4 (0x0B) is now considered closed as "tested exhaustively, no
observable device-side effect."

### Twenty-third finding: reading `OutputState.prototype.send()` directly — the real flush logic, confirmed against our own hardware findings byte-for-byte

Per the request to work outward from known Bitwig SDK anchors rather than starting from CC/plugin
strings (the reverse of the old "panomod" approach): traced `sendSysex`/`sendChannelController`/`sendMidi`
back to their call sites and read the real `OutputState.prototype.send()` function
(`PANORAMA_P1.control.js:64-71`) end to end — the actual flush/diff method that turns the driver's
internal page state into the SysEx and CC traffic we've been reverse-engineering blind all session. Huge
payoff: it independently confirms nearly everything found by hardware testing, and resolves several open
questions outright.

**The `pageTemplate` and `DISPLAY_ID` enums, extracted verbatim from source** (`Z8114CB0CF3E757C0C` and
`DISPLAY_ID` respectively — object property *names* are hashed just like everything else, so the real
English names aren't recoverable this way, but the **numeric values are**, and they match our
independently-confirmed set exactly, digit for digit):
```
pageTemplate: {0, 1, 2, 3, 4, 5, 16, 17, 18, 19, 20, 21, 22}   -- matches our confirmed set exactly
DISPLAY_ID:   {0, 1, 2, 3, 4, 5, 6, 7, 8}                       -- matches our confirmed set exactly
```

**Confirmed exactly, byte-for-byte, from source** (things we'd only inferred from hardware before):
- `displayId 2`/`3` are literally `currentParameterInfo`/`currentParameterValue` in the driver's own
  internal variable names (`Z810A716763F1A2A63`/`Z810A88C163F19D2AD`) -- exactly matching the Twelfth/
  Fifteenth findings' guess at their real names, not just a plausible-sounding label we invented.
- `displayId 7` (`ctrlElementValue`) really is shared with `faderElementValue`, sent as the SAME
  `DISPLAY_ID.7` compose block, with `ctrlElementValue` using indices `1..8` and `faderElementValue`
  using indices `9..16` (`textEntry(1+b+8, ...)`) -- confirms the "shared with faderElementValue at
  indices 9-17" note that was previously only inferred from source comments, now confirmed from the
  actual index arithmetic.
- `displayId 0` (`padState`) is only ever sent `if(pageTemplate===21||pageTemplate===22)` -- exactly
  matching our hardware-confirmed template gating -- and it's sent via `padEntry`, which writes a single
  **raw byte value** (`uint7ToHex(1)` length, one value byte), never text. Confirms item 8 of the
  checklist directly: pad_state is genuinely a small numeric-enum field (backed by a 9-value enum,
  `Z8114B1CC53E76CFF0`, used for on/off/recording/queued clip states), not a text field at all -- no
  further "try binary payloads" testing needed, source settles it.
- **Item 7 (menu_button type/highlight flag) resolved from source**: the per-button highlight ("on")
  state is sent as a **plain Control Change**, `CC.<base>+buttonIndex` where `<base>=106` -- exactly the
  CC range `led_test.rs` already had as a candidate (confirmed independently from source, not just
  guessed from the driver's call-site clustering). It is NOT part of the SysEx text at all. Separately,
  the per-button **type** (`menuButtonType`, a 2-value enum) is sent via a distinct, never-before-tried
  SysEx shape: a `menuButtonTypesEntry` -- **index `0`** (not a normal 1-based text index), length `5`,
  followed by 5 raw type-enum bytes (0 or 1) -- sent as its own compose entry *before* the 5 normal
  1-based text-label entries in the same `displayId 4` message. This is a real, deliberate use of index
  0 (relevant to checklist item 5, "compose-entry malformation... index 0") -- it's not malformed at
  all, it's how the type array is distinguished from the per-button label entries.
- **`displayId 8` is never referenced in `OutputState.prototype.send()` -- but that claim was too
  strong; CORRECTED in the Twenty-sixth finding below.** It's genuinely absent from the normal
  compose/flush path (`send()` never touches it), but the driver DOES send it elsewhere, by hand,
  outside that function -- it's Bitwig's patch/preset browse popup menu. Leaving this paragraph in
  place rather than deleting it, since "not in `send()`" is still an accurate, useful fact (explains why
  it never shows up during ordinary mixer/fader use) -- just don't read it as "never sent at all."

**Follow-up hardware test, prompted by finding the CC-106 mapping in source**: re-ran `led_test on` with
the bottom tab row actually populated with real labels this time (previously always tested against a
blank/default tab row) and the now-wide camera framing. **Still no visible effect** from CC 106-110 --
neither the on-screen tab row nor the 5 physical round buttons directly below the display show any
change. This is now a well-supported negative result (confirmed against source-verified CC numbers, not
a guess), not an untested gap.

**Methodology note for future source-diving**: this file's obfuscation is *not* uniform -- the top-level
Bitwig Controller API glue (`transport`, `application`, `cursorTrack`, `host`, and dozens of `X.setter()`
state variables like `transportPosition`, `punchIn`, `overdub`) are already in **plain, readable names**.
Only Nektar's own custom display/page/menu logic layered on top of that API -- exactly the part most
relevant to this project -- uses the opaque `Z81[A-F0-9]+` hash-style naming (317 distinct identifiers
counted). This means the highest-value approach is exactly what was done here: find a well-known SDK
call (`sendSysex`, a `DISPLAY_ID`/`pageTemplate` numeric literal, a CC base value already confirmed on
hardware) and read outward from it, rather than trying to rename the whole file uniformly.

### Established convention: the "hacpad" resting baseline

After trying a fuller compose-mode design (title bar + big font + tabs + numbered knob content), direct
feedback settled on something much simpler: the device's resting/idle state between tests should be the
plain `pageTemplate=1` **message** overlay (see "Seventh finding") showing just the literal text
`"hacpad"` -- white text on the message mode's dark background, no colored chrome, no knob/fader content
competing for attention. This was explicitly preferred over the compose-mode mixer view for being
"much more evident."

**How to set it**: a single raw SysEx write, no compose fields needed --
```
F0 00 01 77 7F 01 06 01 00 00 06 68 61 63 70 61 64 04 F7
```
(`06 01 00 00 06 "hacpad" 04` -- the confirmed `writeMessageToDisplay` shape from the Seventh finding,
`06` cmd, `01` = pageTemplate MESSAGE, length 6, the ASCII text, `04` terminator, before `F7`). Via
`msg_test`, this is just `msg_test hacpad` (its default no-flag mode already builds this exact shape).
Going forward, reset the device to this state between isolated tests instead of the older
full compose-mode JSON baseline (title bar/big font/tabs/names/values) -- simpler, and it's what's
actually preferred as the resting look.

**Gotcha, and the fix**: `msg_test` always sends the lifecycle exit sequence (`EXIT_1`/`EXIT_2`) right
before the process quits -- and per the methodology note in the Seventeenth/Eighteenth findings, that
exit sequence **resets the display back to its default appearance**. So a plain `msg_test hacpad` only
shows "hacpad" for its ~8s hold window, then reverts -- it does not actually establish a resting
baseline. Added a `--persist` flag (`msg_test --persist hacpad`) that holds the MIDI connection open
indefinitely after sending (no exit sequence ever sent) instead of the normal 8s-hold-then-exit -- run it
in the background (`nohup ... &`) and it keeps "hacpad" on screen for as long as that process lives, only
releasing the connection when killed (needed before any other tool that wants exclusive access, e.g.
another `msg_test`/`service` invocation). Pair with `led_test off` to also clear any LED state left over
from a previous test (safe to run concurrently -- it's a separate, independent MIDI connection, no
init/exit handshake of its own to conflict with the persisted one).

### Twenty-fourth finding: items 5 and 7 — malformed compose entries are safely ignored; the type-flag hypothesis is a negative result

**Item 7**: replicated the `menuButtonType` mechanism found in source (Twenty-third finding) directly —
sent `displayId 4` with the special index-0 entry (`00 05 <5 raw bytes>`, alternating `0,1,0,1,0`) followed
by 5 normal text labels ("TypeA"/"TypeB" alternating). Transmitted cleanly (no error). **Result: no
visible rendering difference between type 0 and type 1** — all 5 buttons rendered identically (same blue
background, same style), confirmed by a zoomed crop of the tab row. Whatever `menuButtonType` actually
controls, it isn't a visible border/highlight/color difference on this hardware, at least not for these
two enum values. Negative result -- the "highlight flag" framing from the original checklist item wording
turned out not to describe an obviously-visible effect.

**Item 5** (compose-entry malformation), three sub-tests, all on `displayId 6` (names), each blank-first
and isolated:
- **Index 0** on a normal field: `06 10 06 00 04 "ZERO" F7` — no visible effect (no "ZERO" text anywhere).
  Confirms index 0 really is a special sentinel reserved for the menu-button-type entry, not a generic
  "0th slot" for ordinary fields.
- **Out-of-range index (99)**: `06 10 06 63 05 "IDX99" F7` — no visible effect, no crash.
- **Mismatched length byte** (claims length 10, only 3 bytes of text follow before `F7`):
  `06 10 06 01 0A "ABC" F7` — no visible effect, no crash, no garbled/corrupted display state, and no
  ALSA transmission error either.

All three transmitted and completed cleanly (`returncode=0`, no error line) — the device (or its SysEx
parser) tolerates all of these malformed/out-of-spec compose entries safely, silently discarding them
rather than crashing, corrupting subsequent display state, or doing anything unexpected. Good news for
robustness, but means none of these particular malformations reveal anything new about the protocol.
Items 5 and 7 are now considered closed (tested, safe, no effect found).

### Twenty-fifth finding: no template shows literally every field, but template 21 comes close (7 of 9) — and pad_state IS visible after all

Asked directly: "is there any template that displays ALL fields?" Tested empirically rather than
guessing -- sent all 9 known `displayId`s (0,1,2,3,4,5,6,7,8) in one session on template 21 (`drum_pads`,
the template that already combines the most fields: pad content + the standard chrome), each with
distinctive content, and photographed the combined result.

**7 of 9 fields showed up simultaneously**: title bar (`ALLTB1`/`ALLTB2`/`ALLTB3`), big font
(`BIGFONT`), current-parameter-value (`PARAMVAL`), the bottom tab row (`Tab1`-`Tab5`), the names field
(only in rows A/B, 8 slots -- confirms the existing per-template slot-count finding still holds even
in this combined test, rows C/D stay genuinely blank), the "mystery box" (displayId 8, rendering over
part of rows C/D on the right side), and -- a correction -- **pad_state (displayId 0)**.

**Correction to the "pad_state confirmed NOT a text field" framing**: that's still literally true (it's
not text), but it's not invisible either -- sending raw byte value `1` for indices 1-8 made those 8 pads
(rows A/B) render with a visible **red/pink tint** instead of the template's default blue. `pad_state`
is a real, visible **color/lit-state indicator** (matches the "on/off/recording/queued" clip-state enum
identified from source in the Twenty-third finding), just never text -- our original "no effect" framing
from the very first pad_state test undersold it.

**Not shown**: `page_labels` (5) and `ctrl_element_value` (7) produced no visible output anywhere on this
template. Most likely explanation: both need a template-specific rendering slot that simply doesn't
exist on the drum-pad layout (page_labels rendered near the first knob position on template 16 in an
earlier test; ctrl_element_value's real partner, `faderElementValue`, naturally wants a fader/knob
value readout, which template 21's pad grid has no equivalent of) -- the write itself doesn't error,
there's just nowhere for the template to draw it. Template 16 (knobs) is the natural next candidate to
test, since it's the one place `page_labels` was ever seen rendering and knobs have per-item value
readouts that `ctrl_element_value` could plausibly use.

**Answer**: no, no single template shows literally all 9 fields at once -- but template 21 gets very
close (7/9), and the 2 that don't show are plausibly a "no slot for it on this template" limitation
rather than the fields being broken or unreachable.

### Twenty-sixth finding: displayId 8 identified from source — it's the patch/preset browse popup menu

Prompted directly: "we got the popup menu also, but I suppose you understand how that works" -- read
`menuHandler.showMenu`/`onMenuEncoder`/`onMenuCancel`/`clearMenu` in source to check. Confirmed: our
"mystery box" (`displayId 8`) is exactly Bitwig's **patch/preset/category browse popup menu**, not an
unused or reserved field (correcting the Twenty-third finding's claim that it's never sent -- it's just
never sent through the normal `OutputState.send()` compose path; `menuHandler` builds and sends its own
SysEx string by hand, separately).

**`menuHandler.showMenu(items, menuCallback, offset)`** builds, verbatim:
```
"F0 00 01 77 7F 01 06 00 08 " + <index,len,text entries for up to 8 items, same shape as every other
compose write> + "F7"
```
Two things stand out:
- **`page_template` is hardcoded to `0`** (the reset sentinel) regardless of whatever template is
  actually active -- the popup is meant to overlay on top of the current page, not replace it via a
  normal template switch. This is an exception to reset's documented "field writes have zero effect"
  behavior (Thirteenth finding) -- apparently that only applies to the *normal* compose fields, not this
  one.
- Only `MAX_PAGEMENU_ITEMS=24` items exist per menu in total, paginated in chunks of **8** via `offset`
  (`this.offset=c?c:0`, `this.numItems=min(24,a.length-offset)`). `onMenuEncoder` increments/decrements
  `selectedMenuItem`, and crossing a page boundary (below 0 or above 8) calls `showMenu` again with
  `offset±8` to redraw the next/previous page of 8 -- **this is what the up/down triangles we
  photographed are for**: page-scroll indicators, not a per-item scrollbar.
- The **currently-highlighted item within a page** is communicated separately, as a **plain Control
  Change**: `delayIndicator()` (scheduled 20ms after every `showMenu` call) and `onMenuEncoder` both send
  `CC 111 = selectedMenuItem` -- not part of the SysEx text at all, matching the same "highlight state is
  a CC, not SysEx text" pattern already found for `menuButtonOn` (Twenty-third finding).
- **Dismissing it** (`clearMenu()`, called from `onMenuCancel`): a fixed, minimal SysEx --
  `F0 00 01 77 7F 01 06 00 08 00 00 F7` (an empty, zero-length first entry on the same `page_template=0`/
  `displayId=8` pair) -- followed by `resetOutputToUnknown()` to force every other field to redraw fresh
  next time.

None of this has been directly confirmed on hardware yet (we've only sent our own guessed content to
`displayId 8`, not replicated this exact populate/paginate/highlight/dismiss cycle) -- added as checklist
item 13, a standing hardware-verification task, per explicit request ("if we do not know how to populate
that menu or dismiss it, that warrants it's own addition to the loop").

### Twenty-seventh finding: the popup menu (displayId 8) confirmed on hardware — populate and highlight work, dismiss does not

Checklist item 13: replicated the exact byte shapes identified from `menuHandler` (Twenty-sixth finding)
against real hardware, one isolated session, blank-first, five steps each photographed:

1. **Blank** (reset, reference point).
2. **Populate**: `06 00 08` + 8 indexed text entries (`Item1`..`Item8`) -- **confirmed**: renders exactly
   as a real scrollable list inside the box on the right side of the screen, overlaid on top of the
   *already-showing* template 21 pad grid from an earlier test without disturbing it at all -- direct
   confirmation that `page_template=0` really does act as a non-destructive overlay target, not a real
   template switch (matches the source's design intent exactly).
3. **Highlight, `CC 111 = 3`**: `Item3`'s row changed to a solid red/pink highlight bar, all other rows
   stayed the default blue -- **confirmed**, exactly as `menuHandler.onMenuEncoder`/`delayIndicator`
   predicted (a plain Control Change, no SysEx involved, drives which row is highlighted).
4. **Highlight, `CC 111 = 5`**: the highlight bar moved to `Item5` -- **confirmed** the highlight
   genuinely tracks the CC value live, not a one-time paint.
5. **Dismiss**: sent the exact `clearMenu()` bytes, `F0 00 01 77 7F 01 06 00 08 00 00 F7` -- **no visible
   change at all**. A zoomed pixel comparison against the previous (highlight5) step shows the popup
   still fully populated with all 8 items and `Item5` still highlighted. The dismiss write did not
   visually clear anything.

**Follow-up, resolved**: checked source further -- every `onRemoveMenu()` implementation
(`DisplayPage.prototype.onRemoveMenu`, and every page object's override) turned out to be either a
no-op or just resetting internal JS boolean flags (`patchMenuOpen`, `categoryMenuOpen`, etc.); **none of
them trigger any redraw at all**. So dismissal isn't driven by `onRemoveMenu`. Tested the actual
alternative directly: populate the popup (on `page_template=0`, as always), then send a **genuine
template switch** to a real, different template (16, carried on an ordinary title-bar write) instead of
replaying `clearMenu()`'s bytes. **Result: the popup vanished completely** -- the screen showed a clean,
fresh, fully-default template 16 mixer view (generic numbered knobs, no custom text anywhere), with zero
trace of the popup. This matches the already-established Thirteenth finding exactly: switching to a
genuinely different real `page_template` clears per-template content, and the popup -- despite being
"overlaid" while nominally on `page_template=0` -- turns out to be exactly this kind of per-template
content, not chrome. The literal `clearMenu()` SysEx bytes alone (staying on `page_template=0`) do NOT
clear it; a real template switch does.

**Status, fully resolved**: populate, highlight, and dismiss are all now hardware-confirmed.
- Populate: `06 00 08 <indexed entries>` -- confirmed, overlays cleanly on the current page.
- Highlight: plain `CC 111 = <row>` -- confirmed, live-tracks the value.
- Dismiss: NOT the literal `clearMenu()` bytes alone -- switch to any real, different `page_template`
  (an ordinary compose write on that template, e.g. a title-bar write) and the popup clears as a normal
  consequence of the template-switch content-clearing rule (Thirteenth finding), not a special
  popup-specific mechanism.

### Twenty-eighth finding: input CC handling — a persistent live listener, and 4 more physical controls confirmed

Per "always have a live listener" -- started `panorama-bridge` (the `main.rs` binary, which already
decodes incoming CC via `decode_cc`/`cc_kind`/`cc_name`) in the background persistently, rather than as
a one-shot capture, so any physical interaction from here on gets logged without needing to remember to
start something first.

Traced the driver's own incoming-CC dispatch (`Z811481AF53E7994F1`, called from `onMidi` for every
Control Change) to find write-sites for flags we'd only seen read before, then had physical buttons
pressed on the real device while the listener ran, to confirm both directions at once:

- **CC 96 = Shift**: source, `case CC.Z8103EE82A3F8314DF: Z8114DFD213E74EEB9=0<e` -- this is the
  write-site for `Z8114DFD213E74EEB9`, the boolean flag that gates alternate Play/Stop/Record/Loop
  behavior everywhere in the driver (see the deobfuscation doc). Resolves that open thread: it's
  genuinely the physical Shift key, a momentary press/release CC (value 127 on press, 0 on release).
- **CC 103 = Mode**: source, `case CC.Z8106041EB3F613A4B: 0<e?setActiveDisplayPage(internalPage)...`
  -- plausibly tied to the "Internal mode" bypass from the Fifth finding, not yet cross-checked live.
- **CC 107/108 = screen buttons 1/2** (the two of the four small buttons above the display we hadn't
  identified yet), dispatched to whatever page object is currently active.
- **CC 109 = screen button 3, "exit/esc"**: confirmed BOTH from source and live -- pressed physically
  while the popup menu (Twenty-seventh finding) was still open, and the exact CC (109) matches
  `case CC.Z81048047C3F79F700: ...if(menuHandler.isActive) menuHandler.onMenuCancel(); else ...
  onScreenButton(3)` precisely.
- **CC 110 = "menu enter"**: confirmed the same way, pressed right after -- matches
  `case CC.Z8104967483F781F8A: if(menuHandler.isActive) menuHandler.onMenuEnter();` exactly.

Updated `main.rs`'s `cc_name`/`cc_kind` with all of these (previously falling through to a bare
`cc_N`/`Unknown`). `research/panorama-p1-driver-deobfuscation.md` updated with the confirmed identities.

Next: a live, interactive full-controller mapping exercise -- physical interaction observed directly via
the webcam feed, cross-referenced against the listener's CC log in real time, to build out the complete
physical-control-to-CC map (not just the handful found through source so far).

### Twenty-ninth finding: a full live-mapping session -- most of the physical control surface, a methodology lesson, and a major new discovery (USB HID keyboard interface)

A live, interactive session: a persistent background listener (`panorama-bridge`) decoding incoming
CC in real time, a `Monitor` watching its log for each new distinct CC and auto-capturing a webcam
photo the instant one appears (a small `tail -F | grep` filter, debounced to one event per 2s to avoid
flooding), while physical controls were touched on camera one at a time. This mapped nearly the entire
control surface in one pass: all 8 param encoders (64-71) and their 8 pan-encoder aliases (48-55 --
same physical knobs, reporting under a different CC range after a mode/Shift switch, not a second row
of controls as first suspected), all 8 faders (0-7; the "master" fader, CC 14, is confirmed
electronically broken hardware on this unit and produces spurious jitter, not a real signal), all 8
select/track buttons (16-23, cross-confirming the earlier LED-output test), the transport row, a
five-button nav cluster (91-95), Shift (96) and Mode (103), the four/five "menu button" screen buttons
(106-110), and the jog wheel (111, both as a relative rotary input and reused as the popup-menu
highlight-index output).

**Methodology lesson, the hard way**: a photo auto-captured "the instant a new CC is seen" during a
*fast, sequential* run of presses is not reliable proof of which physical control produced it -- there
is a small but real lag between a physical press and the log line/photo landing, and during rapid
one-after-another presses this reliably attributes each photo to the *previous* button in the sequence,
not the one that actually just fired. Caught this directly: photo-based reading of the transport row
gave 81=loop/82=rewind/83=forward/84=stop/85=play, but reading the actual driver source (the full `CC`
enum plus each case body's real Bitwig API call -- `transport.play()`, `.stop()`, `.record()`,
`.rewind()`, `.fastForward()`, `.toggleLoop()`) gives the true mapping, shifted by exactly one position:
**80=loop, 81=rewind, 82=forward, 83=stop, 84=play, 85=record**. The same lag affected the 86-90 and
91-95 rows, originally guessed as "Track-/Track+/Patch-/Patch+/View" and generic "nav" buttons from
photos -- source instead shows 86/87 are loop-in/loop-out point setters, 89 is the click/metronome
toggle, and 91-95 are zoom/arrow-key/preset-scroll actions. **Lesson for any future one-shot photo
attribution**: trust source over a fast-sequence photo when both are available; when only a photo is
available, either slow down enough that the debounce window is a non-issue, or independently verify
against source afterward as done here, don't trust a single frame.

**A caveat surfaced mid-session**: this specific unit has been repainted white by its owner, so the
original printed labels on most buttons are no longer visible/legible in photos -- position and
function (screen changes, LED state, source cross-reference) had to substitute for reading text.

**Major new discovery: pressing the "F-Keys" button (CC 99) opens a page the P1 renders entirely on its
own**, showing a scrollable grid of slots (`F1`-`F11`, `P5`/`P11` labels visible). This is NOT something
we (or any DAW driver) sent over SysEx -- our own tooling never writes to the display in response to CC
input, and the source's own handler for this CC (`Z8104C81913F751BFA`) only does driver-side bookkeeping
(`gBrowserOpen=false`, `setActiveDisplayPage(...)`, `nek_set_nektarine_instance_active(0)`) that has no
effect at all when the actual Bitwig extension isn't running (as in our setup). The P1 firmware itself
must be drawing this content locally and independently -- confirming, more concretely than the earlier
"Internal mode" finding, that some of this device's UI is genuinely standalone. Confirmed momentary (the
page closes when the button is released, not a toggle). Sending our own `pageTemplate=1` "hacpad"
message write successfully overrode/dismissed it when needed (the strongest override available -- a
literal template switch is not required, the message-mode write alone was enough here).

**Even bigger discovery, prompted by asking what "F-Keys" could really mean**: enumerated the device's
raw USB descriptors directly (`/sys/bus/usb/devices/1-1/`, no `lsusb` available in this sandbox) and
found the P1 presents **three USB interfaces**, not one: `1-1:1.0` (Audio Control), `1-1:1.1` (MIDI
Streaming -- the interface we've used all session), and **`1-1:1.2`, class 3 (HID)**. Reading its report
descriptor (`/sys/class/hidraw/hidraw1/device/report_descriptor`) gives:
```
05 01 09 06 a1 01 05 07 19 e0 29 e7 15 00 25 01 75 01 95 08 81 02 95 01 75 08 81 01 19 00 29 65 15 00 25 65 95 06 75 08 81 00 c0
```
which decodes to a **completely standard USB Boot Keyboard descriptor** (Usage Page Generic Desktop /
Usage Keyboard, an 8-bit modifier byte, a reserved byte, 6-byte rollover keycode array). **The Panorama
P1 has a genuine, independent USB keyboard HID interface, entirely separate from MIDI and from anything
the Bitwig driver does.** This is very plausibly the real mechanism behind "F-Keys" -- the on-screen page
is likely just the P1's own UI for choosing which key each pad/slot sends via this channel, with actual
keystrokes delivered over USB HID, bypassing MIDI/SysEx/the DAW driver entirely. **Not yet empirically
confirmed**: a raw HID listener (`/dev/hidraw1`, world-readable, no permission issues) was armed but no
report was captured during this session's testing -- either the F-Keys page needs to be actively
re-entered and a specific on-screen F1-F11 slot pressed (not just the mode-entry button itself) to
produce a keystroke, or there's some other trigger condition not yet found. Standing follow-up, not
resolved this session.

### Thirtieth finding: two LED CCs tested live via the webcam-viewer's new LED panel

Added an interactive LED panel to the webcam-viewer simulator (18 candidate on/off LED CCs from
`led_test.rs`'s list, each a clickable dot sending plain CC 127/0). Live-toggled individually against
real hardware:
- **CC 29** ("arranger automation write", per source's `sendChannelController` call site): toggling on
  produces **no visible effect** on this hardware -- either this unit has no backlit automation-write
  button, or 29 isn't the right CC for it. Negative result, not yet explained.
  **Follow-up, resolved which half of that**: re-read the exact source call site directly --
  `transport.addIsWritingArrangerAutomationObserver(function(a){sendChannelController(0,CC.Z8103C1CCB3F85A29E,a?127:0)})`,
  and `Z8103C1CCB3F85A29E` is unambiguously `CC{...:29}` in the enum. **The CC number is confirmed
  correct** -- it's driven by real Bitwig transport state, not a guess. So the negative result means this
  specific physical unit most likely has no dedicated LED for this function at all (or it's combined
  into another button's indicator, e.g. a Shift-gated function sharing Record's LED, per the
  Shift-flag-gates-alternate-function pattern already documented elsewhere in this file) -- not that we
  had the wrong CC.
- **CC 99**: NOT the F-Keys button's own backlight as guessed in `led_test.rs`'s comment ("likely a
  static connected/online indicator") -- confirmed instead to light the **first of a row of 4 small
  status LEDs positioned above the screen**. Matches the "sent unconditionally at init" behavior (a
  connection-status indicator), just not on the button it was assumed to be near. The other 3 LEDs in
  that same strip have no known CC yet -- open follow-up.

### Thirty-first finding: the pageTemplate=1 "message" write does NOT clear an already-open popup menu

Cross-checked the service's exhaustive default-state write (applied at every startup) against the real
screen via the webcam. Assumed output: a clean "hacpad" message-mode takeover, since `message` is sent
last in `apply()`'s field order and message mode was believed to be a full-screen, exclusive rendering
pathway. **Actual screen**: "hacpad" text was visible, but with the popup menu's highlighted-row bars
(from the same default state's `menu_items`/`menu_highlight` demo content, sent earlier in the same
`apply()` call) still overlaid on top of it -- a genuinely new interaction, not previously tested.

The popup (displayId 8, hardcoded `page_template=0`) is confirmed (Twenty-seventh finding) to be
dismissed only by a real switch to a *different, real* page_template via the normal compose path (e.g.
a title-bar write on template 16) -- **not** by the literal `clearMenu()` bytes alone. This finding adds:
it's **also not cleared by a subsequent `pageTemplate=1` message write** -- message mode coexists with an
already-open popup instead of taking over the whole screen as assumed. Practical fix applied in
`service.rs`: the default boot state no longer actually writes the popup fields to the real device (kept
only in the shared/client-facing JSON as demo content to explicitly try), so the resting "hacpad" screen
stays clean. Not yet isolated further (e.g. whether message mode ever clears the popup under any
ordering) -- open follow-up if it matters later.

### Thirty-second finding: verifying "do fields need to match the template" via the simulator + webcam, three-way (state / simulator / device)

Directly asked: does the template drawing a set of fields mean those fields must actually be supplied?
Verified with a scripted WebSocket client acting as a raw simulator client (bypassing index.html's
"helpful" auto-resupply-on-switch behavior added earlier), cross-checked against the real screen via
the webcam at every step. Sequence, each isolated:

1. **`{"layout":"pads16"}` alone, no other field** -- **zero visible effect**. The screen kept showing
   the *previous* template's content completely unchanged. This is the key mechanism fact: **a
   `page_template` switch is not a standalone operation** -- there is no message that "just" changes the
   template. The template id is only ever a byte embedded inside some other field's compose write
   (`write_title_bar(template, ...)`, `write_names(template, ...)`, etc. all take the template as a
   parameter baked into the SysEx body). Sending a layout choice with no accompanying field write
   transmits nothing at all. This is exactly why index.html's `layoutChanged` handler always piggybacks
   a `titleBar` write on a template switch -- confirmed that design is necessary, not just a convenience.
2. **`{"layout":"pads16","titleBar":[...]}`** (same title bar content as before) -- the template
   genuinely switched (screen now shows the empty 4x4 pad grid, rows D/C/B/A), confirming the title-bar
   write is what actually carries the template id onto the wire. The pads rendered **completely
   blank/unlabeled** even though `names` was never touched by this message -- confirms per-template
   content (displayId 6) really is cleared by the switch itself, not by an explicit blanking write
   ("Fourteenth finding", now re-confirmed end-to-end through the simulator rather than just from source).
   Meanwhile `bigfont`/`currentValue` ("CHROMEBIG"/"CHROMEVAL", set once at the very start and never
   touched again for the rest of this whole test) were still showing, unchanged -- chrome persistence
   confirmed without ever being resent, not just "survives a resend of the same value."
3. **Supplied `names` matching pads16's 16 slots** (`P1`..`P16`) -- all 16 pads populated correctly, and
   the row/index mapping matched our mockup's assumption exactly: row D = indices 12-15 (P13-P16), row
   C = 8-11 (P9-P12), row B = 4-7 (P5-P8), row A = 0-3 (P1-P4). Direct confirmation the simulator's
   `(nrows-1-r)*4+c` mapping is correct, not just plausible.
4. **Supplied `values` (16 slots) on pads16** -- no new visible text anywhere (matches the documented
   assumption that `ctrl_element_value` has no rendering slot on the pad-grid widget).

**Answer to the original question**: yes -- but more precisely than "matching content, for cosmetics":
the template switch itself is *carried by* whichever field write you send alongside it, and per-template
content fields (names, and by extension values/pad_state/page_labels) get silently cleared by any real
template change regardless of what that switch message touches, so they must be *resupplied* after a
switch if you want them to keep showing -- not because the field values need to "match" the new template
semantically, but because the device wipes them as a side effect of any genuine template switch. Chrome
fields (title_bar/big_font/current_value/menu_button) need no such resupply.

**Two incidental corrections found via the same cross-reference**, both now fixed in index.html:
- **Title bar coloring**: confirmed (well-lit, close-framed photo) as **`[red, blue, red]`** for the
  3 segments -- our mockup had assumed only the rightmost segment was ever highlighted (red) and the
  other two plain/dark. Fixed the mockup's CSS/JS to the confirmed pattern.
- **Pad-grid row shading**: the real widget renders a **vertical brightness gradient**, darkest at the
  top row (D) fading to lightest at the bottom row (A) -- unrelated to any of our field content (initially
  mistaken, mid-test, for a `values`-write side effect; ruled out by resending `names` alone and seeing
  the same gradient persist). Purely a rendering fact of the pad widget itself. Added an approximating
  gradient to the mockup's pad cells (previously a flat color for every row).

Methodology note: this whole exercise deliberately used three distinct reference points -- the service's
shared JSON *state*, the *simulator*'s deterministic rendering of that state (reasoned about directly,
since no headless browser was available to actually screenshot the page), and the *real device* via the
webcam -- catching two mockup inaccuracies that would otherwise have silently drifted from hardware
truth.

### Thirty-third finding: a fuller re-grep of every literal `sendChannelController` call site resolves the remaining input CCs, and finds two new LEDs (Mute/Solo)

Prompted directly: "the status leds above the screen -- only 'Status1' is controllable... failure to
measure?" Re-did the CC-identification work with a wider net than the earlier pass: extracted the FULL
`CC={...}` enum (75 entries) from `PANORAMA_P1.control.js`, then found every literal
`Z81134B25E3E8D3DA8(0,CC.<ident>` (the `sendChannelController` wrapper) call site rather than grepping
for specific known identifiers -- this is what the earlier pass missed (its case-body grep didn't span
the full minified line for several CCs).

**Two new, previously-untried LED CCs found, both a clean input+output pair (same mechanism as every
other LED in this file)**:
- **CC 30 = Mute**: `case CC.30: cursorTrack.getMute().toggle()` (input) and
  `cursorTrack.getMute().addValueObserver(function(a){sendChannelController(0,CC.30,a?127:0)})` (output
  LED) -- same CC drives both, standard pattern.
- **CC 31 = Solo**: identical pattern, `cursorTrack.getSolo()`.
- Live-tested via the simulator's LED panel (isolated, only these two lit): **no visible change in the
  currently-framed camera view** (screen + top-4-button strip + Track-/Track+/Patch-/Patch+/View row).
  The fader/select-button area, where a Mute/Solo indicator would more plausibly live, is physically out
  of the webcam's current framing (camera position, not zoom -- `zoom_absolute`/`pan_absolute`/
  `tilt_absolute` are all still 0) and hasn't been re-checked with the camera repositioned. **Not yet
  visually confirmed either way** -- the CC identities themselves are certain (straight from source),
  only their physical LED location/visibility is still open.
- **CC 25**: also a literal `sendChannelController` call site (in a conditional tied to `menuButtonLabel`
  text: `"Browser"`/`"<"`/`">"`/`"OK"` vs `"Sends"`/`"User"`/`"Devices"`), not yet characterized or tested
  live -- a genuine new candidate, not confidently identified.
- **CC 15 + CC 47** are NOT simple on/off LEDs -- `cursorTrack.getVolume().addValueObserver(1024,
  function(a){var b=a&7;a>>=3; sendChannelController(0,CC.15,b); sendChannelController(0,CC.47,a)})`:
  a 3-bit/remaining-bits split of the (0-1023-scaled) volume value across two CCs, almost certainly
  driving a segmented level-meter LED bargraph for the focused track, not a single indicator. Different
  mechanism from every other LED_CCS entry -- not added to that list, needs its own dedicated handling
  if pursued.
- **CC 14** confirms our own already-known `fader_master` input mapping from the output side too (tied
  to `masterFaderValue`/`trackFaderValue` feedback) -- no new information, just cross-confirms.

**All remaining previously-unresolved input CCs, resolved from source in the same pass** (main.rs
updated accordingly):
- **CC 88** = Undo/Redo (Shift-gated: shift=`application.redo()`, plain=`application.undo()`) --
  corrects the earlier positional guess that had this at CC 102.
- **CC 90** = Overdub (plain) / automation-write-toggle (Shift) -- `transport.toggleOverdub()` vs
  `transport.toggleWriteArrangerAutomation()`.
- **CC 93 & CC 94** literally share one case body (`case CC.93:case CC.94:PATCH_PRESSED=0<e`) -- a
  fallthrough setting one shared gate flag, not two independently-dispatched buttons at this switch.
  Labeled `patch_minus`/`patch_plus` from the physical button row (confirmed via a webcam photo showing
  the printed labels: Shift/Track-/Track+/Patch-/Patch+/View) -- not fully reconciled with an EARLIER
  finding that attributed CC 94 to `application.zoomIn()`/`arrowKeyDown()`/preset-scroll (that logic
  likely lives elsewhere, reading the shared flag alongside encoder direction) -- treat the exact
  minus-vs-plus split as tentative.
- **CC 95** = View (`onView()`, or in some states sends the 0x0B "Launcher" SysEx family documented
  elsewhere in this file).
- **CC 97** = **CORRECTED** -- source is `TOGGLE_MUTE_PRESSED=0<e`, a mode-gate flag, NOT the jog wheel's
  push/click as previously guessed (that guess is now removed from main.rs). Distinct from CC 30 (which
  directly toggles+lights cursorTrack mute) -- CC 97 is plausibly a pad/drum-mode mute-select button,
  not yet confirmed live.
- **CC 98** = `TOGGLE_VIEW_PRESSED`/`onToggleView()`.
- **CC 100/101/102** = three distinct browser/patch-menu "cancel"-shaped handlers (`gBrowserOpen=false`
  plus varying `setActiveDisplayPage`/`SurfaceStatus` side effects) -- not individually distinguished as
  specific physical buttons yet, kept as generic source-quoted labels rather than over-claiming names.
- **CC 104** = `SurfaceStatus`/`SURFACE.connected`-state related -- plausibly not a normal user button.
- **CC 105** = `transport.toggleWriteArrangerAutomation()`, **unconditional** (unlike CC 90's Shift-gated
  version) -- likely the actual physical button whose LED is CC 29 (still no visible LED effect
  confirmed on this hardware, per the Thirtieth finding).

**Answer to the original question**: the earlier "only Status1 works" result was a genuine gap in how
thoroughly the source was searched, not a hardware limitation being mistaken for one -- a wider, more
systematic re-grep (every literal call site, not just a curated candidate list) turned up 2 solid new
LED CCs (Mute/Solo) plus a 2-CC segmented-value pair (15/47) immediately. The other 3 LEDs in the actual
4-LED status strip above the screen specifically are still unfound, though -- none of the newly-found
CCs (25, 30, 31) were confirmed to land there. Worth another pass looking specifically for call sites
using a computed/indexed CC argument (e.g. `CC[base+i]`) rather than a literal `CC.<ident>`, which this
grep pattern would not have caught.

### Thirty-fourth finding: chasing the status-strip mutex directly — a real session-invalidation bug found, SurfaceStatus's value ruled out, position 1/4 mutex confirmed

Prompted directly ("StatusN seems mutually exclusive??" / "find a way to control the LED status of the
statusN, either individually or as a mutex"). Found the `SURFACE={...}` enum in source: a genuine
4-value enum (0-3), consumed by a function that sends one of 4 different `0x09`-family lifecycle SysEx
payloads depending on the value, guarded by `if(SurfaceStatus!==a)` -- exactly matching "one state
variable, mutually exclusive" in shape. Two of its 4 payloads are byte-for-byte our own already-known
`EXIT_1` (state 0) and `INIT_2` (state 1) constants.

**Tested directly on hardware** (raw sends, bypassing the driver's own mk2/nektarine flag guard so all
4 payloads could be tried regardless): all 4 `SurfaceStatus` payloads (0/1/2/3) produced the **same**
visual result -- lighting the **rightmost** of the 4 status LEDs (call it Status4) every time, not four
different positions. **The "one CC/value selects which position" hypothesis is ruled out** for this
mechanism specifically.

**However, a real, reproducible mutex WAS found**: sending any `0x09`-family lifecycle SysEx lights
Status4 **and turns off Status1** (the CC-99-driven LED from the Thirtieth finding) -- confirmed by
directly comparing before/after photos. So positions 1 and 4 genuinely are mutually exclusive, just not
via SurfaceStatus's *value* -- rather, CC 99 (a plain CC) and the `0x09` SysEx family are two different
mechanisms that happen to share one physical indicator "slot" between them, and whichever fires last
wins. Positions 2 and 3 remain completely unexplained -- none of CC 25/30/31/13, nor any of the 4
SurfaceStatus payloads, produced them.

**Bigger discovery made along the way -- a genuine session-invalidation behavior, not a bug in our
tooling**: sending an EXIT-shaped SysEx (either literally, via `SurfaceStatus`'s state-0 payload, or as
the normal tail end of any `msg_test` invocation without `--persist`) puts the device into a state where
it **silently ignores subsequent writes, including message-mode**, until a fresh INIT sequence
(`INIT_LINUX_ONLY` + `INIT_1` + `INIT_2`) is sent again. Confirmed directly: a raw "hacpad" write sent
with no prior init did nothing; the identical bytes preceded by a fresh init sequence worked
immediately and persistently. This explains an apparent "webserver control seems dead" symptom
encountered mid-session -- the long-running `service` process's one-time startup init had been
invalidated by an EXIT-shaped payload sent from a separate, concurrent test script, and `service` has no
logic to detect that and re-init (a real gap, see Open threads below). Recovered by restarting `service`
(which re-runs its init sequence at startup) -- also cleared the stuck Status4 LED as a side effect,
confirming that LED's state resets along with a fresh init too.

**Also newly observed**: the device replies to our lifecycle SysEx with its own unsolicited SysEx --
same shape, but manufacturer sub-id `7F 02` instead of our `7F 01` (device-to-host vs host-to-device),
and a different trailing checksum byte, e.g. we send
`F0 00 01 77 7F 01 09 03 00 00 01 3E 34 F7` and the device replies
`F0 00 01 77 7F 02 09 03 00 00 01 3E 33 F7`. A real ACK/reply protocol, partially answering checklist
item 9 ("unsolicited device-originated SysEx") -- not further characterized (do we need to reply to it?
does ignoring it contribute to the session-invalidation behavior above?).

**Open threads**: positions 2/3 of the status strip still unfound (candidates tried and ruled out: CC
13/25/30/31, all 4 SurfaceStatus payloads); whether `service.rs` should detect a silently-ignored write
and auto-recover by re-sending init is an open design question, not yet implemented.

### Thirty-fifth finding: the 4-LED status strip fully solved — CC 99-102, one register not four bits

Directly prompted ("go back to the cc recordings when I pressed all buttons" / "find a way to control
the LED status of the statusN, either individually or as a mutex"). The recorded button-press log
(`live_listener.log`, ~500 lines from an earlier live-mapping session) only confirmed CCs 80-103 and
106-111 were ever pressed -- nothing new there (and notably CC 104/105 never fired despite an apparently
thorough press-everything session, reinforcing they're not reachable via a normal button). The real
answer came from replaying every already-known input CC directly against the device while photographing
*only* the status strip, isolated one CC at a time from a forced-clean baseline (critical: an earlier
non-isolated batch run gave a false lead from stale state left over by the service's own default LED
demo list, which includes CC 80 -- always force `ledsOn: []` before a clean test).

**Fully confirmed, one CC at a time, clean before/after photos**: CC 99 = Status1 (leftmost), **CC 100 =
Status2**, **CC 101 = Status3**, **CC 102 = Status4** (rightmost) -- a contiguous 4-CC block, exactly
the CCs already tentatively identified from source as "browser_cancel_1/2/3" (100/101/102) and F-Keys
(99) input handlers. Same "one CC serves as both the input handler AND the LED" pattern as every other
control on this device.

**The mutex was directly demonstrated, not assumed from testing each CC in isolation** (a real risk
flagged mid-investigation): set CC99=127 (position 1 lights), then set CC100=127 **without ever sending
CC99=0** -- position 1 goes dark, position 2 lights. Repeated for a non-adjacent pair (CC102 then CC99)
with the same result. **Further confirmed, answering "does 0 to an unlit position affect the lit one?"**:
with Status2 (CC100) lit, sending CC99=0 (a *different*, already-off CC) turned Status2 off too -- so
this is not "4 independent bits, only clearable by their own CC" but **one hardware register**: writing
127 to CC (98+N) sets the display to position N; writing 0 to **any** of the 4 CCs clears whichever
position is showing, regardless of which CC lit it. 5 total states (off, or exactly one of 1-4 lit), not
16 (4 independent bits) or even 5 independent on/off pairs.

**Practical consequence for our own code**: `led_cc_messages`'s generic "resend every LED_CCS entry,
127 or 0" loop -- correct for every other LED in this file -- would actively misbehave if applied to
this group: whichever of the 4 CCs is iterated last with a 0 would clear an already-applied 127 from
earlier in the same batch, regardless of intent. Given `STATUS_LED_CCS`'s array order (99,100,101,102),
naively including them in that loop would mean only "select Status4" ever sticks (nothing after it to
clear it) -- everything else would silently clear itself. Fixed by giving this group its own dedicated
function (`status_led_messages` in lib.rs, `statusLed` field in service.rs/index.html) that always
clears first, then sets second, so the result is correct regardless of prior state -- and by presenting
it in the simulator as a proper 5-state single-select control, not 4 checkboxes.

**Loose end**: the 0x09-family lifecycle SysEx (`SurfaceStatus`, Thirty-fourth finding) was separately
seen lighting this same position 4/Status4/CC102 slot. Two different mechanisms (a plain CC and a
lifecycle SysEx) apparently feed the same one-of-four hardware indicator -- not reconciled, but no
longer relevant to actually *controlling* the strip now that CC 99-102 does that directly and reliably.

### Thirty-sixth finding: the message overlay and popup menu need an explicit dismiss action, not "send empty"

Follow-up UI gap, prompted directly ("we probably also need a means to control the overlays in an
on/off sense"): the simulator's `message` and popup-menu (`menuItems`) fields had no way to turn the
overlay back *off* once shown -- clearing the text to empty does nothing, since `service.rs`'s `apply()`
guards both writes with `!field.is_empty()` (empty means "don't touch," not "clear"), and per the
Twenty-seventh/Thirty-first findings, neither overlay is clearable by its own empty/cancel bytes anyway
-- only a genuine `page_template` switch dismisses either one. Added an explicit enable/disable checkbox
next to each field in index.html; unchecking either one calls a shared `dismissOverlay()` that forces a
real template switch (resending the title bar on the current layout, the same safe "carries a template
switch" mechanism `layoutChanged` already used) and unchecks both boxes together, since dismissing one
this way genuinely dismisses both -- there's no way to clear just one while leaving the other showing.

### Thirty-seventh finding: DeviceState needed boot-seeding, and the popup highlight has no clear value

Two bugs reported directly this session against the new semantic command layer
(`{"semantic": {...}}` in service.rs, built on `DeviceState` from lib.rs -- see the
widget-model doc): "hiding popup does not really work" and "there is no unset highlight".
Root-caused and confirmed on real hardware, not assumed:

1. **`hide_popup`/`hide_message` were a silent no-op the first time they were ever used.**
   `DeviceState::default()` starts with `last_background: None`; `hide_popup`/`hide_message`
   restore `last_background` and do nothing if it's `None` (by design -- there's genuinely
   nothing to restore to). But `service.rs` never told its `DeviceState` what was actually
   already on the real device from the boot-time raw write (`default_state()`) -- so the
   VERY FIRST semantic `showPopup`/`hidePopup` pair on a freshly-started service always
   failed silently: `popup_visible()` correctly flipped to `false` (misleadingly reporting
   success), but zero SysEx bytes were sent, so the real screen never changed. Reproduced as
   a unit test (`hide_popup_is_a_silent_noop_without_a_prior_switch_background`, lib.rs) and
   confirmed the fix live over the WebSocket from a fresh service process (no prior semantic
   `switchBackground` call at all): `showPopup` → `hidePopup` now correctly restores the
   seeded Mixer background and reports it. Fix: `DeviceState` gained `seed_background`/
   `seed_message_shown` (record state without sending bytes -- the caller already wrote
   those bytes some other way), and `service.rs`'s boot sequence now calls them right after
   the raw `default_state()` write, mirroring exactly what was just put on screen (Mixer,
   `N1`-`N8`/`V1`-`V8`, `TB1`-`TB3`, plus the "hacpad" message that's genuinely showing on
   top of it at boot).

2. **The popup highlight (CC 111 / `selectedMenuItem`) has no value that clears it.**
   Confirmed directly via the webcam, twice, with generous (1-1.5s) settle time between
   each step to rule out display-lag artifacts (a real, separate effect also observed this
   session -- rapid-fire semantic commands with no delay can show 1-2 renders behind for a
   few seconds, confirmed to just be a display/MJPEG-capture timing issue, not a dropped
   write, by re-photographing later with no new command sent and seeing it catch up):
   showed a popup, set the highlight to a real row (2 or 3), then sent `0`, `127`, and `255`
   to the highlight CC in turn. In every case the ORIGINAL row stayed highlighted --
   none of these values moved it, cleared it, or had any visible effect at all. So CC 111
   apparently only accepts values within the popup's actual item range and silently ignores
   (rather than clamps-to-boundary or clears-on) anything outside it. **There is no known
   way to make an open popup show with zero rows highlighted once a highlight has been set
   -- the only confirmed way to make the highlight bar disappear is to dismiss the entire
   popup** (`hide_popup`, which does a real Background switch). Documented as a genuine
   hardware limitation, not a missing button: `index.html`'s semantic panel now says this
   explicitly next to Set Highlight, instead of implying an unset value exists.

   Also incidentally confirmed while investigating this (after ruling out a false alarm from
   the same display-lag effect): shrinking a popup from 8 items to 2 DOES correctly clear
   the trailing entries once given enough time to render -- no stale-item bleed-through, so
   `write_page_menu`'s per-call entry count is trustworthy; the earlier appearance of a
   stale "Item5" mid-investigation was the same lag artifact, re-confirmed by waiting and
   re-capturing rather than assumed away.

### Thirty-eighth finding: resending the SAME page_template never registers as a real switch -- only bouncing through a different one clears an overlay

Direct follow-up after the Thirty-seventh finding's fix (all four Body fields on every
switch) still didn't fix "hiding popup from the browser" -- reported directly, tested
immediately rather than assumed fixed. Photographed the exact failure: `switchBackground
(Mixer)` → `showPopup` → `hidePopup` (which restores `last_background`, i.e. Mixer again,
unchanged) produced a garbled hybrid frame -- the new Body content (fresh param names/values)
was correct, but the popup's own overlay graphic (box border, item list, `Esc`/`Enter`
buttons) stayed ghosted on top of it, half-overlapping the new content.

**Root cause, isolated by direct A/B test**: restoring to the exact same `Background` that
was already active means `page_template` never actually changes VALUE between the two writes
-- and this device apparently requires the template value to genuinely change to run its own
overlay-clearing logic, not merely receive a fresh content write under the same template
number. Confirmed by forcing a real value change: `switchBackground(Mixer)` → `showPopup` →
`switchBackground(Blank)` (a different template) → `switchBackground(Mixer)` again --
photographed clean at every step, popup fully gone. Directly contradicts nothing already on
record; it's a refinement of the Twenty-seventh/Thirty-second findings' "a genuine switch
clears it" language -- turns out "genuine" specifically means the numeric template value has
to move, not just that a real field write occurs.

**Fix**: `DeviceState::resend_body()` (used by `show_popup`/`hide_popup`/`show_message`/
`hide_message`, all of which restore `last_background` unchanged) now always bounces through
a different template first -- `Background::Blank` (template 5) normally, or `Reset` (template
0) specifically when the real target's own template IS 5 (Blank or Grid), so the bounce is
never accidentally a no-op itself. Confirmed fixed live, from a totally fresh service process
(no prior semantic `switchBackground` at all): the exact same photographed sequence that
previously produced the garbled frame now renders a clean Mixer view with all 8 knob labels
correct and no popup text bleed-through.

**Follow-up, same session -- root-caused and fixed**: the bounce alone left a residual pair of
blank (unlabeled) button-shaped outlines in the popup's old screen position (confirmed by
comparing against a Background that never had a popup opened on it -- no such artifact there).
Asked directly: "does that mean we also have to resend the footer?" Tested by sending CC 109
(`screen_button_3_exit`, the INPUT CC an Esc-button press reports) as host->device output
while a popup was open: it didn't close the popup, but it DID change one popup button's own
label from `Esc` to `Tab4` -- direct proof the popup's `Esc`/`Enter` buttons are rendered
through the SAME Footer slot (displayId 4, `menu_button`/tabs) as the normal tab row, and
`switch_background_messages` had NEVER written to it at all. So whatever last touched that
slot (the popup's own buttons, or ancient boot-time `Tab1`-`Tab5` demo content) lingered
indefinitely -- nothing to do with Body at all. Fix: `DeviceState` now tracks `last_tabs`
(defaulting to 5 real-but-blank `" "` placeholders -- non-empty so `indexed_entries` doesn't
filter them out and skip the write) and resends it via `write_tabs` alongside every
`switch_background`/`resend_body` call (bounce AND real target), plus a new `set_tabs`
setter/`SetTabs` semantic command for a caller to actually control it. **Confirmed fully
clean** on the next photograph: the same previously-ghosted sequence now shows all 8 Mixer
knob labels correct AND a clean, evenly-blank 5-slot footer -- no residual button outlines at
all.

### Open question: initial CC-mapping bootstrap (unconfirmed, not investigated)

The device's own GLOBAL (non-DAW) view has explicit controls for assigning physical
controls to CC numbers — raising the question of whether there's some bootstrap/handshake
where the P1 tells a connected host (or is told) what each physical control is currently
mapped to, rather than the CC map being a fixed, host-assumed constant as treated
everywhere in this file so far. Not investigated this session (deliberately deferred) —
noted here as a standing unknown, not a confirmed finding either way.

## Debugging technique: USB webcam on the screen

A USB webcam pointed at the P1's own screen is a cheap, effective way to visually confirm whether a
sent SysEx message actually changed the display, without needing the official software or a second
reference implementation running. Practical notes from doing this:
- The P1 appeared directly as an ALSA rawmidi device (`/dev/snd/midiC1D0`) with group-writable
  permissions — no extra MIDI tooling (`amidi`, `mido`, etc.) was needed; raw SysEx bytes can be
  written straight to that device node.
- A capture can be grabbed headlessly with `ffmpeg -f v4l2 -input_format yuyv422 -video_size 1280x720
  -i /dev/video0 -frames:v 1 -update 1 out.jpg` (find the right `/dev/videoN` via `v4l2-ctl
  --list-formats-ext`).
- First attempt came back too out-of-focus to read the display text — camera framing/focus (autofocus
  was on, but the shot was still blurry) needs to be sorted before this is reliable for verifying exact
  text content, though gross layout/color changes are visible even out of focus.
