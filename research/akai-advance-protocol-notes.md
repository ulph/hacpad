# Akai Advance 25 — protocol notes

Findings log for the Advance 25, kept separate from the Panorama P1 notes because
the device is a separate line of investigation (separate crate, separate service,
separate client).

Same discipline as the P1 log: nothing is written here that has not been observed
directly. Retractions stay visible rather than being edited away.

---

## First — USB identity and endpoint map

`09e8:002f`, `Akai / ADVANCE25`, at `usb-0000:00:14.0-1.3`. Full speed (12 Mbit/s),
USB 1.10, `bcdDevice 0200`, 1 configuration, **2 interfaces**:

| Interface | Class / Sub / Proto | Endpoints | Driver |
|---|---|---|---|
| 0 | `01 / 01 / 00` — Audio Control | none | snd-usb-audio |
| 1 | `01 / 03 / 00` — MIDI Streaming | Bulk OUT `0x03`, Bulk IN `0x83`, `wMaxPacketSize` 64 | snd-usb-audio |

**There is no non-MIDI endpoint.** No vendor-specific interface, no HID, no second
bulk pipe. The 4.3" colour screen is therefore fed through the same USB-MIDI
streaming interface as everything else — structurally the same kind of transport
as the P1, not a framebuffer pipe.

Read from sysfs (`/sys/bus/usb/devices/1-1.3/`); `lsusb` is not installed on this
machine.

## Second — three MIDI ports, and only one of them answers

ALSA exposes `ADVANCE25 MIDI 1`, `MIDI 2`, `MIDI 3` (seq clients `32:0`, `32:1`,
`32:2`), each In/Out.

A Universal Device Inquiry (`F0 7E 7F 06 01 F7`):

- sent on **MIDI 3** → reply arrived on **MIDI 1**, ~9 ms later
- sent on **MIDI 2** → no reply within 800 ms
- sent on **MIDI 1** → not yet established (the one successful run's output was
  truncated before that line; later attempts were against an already-wedged device)

So command-in and reply-out are on *different* ports. Do not assume a port is
bidirectional for protocol purposes just because ALSA lists it as In/Out.

## Third — the device identifies itself, and the numbers check out

Reply, verbatim:

```
F0 7E 00 06 02 47 2F 00 19 00 01 02 05 00 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F 7F F7
         │  │     │     │     │           └─ 22 bytes of 0x7F padding
         │  │     │     │     └─ version 01 02 05 00
         │  │     │     └─ family member 0x0019 = 25
         │  │     └─ family 0x002F
         │  └─ 0x47 = Akai
         └─ 06 02 = Device Inquiry Reply, device ID 0
```

Two independent corroborations that this decode is right:
- family `0x002F` equals the USB product ID `002f`
- family member `0x0019` equals **25**, the key count

The padding is fixed-width `0x7F`, i.e. a fixed-size reply record, not a string.

**The significance is not the identity — it is that the device answers at all.**
The P1 had no read-back on any field, which is why `DeviceState` had to *be* the
state and assert itself in full. The Advance replies to at least one query. If
that generalises to Akai-format queries, this device can be *probed* rather than
assumed, and the driver architecture differs from the P1's accordingly.

Not yet established: whether any `F0 47 …` command elicits a reply. Only the
universal inquiry has been confirmed.

## Fourth — idle is silent, but the first-open burst is real and reproducible

On the very first port-open after enumeration, a burst of identical `A0 00 00`
messages (poly key pressure, ch 1, note 0, pressure 0) arrived within ~3 ms.

A subsequent 10-second listen, ports freshly opened, nothing touched:
**completely silent on all three ports.** Idle really is silent — the device
sends nothing periodic and expects no keepalive.

### Correction

The first version of this finding concluded from that silence that the burst was
"a one-off at first enumeration". **That was wrong**, and a power cycle disproved
it. The burst is emitted at the first port-open *after each enumeration*; the
silent 10-second run missed it only because the device had already been opened
once since it enumerated. Watching across a deliberate power cycle produced it
again, and more precisely than the first capture:

```
[0.001s] MIDI 1   B0 10 01          <- CC 16 = 1, single, distinct
[0.001s] MIDI 1   A0 00 00  x16
[0.002s] MIDI 2   A0 00 00  x16
[0.003s] MIDI 3   A0 00 00  x16
```

**Exactly 16 per port**, on all three ports, not a ragged count — so this is a
deliberate initialisation sweep, not noise. The Advance 25 has 8 knobs and 8
pads; 16 is suggestive but unproven, and the messages are all note 0 rather than
notes 0-15, which argues against a straight per-control sweep. Not yet explained.

The leading `B0 10 01` (CC 16, value 1) on MIDI 1 only is new — it was missed in
the first capture because that run's output was truncated. Unidentified.

Methodology note, same shape as the P1's `clear_popup_highlight` error: a silent
observation window is not evidence of absence when the trigger condition
(enumeration) was not reproduced inside that window.

## Fifth — the device wedges under an unpaced SysEx flood *(caution)*

Sent ~1 MB to MIDI 3 as 2000 × 512-byte SysEx messages with manufacturer ID
`0x7D` (reserved for non-commercial use; no Akai device should act on it), with no
pacing between messages.

