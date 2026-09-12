# Panorama P1 USB Bridge — prototype

A hacpad USB device bridge prototype for the Nektar Panorama P1 controller. Reverse-engineered
entirely from scratch (own MIDI/SysEx capture, cross-checked against Nektar's official Bitwig
driver source and documentation) — no Bitwig install required to build or run this.

## Status: both directions confirmed working against real hardware

- **Input** — decodes the P1's physical faders, encoders, and buttons from their raw MIDI Control
  Change messages into a typed `Event` enum (`Fader`/`Encoder`/`Button`/`Unknown`).
- **Output** — writes arbitrary text to the P1's own screen via vendor SysEx, including multi-line
  banners (`\n`-separated). This was the hard, long-blocked part of this investigation — see
  `research/panorama-p1-protocol-notes.md` for the full story, in particular the "Fifth finding"
  (the actual root cause: an inverted MIDI port assignment, not a byte-level protocol error).

## Run

```
cargo run                    # send init, write a screen message, then listen for CC input
cargo run -- "some text"     # same, with a custom message (supports embedded \n for multi-line)
```

Requires the P1 connected over USB. Uses named ALSA sequencer ports via the `midir` crate (not the
plain rawmidi character device, which only exposes one of the P1's several MIDI cables) — no setup
needed beyond having the device plugged in on Linux.

## `src/bin/msg_test.rs`

A smaller, faster-iterating scratch binary used to test raw protocol hypotheses against hardware
without going through the full bridge. Supports `--raw "<hex bytes>"` to replay a literal SysEx
message verbatim. Kept around for continued protocol experimentation (e.g. the full per-field
page-composition path, not yet built — see protocol notes "Next steps").

## Protocol facts

All SysEx structure, the CC map, and the port-mapping story live in
`research/panorama-p1-protocol-notes.md`, not duplicated here — read that first if you're changing
the protocol-level code in this crate.
