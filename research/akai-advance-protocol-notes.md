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