Result: **the MIDI interface stopped responding.** The device remained enumerated
on USB (`/proc/asound/cards` still lists it), but Device Inquiry got no reply on
any of the three ports at t+0, +20 s, +40 s, +60 s. Power cycle required.

Two readings were open at the time:
- the device's SysEx parser choked, or
- ~1 MB is still queued in the kernel/USB layer and the inquiry is stuck behind it

**Resolved: a power cycle clears it completely.** After power-cycling, the device
re-enumerated in ~10 s, emitted its normal first-open burst, and answered Device
Inquiry immediately with the identical reply as before. Nothing was persistently
damaged and no reset procedure was needed.

That rules out permanent harm but does *not* distinguish parser-choke from
queue-backlog — the power cycle destroys the evidence for both. Distinguishing
them needs `usbmon` on the host during the flood, to see whether bytes are still
moving on the wire while the device is unresponsive.

The operational rule is unchanged: **pace writes to this device, and never send
unbounded bursts.** Flow control is a first-class concern here, not an
optimisation. Any sweep harness must checkpoint to disk and be resumable, because
wedging is an expected outcome rather than an exceptional one.

## Sixth — throughput is NOT yet measured *(retraction)*

The first throughput run reported ~89 MB/s and derived per-frame timings from it.
**That figure is meaningless and the derived numbers are withdrawn.** `midir`'s
`send()` returns once the message is buffered into ALSA; the run measured how fast
the buffer accepts bytes, not how fast the wire drains. The device is full-speed
USB — ~89 MB/s is not physically possible on it.

The question the measurement was meant to answer is still open and still matters,
because it bounds how free-form the screen can be: *what is the real byte rate to
the device?* A valid method has to measure drain, not enqueue — e.g. pace a known
volume, then time how long a trailing Device Inquiry takes to come back.

---

## Open questions

- Does MIDI 1 accept commands, or is it reply-only?
- What is MIDI 2 for? It answered nothing and emitted only the first-open burst.
- Does any `F0 47 <dev> 2F <cmd> … F7` message elicit a reply? (Akai's MPK2-series
  documented opcodes are the obvious first candidates to try against family `0x2F`.)
- Real sustained byte rate to the device.
- Is there a bitmap/blit command at all, or is the screen strictly structured
  text and widgets like the P1's? **This is the question that decides whether
  free-form drawing is possible on this device.**
- What are the 16 `A0 00 00` per port at first open, and what is `B0 10 01`?
- Under the flood: are bytes still moving on the wire (queue backlog) or has the
  parser stopped consuming (choke)? Needs `usbmon` during the flood.

## Seventh — the device runs Lua and has a real drawing API

From the official firmware updater (`Akai Professional ADVANCE Firmware Updater
1.0.10.dmg`), unpacked on Linux by parsing the UDIF container directly — no
`7z`/`dmg2img`/`binwalk` on this box, so `scratchpad/udif.py` reassembles the
zlib chunk map, and `scratchpad/unhex.py` decodes what it finds.

**Firmware is shipped as Intel HEX wrapped in SysEx:**

```
F0 47 00 2E 70 :020000040804ee\r\n:10000000...  F7
      │  │  └─ command 0x70
      │  └─ model 0x2E  (note: our Advance 25 reports family 0x2F — unexplained)
      └─ device 0x00
```

Two images, decoded with zero bad records:

| Image | Load address | Size | Contents |
|---|---|---|---|
| `fw_08040000.bin` | `0x08040000` | 768 KB | ARM Cortex-M code |
| `fw_08100000.bin` | `0x08100000` | 4 MB | assets — PNGs, fonts, Lua scripts |

The first record decodes to a textbook Cortex-M vector table: initial SP
`0x20020000`, reset vector `0x08072EF1` (odd = Thumb). A validity magic
`0x12345678` sits at `0x080FFFFC`. `post_dma2d_queue_empty` in the code image
points at STM32 DMA2D (Chrom-ART), so this is an STM32F4/F7-class part with
hardware 2D blitting.

### There is a Lua interpreter on the device

Lua 5.2, with the full standard library (`gsub`, `gmatch`, `sort`, metamethods,
`precompiled chunk` support — so it accepts bytecode as well as source).
Alongside it: **LodePNG** (decoder *and* encoder strings) and **FreeType**.

The firmware registers this API into the Lua state — read directly from the
name table at `0x080ADA78`–`0x080ADCA0`:

| Group | Functions |
|---|---|
| Drawing | `draw_rect`, `draw_image`, `draw_text`, `draw_system_text`, `decode_image` |
| Widgets | `lua_widget_make_dirty`, `post_page_draw`, `asset_get_valid` |
| LEDs | `led_control_set_level`, `led_control_set_level_midi` |
| Events | `set_hook_enabled`, `note_generated`, `get_byte` |
| Loader | `lua_ifc_load_script` |
| Diagnostics | `mem_usage`, `clear_errors`, `lua_debug` |

`draw_text` takes a table with `color`, `font`, `font_size`, `just_hor`,
`just_ver`, `padding_hor`, `padding_ver`, `bk_color`, `border_color`,
`border_width_{left,top,right,bottom}`, `text_data` — a styled-text widget, not a
fixed label slot.

### A complete working script ships in the asset region

At `0x0810E979` there is a readable Lua widget (a note-visualiser), reproduced in
full in the scratchpad. The important parts:

