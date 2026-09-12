# Nektar Panorama P1 — protocol notes

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

## Next steps

1. **Physically switch the device into its Bitwig/DAW-control mode via its own on-screen `Setup` menu**,
   then re-run `msg_test` and re-check the webcam. This is the leading remaining hypothesis for why
   byte-perfect SysEx traffic (correct ports, correct bytes, bidirectional connections, all confirmed)
   is having zero effect, and it's the one thing left that requires a human at the hardware rather than
   more code — every code-only hypothesis triable without either a live Bitwig session or this physical
   step has now been tried and ruled out (see above).
2. If step 1 changes nothing: real USB-level packet capture (usbmon/Wireshark) remains untried this
   whole investigation — would show the literal bytes on the wire during a real Bitwig session,
   sidestepping the need to fully trace the JS state machine by hand. This would also settle whether
   Bitwig sends something else entirely before the display ever lights up that we haven't found by
   reading the source (e.g. a non-SysEx trigger, or traffic from `pnx1.js`'s `DisplayPage` instances
   we haven't traced through in full — `Z810A561E13F1C28DD`/`textEntry`/`Z810AA638B3F174CED` compose
   path specifically, which we've read but never actually replayed against hardware since it needs
   real per-field content we don't have without a live session).

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
