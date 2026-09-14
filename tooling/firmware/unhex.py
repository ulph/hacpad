#!/usr/bin/env python3
"""Pull the SysEx-wrapped Intel HEX firmware images out of the updater image.

Akai ships firmware as `F0 47 <dev> <model> 0x70 <intel-hex as ASCII> F7`.
Decode each blob to a flat binary plus its load address.
"""
import re
import sys
from collections import OrderedDict

src = sys.argv[1]
d = open(src, "rb").read()

blobs = []
for m in re.finditer(rb"\xf0\x47(.)(.)\x70", d):
    start = m.start()
    end = d.find(b"\xf7", start)
    if end < 0:
        continue
    blobs.append((start, end, m.group(1)[0], m.group(2)[0], d[start + 5:end]))

print(f"{len(blobs)} SysEx firmware blobs\n")

seen = {}
for start, end, dev, model, body in blobs:
    mem = {}
    base = 0
    bad = 0
    for line in body.split(b"\r\n"):
        line = line.strip()
        if not line.startswith(b":"):
            continue
        try:
            raw = bytes.fromhex(line[1:].decode("ascii"))
        except ValueError:
            bad += 1
            continue
        n, addr_hi, addr_lo, rtype = raw[0], raw[1], raw[2], raw[3]
        data = raw[4:4 + n]
        if (sum(raw[:4 + n]) + raw[4 + n]) & 0xFF:
            bad += 1
            continue
        if rtype == 0x00:
            a = base + (addr_hi << 8) + addr_lo
            for i, b in enumerate(data):
                mem[a + i] = b
        elif rtype == 0x04:
            base = int.from_bytes(data, "big") << 16
        elif rtype == 0x05:
            pass  # start linear address
        elif rtype == 0x01:
            break

    if not mem:
        continue
    lo, hi = min(mem), max(mem)
    size = hi - lo + 1
    out = bytearray(b"\xff" * size)
    for a, b in mem.items():
        out[a - lo] = b

    key = (lo, bytes(out))
    tag = "dup" if key in seen else "new"
    if key not in seen:
        name = f"fw_{lo:08x}.bin"
        open(name, "wb").write(out)
        seen[key] = name
    print(f"@{start:#010x} model={model:#04x} dev={dev:#04x}  "
          f"load={lo:#010x}..{hi:#010x}  {size:>8} bytes  "
          f"gaps={size - len(mem):>7}  badrec={bad}  [{tag} {seen[key]}]")

print("\nwrote:", ", ".join(sorted(set(seen.values()))))