```lua
MAX_W = 480
MAX_H = 140

function draw_note_rain (args)
  draw_rect(0, prev_line-PAST_ERASE_PX, MAX_W, PAST_ERASE_PX-SPEED_PX, 0x20000000)
  draw_rect(0, prev_line, MAX_W, SPEED_PX, 0xff000000)
  ...
  lua_widget_make_dirty (WID)
end

function draw (args) draw_note_rain(args) end
function note (args)
  note_status[get_byte(args,1)] = get_byte(args,2)
end
set_hook_enabled(2,1); -- Enable note hook
```

So: `draw_rect(x, y, w, h, argb)` with **real alpha** (`0x20000000` is used as a
fade-erase), a widget dirty/invalidate model, and an event-hook system with
per-hook enable. The drawing area here is 480x140 — that is a widget region, not
proven to be the whole panel.

**This is the answer to "do we get free-form control of the display": the
hardware and stock firmware plainly support it.** That is a much stronger
position than the P1, where the screen was a fixed set of addressable text slots.

### What is NOT yet established

The open question is now precise, and it is a tier question:

- Scripts and assets live in the `0x08100000` region. The string
  `Assets missing. Run Firmware Update Mode.` indicates that region is written
  through the same update path as the code — i.e. a persistent flash write.
- **Unknown: whether `lua_ifc_load_script` is reachable at runtime over SysEx.**
  If it is, we can push a widget without flashing anything and free-form drawing
  is available at tier 1. If it is not, installing our own widget means writing
  the asset region, which is a persistent modification.

VIP must have *some* runtime protocol, since it updates parameter names and
values live. Whether that protocol carries arbitrary draw calls or only populates
resident widgets is the thing to find next.

Note also that the firmware-update command uses model `0x2E` while our Advance 25
answers Device Inquiry with family `0x2F`. Not yet explained; do not assume the
two are interchangeable.

## Eighth — VIP drives the screen by uploading Lua at runtime. Free-form is tier 1.

The Seventh finding left one question open: whether `lua_ifc_load_script` is
reachable at runtime, or whether installing a widget means flashing the asset
region. **It is reachable at runtime, and VIP does exactly that.**

Unpacked `VIP-3.1.2.1-Mac-Update.zip` on Linux without macOS tooling —
`tooling/firmware/xar.py` reads the `.pkg` (XAR) container, `tooling/firmware/cpio.py`
reads the gzip'd `odc` cpio Payload inside it. Being a macOS build is no obstacle
to reading it, and Mach-O keeps more symbols than a stripped Windows build would.

### The evidence

Strings in `VIP.vst/Contents/MacOS/VIP`:

```
Too many widgets created on device
Too many page created on device
created all hardware pages  /  destroyed all hardware pages
%s called. script not loaded
../lua_scripts/bitmapButton.lua
../lua_scripts/instancesList.lua
ADW1Interface::heartbeat() missed a ping.
luaSetTextIndex > 1500
```

VIP creates pages and widgets *on the device*, loads Lua scripts by name, and
calls into them — all against stock firmware, with nothing flashed.

### The scripts ship as readable source

All 20 of them, at
`Library/Application Support/VIP/skins/lua_scripts/*.lua`. They are Akai's files
and are deliberately not committed here; they live under `~/.cache/hacpad-re/`
for reference. `splash.lua` in full is short enough to be worth quoting:

```lua
local DEFAULT_TEXT = {
	text = "Launching VIP", color = 0xffffffff,
	font = 9,  -- AeileronRegular
	font_size = 18, just_ver = 1, just_hor = 0,
	bk_color = 0x00050505, border_color = 0x88ffffff,
	border_width_top = 0, ...
}
TEXT_TITLE = text_data.new()
text_data.set (TEXT_TITLE, DEFAULT_TEXT )

function draw (args)
  draw_rect(0, 0, 480, 272, 0xff000000)
  draw_text(TEXT_TITLE, 185, 80, 200, 30)
  if (decodedID ~= 0) then draw_image (18, decodedID, 223, 130) end
end

function set_decoded(args)
    decodedID = get_byte(args, 0) * 256 + get_byte(args, 1)
end

function decode_png_asset(args)
    local asset = get_byte(args, 0) * 256 + get_byte(args, 1)
    if (get_byte(args, 3) == 0) then
        decode_image(14, asset, 18, asset, 0xFFFFFFFF)  -- white as transparent
    else
        decode_image(14, asset, 18, asset)
    end
end
```

**`draw_rect(0, 0, 480, 272, ...)` establishes the screen as 480x272**, which the
`MAX_W = 480 / MAX_H = 140` in the firmware's own widget did not (that was a
widget region, not the panel).

### The device API, counted across all 20 scripts

| Function | Uses | Shape |
|---|---:|---|
| `draw_rect` | 175 | `(x, y, w, h, argb)` — alpha is real |
| `text_data.set` | 124 | `(handle, style_table)` |
| `draw_image` | 61 | `(bank, asset_id, x, y)` |
| `draw_text` | 51 | `(handle, x, y, w, h)` |
| `text_data.new` | 46 | returns a styled-text handle |
| `decode_image` | 7 | `(src_bank, src_id, dst_bank, dst_id [, transparent])` |
| `led_control_set_level_midi` | 24 | LED brightness |
| `get_byte` | 322 | `(args, n)` — unpack host-supplied bytes |
| `lua_widget_make_dirty` | 1 | invalidate |
| `send_arg` | 2 | **script -> host**, so the channel is bidirectional |

