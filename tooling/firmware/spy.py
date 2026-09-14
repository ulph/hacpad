#!/usr/bin/env python3
"""Watch the Advance 25's USB traffic and decode it as Akai SysEx.

Intended for observing VIP drive the device: run VIP (natively or in a guest
with the device passed through — either way the URBs cross this host's USB
stack), use the hardware normally, and this prints the protocol as it happens.

usbmon's text interface is line-oriented, so no tshark is needed:

    sudo modprobe usbmon
    sudo mount -t debugfs none /sys/kernel/debug      # if not already mounted
    sudo python3 spy.py                               # or: sudo cat ... | python3 spy.py

USB-MIDI packs MIDI into 4-byte event packets; the low nibble of byte 0 is the
Code Index Number, which says how many of the following three bytes are real.
This reassembles those into MIDI messages, then decodes anything Akai-shaped
against the frame confirmed in research/akai-advance-protocol-notes.md:

    F0 47 <dev> <model> <cmd> <op> <len_hi7> <len_lo7> <payload...> F7
"""
import argparse
import os
import re
import sys
import time

# Code Index Number -> how many of the 3 data bytes are valid.
CIN_LEN = {
    0x0: 0, 0x1: 0, 0x2: 2, 0x3: 3, 0x4: 3, 0x5: 1, 0x6: 2, 0x7: 3,
    0x8: 3, 0x9: 3, 0xA: 3, 0xB: 3, 0xC: 2, 0xD: 2, 0xE: 3, 0xF: 1,
}

LINE = re.compile(
    r"^(?P<urb>\S+)\s+(?P<ts>\d+)\s+(?P<ev>[SCE])\s+"
    r"(?P<type>[BCIZ])(?P<dir>[io]):(?P<bus>\d+):(?P<dev>\d+):(?P<ep>\d+)\s+"
    r"(?P<rest>.*)$"
)

# Ops we have identified so far. Everything else prints as a bare number, which
# is the point — unknown ops are what we are here to find.
KNOWN_OPS = {
    (0x02, 0x3B): "load_lua_script",
    (0x02, 0x3D): "reply/nak",
}


def find_device():
    """Return (bus, devnum) for the Advance, or None."""
    root = "/sys/bus/usb/devices"
    for d in os.listdir(root):
        try:
            with open(f"{root}/{d}/idVendor") as f:
                if f.read().strip() != "09e8":
                    continue
            with open(f"{root}/{d}/idProduct") as f:
                if f.read().strip() != "002f":
                    continue
            with open(f"{root}/{d}/busnum") as f:
                bus = int(f.read().strip())
            with open(f"{root}/{d}/devnum") as f:
                dev = int(f.read().strip())
            return bus, dev
        except (OSError, ValueError):
            continue
    return None


class Stream:
    """Reassembles USB-MIDI event packets from one endpoint into MIDI messages."""

    def __init__(self, label):
        self.label = label
        self.sysex = bytearray()
        self.in_sysex = False

    def feed(self, data, out):
        for i in range(0, len(data) - 3, 4):
            pkt = data[i:i + 4]
            n = CIN_LEN[pkt[0] & 0x0F]
            if n == 0:
                continue
            body = pkt[1:1 + n]
            for b in body:
                if b == 0xF0:
                    self.in_sysex = True
                    self.sysex = bytearray([b])
                elif self.in_sysex:
                    self.sysex.append(b)
                    if b == 0xF7:
                        self.in_sysex = False
                        out.append(bytes(self.sysex))
                        self.sysex = bytearray()


def decode(msg):
    """Render one SysEx message, decoding the Akai runtime frame when present."""
    h = " ".join(f"{b:02X}" for b in msg)
    if len(msg) >= 9 and msg[1] == 0x47:
        dev, model, cmd, op = msg[2], msg[3], msg[4], msg[5]
        ln = (msg[6] << 7) | msg[7]
        payload = msg[8:-1]
        name = KNOWN_OPS.get((cmd, op), "")
        tag = f" {name}" if name else ""
        body = " ".join(f"{b:02X}" for b in payload[:24])
        more = f" ...(+{len(payload) - 24})" if len(payload) > 24 else ""
        txt = "".join(chr(b) if 0x20 <= b < 0x7F else "." for b in payload[:40])
        return (f"AKAI dev={dev:02X} model={model:02X} cmd={cmd:02X} op={op:02X}{tag} "
                f"len={ln} actual={len(payload)}\n"
                f"       {body}{more}\n"
                f"       \"{txt}\"")
    if len(msg) >= 5 and msg[1] == 0x7E:
        return f"UNIVERSAL  {h}"
    return f"SYSEX({len(msg)})  {h}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bus", type=int, help="USB bus (default: autodetect)")
    ap.add_argument("--dev", type=int, help="USB device number (default: autodetect)")
    ap.add_argument("--source", help="read from this file instead of usbmon")
    ap.add_argument("--raw", action="store_true", help="also print non-SysEx MIDI")
    args = ap.parse_args()

    bus, devnum = args.bus, args.dev
    if bus is None or devnum is None:
        found = find_device()
        if not found:
            sys.exit("Advance 25 (09e8:002f) not found on the bus.")
        bus, devnum = found
        print(f"# Advance 25 on bus {bus}, device {devnum}", file=sys.stderr)

    if args.source:
        src = open(args.source, "r", errors="replace")
    else:
        path = f"/sys/kernel/debug/usb/usbmon/{bus}u"
        if not os.path.exists(path):
            sys.exit(
                f"{path} not found. Enable usbmon first:\n"
                "  sudo modprobe usbmon\n"
                "  sudo mount -t debugfs none /sys/kernel/debug")
        try:
            src = open(path, "r", errors="replace")
        except PermissionError:
            sys.exit(f"Need root to read {path}. Re-run with sudo.")

    streams = {}
    t0 = None
    truncation_warned = False
    print(f"# watching bus {bus} device {devnum} — use the device now", file=sys.stderr)

    for line in src:
        m = LINE.match(line)
        if not m or int(m["bus"]) != bus or int(m["dev"]) != devnum:
            continue
        rest = m["rest"]
        if "=" not in rest:
            continue
        head, _, hexpart = rest.partition("=")
        # head is: <status> <length> [<extra>]
        nums = head.split()
        declared = None
        for tok in nums[1:]:
            if tok.isdigit():
                declared = int(tok)
                break
        data = bytes.fromhex("".join(hexpart.split()))
        if declared is not None and len(data) < declared and not truncation_warned:
            print(f"# WARNING: usbmon truncated data ({len(data)} of {declared} bytes). "
                  f"Long SysEx will be incomplete — the binary usbmon API is needed "
                  f"for full payloads.", file=sys.stderr)
            truncation_warned = True

        ts = int(m["ts"])
        if t0 is None:
            t0 = ts
        rel = (ts - t0) / 1e6
        key = (m["dir"], m["ep"])
        arrow = "HOST->DEV" if m["dir"] == "o" else "DEV->HOST"
        # A submit carries the data for OUT; a callback carries it for IN.
        if (m["dir"] == "o" and m["ev"] != "S") or (m["dir"] == "i" and m["ev"] != "C"):
            continue
        st = streams.setdefault(key, Stream(arrow))
        msgs = []
        st.feed(data, msgs)
        for msg in msgs:
            print(f"[{rel:9.3f}] {arrow}  {decode(msg)}", flush=True)
        if args.raw and not msgs:
            print(f"[{rel:9.3f}] {arrow}  raw {data.hex(' ')}", flush=True)


if __name__ == "__main__":
    main()
