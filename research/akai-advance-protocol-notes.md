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