Fonts are referenced by number (`font = 9 -- AeileronRegular`). `bit32` is
available. Colours are ARGB throughout, and alpha composites (`0x20000000` used
as a fade, `0x88ffffff` as a translucent border).

Entry points are **arbitrary function names**: the scripts define `draw`, `init`,
`set_text`, `set_led_state`, `setColors`, `select`, `highlight`, `read_int32`,
`read_string` and so on, and the host invokes them by name with byte arguments.
That matches the firmware's `Lua call_function '%s' error`.

### What this means for hacpad

This device is not a set of addressable text slots like the P1. It is a
**480x272 ARGB framebuffer with a scripting runtime, hardware 2D blitting, PNG
decoding and font rendering**, and the host can push code to it at runtime.
On-screen drawing — the thing hacpad exists for — is available here at **tier 1**,
on stock firmware, with nothing flashed.

### Still unknown: the wire format

We now know these operations exist and are runtime-reachable. We do **not** yet
know how they are encoded on the wire. Specifically unresolved, and important:

- **VIP bundles `libusb-1.0.0.dylib`.** It may not use CoreMIDI at all, and may
  instead claim the USB interface and drive the bulk endpoints directly. If so
  the transport is not MIDI SysEx and our ALSA-based client is the wrong shape —
  we would want libusb on Linux too, which would also sidestep the Fifth
  finding's wedge, since that was provoked through the ALSA path.
- The encoding of script-upload, page-create, widget-create and call-function.
- What `ADW1Interface::heartbeat()` expects, and what happens when it is missed.

That is the next thing to settle, and a MIDI-layer or libusb-layer capture of VIP
talking to the device would settle all of it at once.

## Ninth — transport is MIDI after all, and the runtime command frame is decoded

### Correction to the Eighth finding

The Eighth finding worried that VIP's bundled `libusb-1.0.0.dylib` might mean it
bypasses CoreMIDI and drives the bulk endpoints directly, making our ALSA-shaped
client wrong. **It does not.** VIP imports exactly five libusb symbols:

```
libusb_init  libusb_exit
libusb_get_device_list  libusb_free_device_list
libusb_get_device_descriptor
```

No `libusb_open`, no `claim_interface`, no `bulk_transfer`. libusb is used purely
for **enumeration** — noticing what is plugged in. All data goes over CoreMIDI,
and the linked frameworks plus `MidiInCore` / `MidiOutCore` / `RtMidiIn` /
`RtMidiVipDeviceInterface` symbols show VIP uses **RtMidi**.

So the runtime protocol is SysEx over MIDI, and the existing ALSA client is the
right shape. (The `libusb` guess was reasonable but wrong; recorded rather than
edited out.)

### The runtime parser

Found by disassembling (capstone, Thumb) for `cmp rN, #0x47` — the Akai ID check.
The runtime handler is at `0x0805A6BA`, distinct from the `0x70` firmware-update
path. It validates `0x47` at buffer offset 5, then:

```
0x0805a6dc  ldrb  r0, [r2, #0xa]        ; length high
0x0805a6de  ldrb  r3, [r2, #0xb]        ; length low
0x0805a6e0  orr.w r3, r3, r0, lsl #7    ; 14-bit, 7 bits per byte
0x0805a6f2  ldrb  r2, [r2, #8]          ; command
0x0805a708  subs  r2, #1
0x0805a712  cmp   r2, #5
0x0805a714  bhi   <return>              ; so command is 1..6 ONLY
0x0805a716  tbb   [pc, r2]
```

Which gives the frame:

```
F0 47 <dev> <model> <cmd> <sub> <len_hi7> <len_lo7> <payload...> F7
```

The `0x70` firmware-update command from the Seventh finding is outside `1..6`, so
it is handled by a separate (bootloader) parser — consistent, not contradictory.

### The command map

`tbb` table on `<cmd>`, read as data:

| cmd | handler | shape |
|---:|---|---|
| 1 | `0x0805A7A6` | takes buffer + length (`subs r1,#4`, buf `0x2000FE91`) -> `0x08059804` |
| 2 | `0x0805A79C` | takes buffer + length, same buf -> `0x0805A9F8` |
| 3 | `0x0805A76C` | sub-dispatch on `<sub>`, 9 entries |
| 4 | `0x0805A762` | `<sub>` as a byte arg, buf `0x2000FE94` -> `0x08059DAC` |
| 5 | `0x0805A758` | `<sub>` as a byte arg, same buf -> `0x08059F7C` |
| 6 | `0x0805A72C` | `<sub>`: 0/1 -> handler; 2 -> builds a **reply** |

Sub-dispatch under cmd 3 (`ldr.w pc, [r1, r2, lsl #2]`, 9 entries):

| sub | target | note |
|---:|---|---|
| 0 | `0x0805A7BA` | |
| 1 | `0x0805A6D6` | return stub — **unimplemented** |
| 2 | `0x0805A7B6` | |
| 3 | `0x0805A7E2` | |
| 4 | `0x0805A6D6` | return stub — **unimplemented** |
| 5 | `0x0805A6D6` | return stub — **unimplemented** |
| 6 | `0x0805A7C4` | |
| 7 | `0x0805A7F4` | |
| 8 | `0x0805A7EE` | |

