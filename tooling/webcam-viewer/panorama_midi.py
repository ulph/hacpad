#!/usr/bin/env python3
"""
Minimal Python port of the confirmed Panorama P1 SysEx protocol (mirrors
prototypes/panorama-p1/src/main.rs and msg_test.rs -- see
research/panorama-p1-protocol-notes.md for how every byte here was
confirmed against real hardware). Kept deliberately separate from the Rust
prototype: this exists only so the webcam-viewer's screen simulator can
push live edits straight to the real device, without shelling out to cargo.

Port mapping (confirmed, "Fifth finding" in the protocol notes):
  default output port (Bitwig's bare sendSysex())      = "PANORAMA P1 Instrument"
  host.getMidiOutPort(1) (Linux-only init message only) = "PANORAMA P1 Internal"
"""
import rtmidi

PORT_DEFAULT_NAME = "PANORAMA P1 Instrument"
PORT_ONE_NAME = "PANORAMA P1 Internal"

PREFIX = [0xF0, 0x00, 0x01, 0x77, 0x7F, 0x01]
END = 0xF7

INIT_LINUX_ONLY = [0x08, 0x01, 0x00, 0x00, 0x01, 0x01, 0x75]
INIT_1 = [0x08, 0x02, 0x00, 0x00, 0x01, 0x01, 0x73]
INIT_2 = [0x09, 0x03, 0x00, 0x00, 0x01, 0x3E, 0x34]

CMD_WRITE_DISPLAY = 0x06
DISPLAY_ID_TITLE_BAR = 1
DISPLAY_ID_CTRL_NAME = 6
DISPLAY_ID_CTRL_VALUE = 7

# Confirmed page-template identities, see the "Summary" table in
# research/panorama-p1-protocol-notes.md ("Tenth finding").
LAYOUT_TEMPLATES = {
    "knobs": 16,
    "faders-split": 18,
    "faders-row": 19,
    "pads": 21,
    "list": 4,
    "grid5": 5,
}


def _sysex(body):
    return PREFIX + list(body) + [END]


def compose_write(page_template, display_id, entries):
    """entries: list of (index, text) pairs, index 1-based."""
    body = [CMD_WRITE_DISPLAY, page_template, display_id]
    for i, (index, text) in enumerate(entries):
        if i > 0:
            body.append(0x00)
        text_bytes = text.encode("ascii", errors="replace")
        body.append(index)
        body.append(len(text_bytes))
        body.extend(text_bytes)
    return _sysex(body)


def write_title_bar(page_template, segments):
    entries = [(i + 1, s) for i, s in enumerate(segments) if s]
    return compose_write(page_template, DISPLAY_ID_TITLE_BAR, entries)


def write_names(page_template, names):
    entries = [(i + 1, s) for i, s in enumerate(names[:8]) if s]
    return compose_write(page_template, DISPLAY_ID_CTRL_NAME, entries)


def write_values(page_template, values):
    entries = [(i + 1, s) for i, s in enumerate(values[:8]) if s]
    return compose_write(page_template, DISPLAY_ID_CTRL_VALUE, entries)


class PanoramaLink:
    """Holds open MIDI connections to the P1 and sends confirmed-format
    display writes. Best-effort: if the device isn't present, `connect()`
    raises and the caller decides whether that's fatal (the webcam-viewer
    server treats it as non-fatal, so the rest of the page keeps working
    without a physical device attached)."""

    def __init__(self):
        self._default_out = None
        self._port1_out = None

    def connect(self):
        default_out = rtmidi.MidiOut()
        port1_out = rtmidi.MidiOut()
        default_idx = _find_port(default_out, PORT_DEFAULT_NAME)
        port1_idx = _find_port(port1_out, PORT_ONE_NAME)
        default_out.open_port(default_idx)
        port1_out.open_port(port1_idx)
        self._default_out = default_out
        self._port1_out = port1_out
        self._send_init()

    @property
    def connected(self):
        return self._default_out is not None

    def _send_init(self):
        self._port1_out.send_message(_sysex(INIT_LINUX_ONLY))
        self._default_out.send_message(_sysex(INIT_1))
        self._default_out.send_message(_sysex(INIT_2))

    def send(self, message):
        if not self.connected:
            raise RuntimeError("not connected to the Panorama P1")
        self._default_out.send_message(message)

    def update_screen(self, layout, title_bar, names, values):
        template = LAYOUT_TEMPLATES.get(layout, 16)
        self.send(write_title_bar(template, title_bar))
        self.send(write_names(template, names))
        if layout == "knobs" and values:
            self.send(write_values(template, values))


def _find_port(midiout, needle):
    for i, name in enumerate(midiout.get_ports()):
        if needle in name:
            return i
    raise RuntimeError(f"no MIDI port matching {needle!r} found (device not connected?)")
