#!/usr/bin/env python3
"""Minimal XAR (.pkg) reader — list or extract, no macOS tooling required.

Header is 28 bytes, then a zlib-compressed XML table of contents, then the heap.
Each file entry carries its heap offset, stored length and encoding.
"""
import struct
import sys
import zlib
import os
import xml.etree.ElementTree as ET


def open_xar(path):
    f = open(path, "rb")
    hdr = f.read(28)
    magic, hdr_sz, ver, toc_c, toc_u, cksum = struct.unpack(">4sHHQQI", hdr)
    if magic != b"xar!":
        sys.exit("not a XAR archive")
    f.seek(hdr_sz)
    toc = zlib.decompress(f.read(toc_c))
    heap = hdr_sz + toc_c
    return f, ET.fromstring(toc), heap


def walk(node, prefix=""):
    for fe in node.findall("file"):
        name = fe.findtext("name", "")
        path = f"{prefix}/{name}" if prefix else name
        ftype = fe.findtext("type", "")
        data = fe.find("data")
        if data is not None:
            off = int(data.findtext("offset", "0"))
            ln = int(data.findtext("length", "0"))
            sz = int(data.findtext("size", "0"))
            enc = data.find("encoding")
            style = enc.get("style") if enc is not None else ""
            yield path, ftype, off, ln, sz, style
        else:
            yield path, ftype, 0, 0, 0, ""
        yield from walk(fe, path)


def main():
    path, mode = sys.argv[1], sys.argv[2]
    f, toc, heap = open_xar(path)
    entries = list(walk(toc.find("toc")))

    if mode == "list":
        for p, t, off, ln, sz, style in entries:
            if t == "file":
                print(f"{sz:>12}  {style or '-':<28} {p}")
            else:
                print(f"{'':>12}  {'[' + t + ']':<28} {p}")
        return

    outdir = sys.argv[3]
    want = sys.argv[4] if len(sys.argv) > 4 else None
    for p, t, off, ln, sz, style in entries:
        if t != "file" or ln == 0:
            continue
        if want and want not in p:
            continue
        f.seek(heap + off)
        blob = f.read(ln)
        if "gzip" in (style or ""):
            try:
                blob = zlib.decompress(blob)
            except zlib.error:
                blob = zlib.decompress(blob, 47)  # gzip wrapper
        dest = os.path.join(outdir, p)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        with open(dest, "wb") as o:
            o.write(blob)
        print(f"  {len(blob):>12}  {p}")


if __name__ == "__main__":
    main()