Commands **1 and 2 are the bulk-payload commands** — they are the only two that
take a buffer pointer and a length, so script upload and/or call-function live
there. Command 6 sub 2 is the only path seen so far that constructs a reply
(caps its payload at 1000 bytes, then calls the send routine with `(6, 3, ...)`).

### Can we load our own Lua and talk to it?

Everything points to yes, and nothing so far argues against it:

- the interpreter is resident and VIP demonstrably loads scripts into it at
  runtime, on stock firmware
- the transport is plain MIDI SysEx, which we already speak
- the frame is decoded
- the command space is **six commands**, not a 128-wide blind sweep

What is still missing is only the payload layout inside commands 1 and 2 — which
carries the script, and which carries the function invocation. That is a small,
bounded search, and we have two oracles for it: the reply path (cmd 6 sub 2
proves the device answers) and the camera.

Not yet proven, and it should not be asserted until a script of ours is observed
running on the panel.

## Tenth — the frame is CONFIRMED on hardware

`prototypes/akai-advance/src/bin/frame_probe.rs` sends each op with a
deliberately wrong payload length, so the firmware must take its NAK path
without ever running the operation. Twenty ops, twenty NAKs:

```
sent: F0 47 00 2F 02 39 00 03 00 00 00 F7        (op 0x39, which wants len 2)
got:  F0 47 00 2F 02 3D 00 04 0D 00 39 47 F7
                     |  |     |  |  |  +-- 0x47, the `movs r3, #0x47` constant
                     |  |     |  |  +----- the op we sent
                     |  |     |  +-------- 0x00, from `movs r1, #0`
                     |  |     +----------- per-op code, from `movs r0, #N`
                     |  +----------------- length 4
                     +-------------------- op 0x3D = reply
```

Every field matches the disassembly of the error path exactly. **Addressing is
`dev = 0x00`, `model = 0x2F`** — the family byte from Device Inquiry, not the
`0x2E` the firmware updater used. So the confirmed runtime frame is:

```
F0 47 00 2F <cmd> <op> <len_hi7> <len_lo7> <payload...> F7
```

The per-op length table in the Ninth finding is confirmed too: each op NAKs when
given `want + 1` bytes, and the NAK names the op. That is a precise, safe oracle —
we can probe the shape of any op without executing it.

The device stayed alive through the whole sweep. Paced at 120 ms, this provokes
none of the Fifth finding's wedging.

### One caveat on the probe

Phase 1 (finding `dev`/`model`) treated *any* inbound message as a reply, and the
first one it saw was a stray `A0 00 00`. The pair it settled on happened to be
correct — phase 2's real NAKs prove it — but the detection itself was a false
positive and should match on `F0 47` before being trusted.

### Not yet done: drawing

Loading a script is cmd 2 op `0x3B`. Its payload is
`<id:2> <size:2> <count:2> <packed bytes>`, ids are bounded at 1024 by
`cmp.w r0, #0x400`, and the body is bit-packed by the routine at `0x08068070`
(14-bit count, then a 7-state bit shuffle) rather than by the naive 7-bit MIDI
scheme. **That packing is the one piece not yet worked out**, and nothing has
been drawn on the panel yet. No claim of on-screen output until there is a photo.

## Eleventh — VIP under Wine is blocked by PACE; pivoting to drive the device directly

VIP installed cleanly under Wine into `~/.wine-vip` (standalone at
`C:\vst3\VIP_x64.exe`). Only `Authorizer.msi` failed, with `0x80070643`,
because it installs `iLokDrvr.sys` -- a kernel driver Wine cannot load. But the
deeper wall is that the VIP binary is **PACE-Eden wrapped**, and the wrapper
refuses to start under Wine. PACE kernel-mode protection is a known dead end
there; the only way to run VIP for real is a Windows VM with a genuine iLok
authorization.

Not worth it, because **we do not need VIP**. It would only have handed us the
session *choreography* as a shortcut. What we already have is sufficient to drive
the screen ourselves:

- the runtime frame, confirmed on hardware (Tenth finding)
- the full command dispatch tables (cmd 1..6; the 61-entry cmd-2 table)
- op `0x3B` = load Lua script, wired to `luaL_loadbuffer`
- all 20 of VIP's own Lua scripts, as readable source, showing the exact API and
  a working 480x272 draw loop

The one remaining unknown is the bit-packing of the script-upload payload (the
routine at `0x08068070`). Decoding that, rather than capturing VIP, is now the
path to our own Lua on the panel.

## Twelfth — load-script CONFIRMED on hardware; we can put Lua on the device

`prototypes/akai-advance/src/bin/load_script.rs` builds the op-0x3B message and
sends it. The payload format, fully derived from firmware, is confirmed:

```
F0 47 00 2F 02 3B <outerlen:2x7> <id:2x7> <len:2x7> <N:2x7> <packed> F7
```

- id = script slot (<1024); len and N both = script byte count
- <packed> = script as a contiguous LSB-first 7-bit stream (packer validated by
  round-trip against a faithful re-impl of the firmware unpacker at 0x08068070:
  25 named cases + 2000 fuzz, the only misses being scripts over the 14-bit
  length ceiling of 16383 bytes)
