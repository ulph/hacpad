#!/usr/bin/env python3
"""Minimal cpio 'odc' (070707) reader for macOS .pkg payloads.

Header is 76 bytes of octal ASCII, then the NUL-terminated name, then the data.
No padding. Usage:
    cpio.py <payload.gz> list [substr]
    cpio.py <payload.gz> extract <outdir> [substr]
"""
import gzip
import os
import sys

FIELDS = [("magic", 6), ("dev", 6), ("ino", 6), ("mode", 6), ("uid", 6),
          ("gid", 6), ("nlink", 6), ("rdev", 6), ("mtime", 11),
          ("namesize", 6), ("filesize", 11)]


def entries(f):
    while True:
        hdr = f.read(76)
        if len(hdr) < 76 or not hdr.startswith(b"070707"):
            return
        vals, off = {}, 0
        for name, width in FIELDS:
            vals[name] = int(hdr[off:off + width], 8)
            off += width
        name = f.read(vals["namesize"])[:-1].decode("utf-8", "replace")
        size = vals["filesize"]
        if name == "TRAILER!!!":
            return
        yield name, vals["mode"], size, f


def main():
    path, mode = sys.argv[1], sys.argv[2]
    f = gzip.open(path, "rb")

    if mode == "list":
        want = sys.argv[3] if len(sys.argv) > 3 else None
        n = 0
        for name, m, size, fh in entries(f):
            data_skipped = False
            if want is None or want.lower() in name.lower():
                kind = "d" if (m & 0o170000) == 0o040000 else \
                       "l" if (m & 0o170000) == 0o120000 else "-"
                print(f"{kind} {size:>11}  {name}")
                n += 1
            fh.read(size)
        print(f"\n{n} matching entries", file=sys.stderr)
        return

    outdir = sys.argv[3]
    want = sys.argv[4] if len(sys.argv) > 4 else None
    for name, m, size, fh in entries(f):
        if want and want.lower() not in name.lower():
            fh.read(size)
            continue
        data = fh.read(size)
        if (m & 0o170000) == 0o040000:
            continue
        dest = os.path.join(outdir, name.lstrip("./"))
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        if (m & 0o170000) == 0o120000:
            continue  # skip symlinks
        with open(dest, "wb") as o:
            o.write(data)
        print(f"  {size:>11}  {name}")


if __name__ == "__main__":
    main()
