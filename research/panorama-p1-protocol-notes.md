# Nektar Panorama P1 — protocol notes

> **Current status (read this first): DISPLAY WRITE CONFIRMED WORKING.** Root cause found and fixed:
> our `Internal`/`Instrument` port assignment was backwards all session (see "Fifth finding"). The
> official Bitwig integration guide's Linux port-config table (`Output1: Instrument, Output2:
> Internal`) revealed the correct mapping; once `msg_test.rs` was corrected to send the init sequence
> and display write on the actually-correct ports, the device (a) replied to our init handshake for
> the first time all session, and (b) rendered our literal text (`"HACPAD FIX"`) on its physical
> screen, confirmed by webcam photo. Input (CC decoding) was already working. **Both directions of the
> USB bridge are now confirmed live against real hardware.** The earlier "Internal mode" finding
> (device's own native standalone UI, tab labels `Faders`/`Encoders`/`Cntrl Edit`/`Global`/`Setup`
> matching the old manual's documented Internal Mode exactly) was real and correctly identified, but
> turned out not to be the actual blocker — the display write overwrote/replaced that screen directly
> regardless of device mode once sent on the correct port. `main.rs` (the real `panorama-bridge`
> binary) has the corrected port mapping and is smoke-tested end-to-end (init → screen write → CC
> input listen). Multi-line text (`\n`-separated) and a full bordered ASCII logo banner are also
> confirmed rendering correctly — see "Sixth finding". See `prototypes/panorama-p1/README.md` for
> how to run it.
>
> **Correction (important):** the "Seventh finding" below, claiming the message write is a
> "column-major rotated character grid" needing transposed ASCII art, **was wrong** — it was an
> artifact of the debugging webcam being mounted 90° off from the device without that being accounted
> for. Corrected: it's an ordinary top-to-bottom, left-to-right multi-line text box, nothing rotated or
> transposed. See the correction under "Seventh finding" for how this was caught, and "Ninth finding"
> for how real plugin-parameter text (names/values from Bitwig's API) actually flows into the screen
> via a *separate*, structurally different general per-field compose write — the still-open item for a
> future session, not yet tried with real content.

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