- all three length fields use (hi<<7 | lo), confirmed at 0x08067fcc

The device replies:

```
F0 47 00 2F 02 3D 00 04 0D <slot> 3B 40 F7
                           |       |  +- 0x40 ACK code
                           |       +- op 0x3B echoed back
                           +- slot id (0x64=100, 0x65=101 across two runs)
```

**Caveat, proven by a control run:** a deliberately-broken (syntax-error) script
got the same `0x40` ACK as a valid one. The handler hard-codes 0x40 and ignores
the loader's return value (matches the disassembly). So the ACK confirms "message
received, slot written" — NOT "script compiled and ran." The device stayed alive
through both loads; paced single messages provoke no wedge.

Open: whether a script whose *main chunk* calls draw_rect paints the panel at
load time (the firmware runs the chunk on load), or whether drawing needs a
separate page/widget/flush trigger from among the other cmd-2 ops. That is the
next thing to resolve, and only the screen can answer it.

## Thirteenth — the script lifecycle, and why loading alone does not draw

Disassembling the cmd-2 workers (all share the script table at RAM 0x2000F360)
gives the Lua-slot lifecycle:

| op | want | worker | meaning |
|---|---|---|---|
| 0x39 | 2 | 0x080654B0 | **create_slot(id)** — malloc an 8-byte slot, table[id]=slot; slot->state stays null |
| 0x3B | var | 0x0806571C | **run(id, chunk)** — load+pcall the packed chunk in slot[id]'s state |
| 0x3C | var | 0x08065668 | **call(id, name, args)** — push global `name`, pcall with args |

Both 0x3B and 0x3C require slot[id] AND slot->state to be non-null, or they bail
(0x41 no slot / 0x4D null state). That explains the first load doing nothing:
slot 100 was never created, so 0x3B ran in nothing.

**Confirmed on hardware:** create_slot(100) then load now both ACK
(`...39 40 F7`, `...3B 40 F7`), device stays alive. But the panel is still
UNCHANGED — photographed via the viewer, still the standalone SETUP/DAW-select
page.

The reason is the compositor, not the load: the active firmware page redraws
every frame and overwrites anything our chunk's draw_rect paints. Our draw goes
to a back-buffer the native page immediately clobbers. To make pixels stick we
must become the ACTIVE page — i.e. create a page, put a widget bound to our
script on it, and show that page. Those are the VIP operations behind the strings
"created all hardware pages" / "Too many page created on device" / "Too many
widgets created on device".

Next target: the page/widget ops. Prime candidates are 0x00 and 0x2D (their
workers 0x0806255C / 0x08062A20 both allocate-and-init via a 5-call pattern
including 0x08064FCC) for create, and a single-byte op like 0x10 (0x08063BC4) for
show/select.

Open lifecycle gap: which op sets slot->state (allocates the Lua VM). create_slot
leaves it null, yet the loads ACK — so either a state is lazily created, or the
ACK (hard-coded 0x40) is hiding a 0x4D. To be resolved with the page work.

## Fourteenth — op 0x10 = set active page (camera-verified)

`send_op.rs` is the interactive workbench: it sends any cmd-2 op with a raw
payload, paced and liveness-checked, and prints replies.

op 0x10 (want 1) has the simplest possible worker (0x08063BC4):
`str r0, [0x200011D0]` — it stores one byte into a global. Sending
`op 0x10 = 01` switched the panel OFF the standalone SETUP/DAW-select page to a
blank page (confirmed by webcam). Reply echoes the value:
`F0 47 00 2F 02 3D 00 04 04 01 10 40 F7`.

So op 0x10 is **set_active_page(n)** and 0x200011D0 is the active-page global. This
is the compositor lever we needed: we can take the display away from the native
SETUP page that was clobbering our draws.

Still no red rect: with a blank page active, re-loading the draw_rect script (slot
100) leaves the panel blank/black, not red. Loading + running the chunk is not
enough — slot 100 is not a WIDGET on the active page, so the compositor never
calls its draw(). The remaining link is create-widget: bind script slot 100 to a
widget on the active page with a region. op 0x00 (worker 0x0806255C, a
create-returning-handle taking id + 2 bytes) is the create candidate.

Fallback if the firmware side stalls: disassemble VIP_x64.dll (x86-64, in the
Wine prefix) around its "created all hardware pages" / "Too many widgets" strings
to read the exact op sequence VIP emits.

## Fifteenth — the widget system: create (0x00) and configure (0x2f)

- **op 0x00 = create_widget(id, type, arg)** (want 4). Worker 0x0806255C:
  malloc a 52-byte struct into widget table 0x2000EF54[id]; sets a type byte
  and a default colour field to 0xFFFF0000 (opaque red); geometry fields
  (offsets 8, 0xC) start zero, so a freshly-created widget has zero size and
  draws nothing. Also sets struct[0x30]=1, struct[0x31]=type. Does NOT attach to
  a page or bind a script by itself.
- **op 0x2f = configure_widget** (want 26). Worker reads id then TWELVE more
  2-byte (14-bit) values into the struct — matching the draw_text style table
  (color, font, size, just_hor/ver, padding_hor/ver, bk_color, border color +
  4 widths). So this is the full per-widget style/geometry setter.

Getting our rect visible is therefore a multi-op recipe: create_widget (0x00) ->
configure geometry/colour (0x2f or a sibling) -> attach widget to a page ->
set_active_page (0x10, done) -> the compositor calls the widget's draw. Two ops
still unpinned: page-create and widget->page attach.

Decision: the firmware widget system is a web of interlocking ops/structs; the
efficient way to get the exact sequence and argument layout is to disassemble
VIP_x64.dll (x86-64 PE in the Wine prefix, 29 MB) around its "created all
hardware pages" / "Too many widgets created on device" strings, which are present
at 0xC4BB58 / 0xC4CAE0. That is the next step.

Camera-verified so far this session: set_active_page works (panel leaves the
SETUP page). Not yet drawing our own pixels.

## Sixteenth — the widget/page table map, and a realistic scope read

Which cmd-2 ops touch which RAM table (a fast way to carve the ~50 ops into
subsystems):

| Table | RAM | Ops | Meaning |
|---|---|---|---|
| widgetTblA | 0x2000EF54 | 0x00-0x06, 0x0d, 0x3a | widgets |
| pageTbl (tblB) | 0x2000EF5C | 0x0d-0x11, 0x22, 0x26 | pages |
| tblC | 0x20010CE4 | 0x10, 0x11, 0x31, 0x32, 0x34 | text_data / elements |
| scriptTbl | 0x2000F360 | 0x15, 0x39, 0x3a, 0x3b, 0x3c | Lua slots |
| activePage | 0x200011D0 | 0x10, 0x11 | current page |

Confirmed pieces of the draw recipe:
- op 0x00 create_widget, op 0x2f configure_widget (12-field style), op 0x10
  set_active_page (camera-verified), op 0x39/0x3b/0x3c script lifecycle.
- **op 0x3a (want 5) binds a widget to a script** — it touches both scriptTbl and
  the widget table. BUT its worker (0x08066318) runs a Jenkins-style hash
  (magic 0xFEEDBEF3), so the binding is keyed by a NAME HASH, not a raw slot id.
  That means our script has to be addressable by the hash VIP/firmware expects,
  which is an extra wrinkle.
- Page create + widget->page attach live among ops 0x0d/0x0e/0x0f/0x11 (pageTbl),
  not yet individually pinned.

### Scope

Getting our own rect drawn is a multi-op choreography over four interlocking
tables (create page, create widget, set geometry, hash-bind a script or use a
self-drawing widget type, attach to page, show page), several of whose exact
argument layouts are still unknown. This is a substantial reverse-engineering
effort, not a next-command win. What IS won and camera-verified: we can load and
run Lua on the device, and we can control which page is active.

The VIP_x64.dll disassembly shortcut stalled: its page/widget log strings
("created all hardware pages", etc.) are in an embedded data blob with no
RIP-relative or absolute code references found, so the message-builders are not
trivially anchored by those strings. A better VIP anchor would be the SysEx
send path (the code writing the F0 47 00 2F 02 header), or the op-byte immediates.

## Seventeenth — VIP static analysis is closed (PACE-encrypted)

VIP_x64.dll is PACE-packed, so its code cannot be read statically:
- imports ONLY KERNEL32/USER32/SHELL32 (a 29 MB MIDI+graphics+Lua plugin cannot
  really import three basic DLLs — everything else is resolved at runtime)
- the plaintext strings ("created all hardware pages", "lua_scripts", the Lua
  source) have ZERO references of any kind (RIP-relative, absolute pointer, or
  32-bit RVA)
- entropy: .text = 8.00 with only ~10 valid instructions per 4 KB, .data = 8.00
  (both encrypted); only .rdata is plaintext (5.34); .guard (7.17) is the stub

So both VIP routes are blocked by the same PACE protection: it won't run under
Wine, and its code won't disassemble. Unpacking PACE is a DRM-defeat last resort
we are not taking. The recipe must come from observable artifacts: the ARM
firmware (readable), on-device probing, and the camera.

## Eighteenth — the page map, and why the Lua-hijack fails

Cycling set_active_page (op 0x10) 0-11 and photographing each (montage saved)
maps the firmware's built-in standalone pages:

| page | content |
|---|---|
| 0 | SETUP / DAW select (Ableton, FL, Reaper, Studio One, MPC) |
| 1 | BLANK |
| 2,3 | knob views (8 rotary encoders, e.g. "Logic Pro X", CC values) |
| 4,5 | pad views (coloured pad grid) |
| 6,7 | button views |
| 8-11 | GLOBAL settings (channel, brightness, tempo, save/factory-reset...) |

Attempted hijack: set page 2 active, then redefined draw() in script slots 0..31
with a full-screen red fill via op 0x3B. **Page 2 stayed green** — no effect.

Conclusion: the standalone pages are rendered by native firmware code, not by Lua
widgets. The Lua widget system is dormant in standalone mode; VIP is what creates
the pages/widgets/scripts that use it. So there is no shortcut by hijacking an
existing page — drawing our own pixels requires replicating VIP's create recipe.

Encouraging detail for a no-Lua path: op 0x00 create_widget defaults the widget
colour field to 0xFFFF0000 (red) and stores a type byte at struct+0x31. If a
widget TYPE self-draws as a filled rect in its colour, the recipe could be pure
widget ops (create_page -> create_widget(type=rect) -> set geometry -> attach ->
show) with no script at all. Next: find the widget-draw dispatch (reads
struct+0x31, switches on type) to learn the type enum and which type fills.

## Nineteenth — draw_rect is widget-context-only; no flush/hijack shortcut exists

The Lua draw_rect C function (0x0805342C, from the luaL_Reg table at 0x08053758)
reads its 5 args, then calls the widget-context resolver 0x08066480 (the same one
op 0x3a's bind uses) and computes every coordinate relative to the CURRENT
widget's draw origin (globals 0x2000F364 / 0x2000F368, set only during widget
composition). Called outside a widget's draw() -- e.g. from a top-level chunk we
load -- there is no valid current widget, so the draw lands nowhere.

This closes the last hoped-for shortcut: there is no "draw then flush" and no way
to paint without a widget. A visible rect strictly requires a real widget on the
active page that the firmware composites. Combined with the Eighteenth finding
(standalone pages are native, Lua dormant), the ONLY way to pixels is to
replicate VIP's full create recipe: create_page, create_widget, set geometry,
bind/point at content, attach to page, set_active_page.

### Strategic state (honest)

- VIP holds the recipe but is PACE-encrypted (Seventeenth) -> unreadable.
- The firmware has all the ops but the recipe is all-or-nothing: nothing shows on
  the panel until the ENTIRE chain (page+widget+geometry+attach+content+show) is
  correct, so the camera gives no incremental feedback to reverse args by
  experiment. That makes blind firmware reversing of the arg layouts slow and
  uncertain.
- The reliable route to the exact sequence is to CAPTURE VIP driving the device:
  a Windows VM with the user's real iLok/VIP authorization, device passed
  through, and usbmon capturing on the Linux host. Heavy setup but definitive --
  it hands over the precise op order and argument layout in one session.

Recommendation: either commit to a methodical firmware-reversing grind of the
remaining ops (create_page / attach / geometry / widget-type enum / the bind
hash), or stand up the VM capture. The VM capture is the higher-confidence path
now that VIP static analysis is closed.

## Twentieth — widgets are script-backed; the no-Lua path does not exist

The widget module (0x8062xxx, all 17 widget-table references live here) shows the
widget struct embeds a script context: destroy_widget (0x080629B0) frees it via
the script module (0x08065108) at widget+4, and the render accessors invoke the
script to draw. struct+0x31 is a FLAGS byte (bits set/cleared), not a
self-drawing type enum; struct+0x30 is the valid flag.

So there is no "filled-rect widget type that self-draws its colour" -- every
widget draws through a bound Lua script, and binding is the hash-keyed op 0x3a.
The default 0xFFFF0000 colour is just a field a script would read.

### Conclusion of the static-firmware effort

Drawing our own pixels requires the complete VIP recipe -- create_page,
create_widget, create_slot + load a draw script, hash-bind the widget to the
script, set geometry, attach to the active page, show -- and it is all-or-nothing:
the panel shows nothing until every step is correct, so the camera gives no
partial signal to reverse the argument layouts by experiment. The one step that
resists static reversing is the name-hash binding (op 0x3a, Jenkins 0xFEEDBEF3),
whose expected key we cannot derive from the firmware alone.

The static observable artifacts (ARM firmware, on-device probing, camera) are now
substantially exhausted for this purpose. The remaining observable artifact that
WOULD yield the exact recipe is VIP's RUNTIME traffic -- captured in a Windows VM
with the real iLok authorization, device passed through, usbmon on the Linux host.
That is the recommended path; the per-10-min firmware loop was stopped here as it
had reached a firm negative.

## Approach

Ranked by leverage, given VIP is Windows/macOS only and this host is Ubuntu LTS:

1. **Unpack VIP on Linux** (`7z` / `innoextract` / `msiextract`) and look for font
   atlases or bitmap assets sized for a 4.3" screen. If VIP ships pixels, the
   device takes pixels; if it ships only strings, the device renders text itself.
   Needs no VM and no hardware, and may settle the free-form question outright.
2. **Windows guest under KVM** (available here: VT-x, `/dev/kvm`) with USB
   passthrough of `09e8:002f`, running VIP — and **`usbmon` capturing on the
   Ubuntu host**, since passthrough traffic still traverses the host USB stack.
   Ground truth with no instrumentation inside the guest.
3. **A custom VST loaded into VIP**, turning the capture into a chosen-plaintext
   attack: we pick the parameter names, so we know exactly which bytes to find.
   Sweep name length to find length fields, use non-ASCII to expose the 7-bit
   packing, vary parameter count to find paging. Buildable as VST3 via nih-plug
   cross-compiled to `x86_64-pc-windows-gnu` from Ubuntu.
4. **Opcode sweep against the reply channel**, using the fact that this device
   answers — error replies would separate "opcode exists, payload wrong" from
   "opcode does not exist". The P1 never had this.
5. **Firmware static analysis**, targeting the SysEx dispatch table specifically
   rather than whole-image comprehension. May be blocked by encryption; entropy
   test first before committing to it.

Note the downloads for VIP and the drivers are gated behind an akaipro.com
account and are separate downloads from each other, so they cannot be crawled
anonymously — they have to come from a logged-in session.
