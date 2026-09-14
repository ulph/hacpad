#!/usr/bin/env python3
"""Extract the raw disk image out of an Apple UDIF (.dmg).

No 7z/dmg2img on this box, so parse it directly: the koly trailer points at an
XML plist whose resource-fork/blkx array holds base64 'mish' block maps, and each
mish lists chunks (raw / zero / zlib / bzip2) to reassemble in sector order.
"""
import base64
import bz2
import plistlib
import struct
import sys
import zlib

RAW, IGNORE, COMMENT, ZLIB, BZ2, LZFSE, LZMA, TERM = (
    0x00000001, 0x00000002, 0x7FFFFFFE,
    0x80000005, 0x80000006, 0x80000007, 0x80000008, 0xFFFFFFFF,
)
NAMES = {RAW: "raw", IGNORE: "zero", COMMENT: "comment", ZLIB: "zlib",
         BZ2: "bzip2", LZFSE: "lzfse", LZMA: "lzma", TERM: "term"}
SECTOR = 512


def extract(path, out):
    d = open(path, "rb").read()
    k = d.rfind(b"koly")
    if k < 0:
        sys.exit("no koly trailer — not a UDIF image")
    t = d[k:k + 512]
    xml_off, xml_len = struct.unpack(">QQ", t[216:232])
    pl = plistlib.loads(d[xml_off:xml_off + xml_len])

    blkx = pl["resource-fork"]["blkx"]
    print(f"{len(blkx)} blkx entries")

    seen = {}
    total = 0
    with open(out, "wb") as f:
        for entry in blkx:
            mish = base64.b64decode(entry["Data"]) if isinstance(entry["Data"], str) else entry["Data"]
            if mish[:4] != b"mish":
                continue
            start_sector, = struct.unpack(">Q", mish[8:16])
            n_desc, = struct.unpack(">I", mish[200:204])
            off = 204
            for _ in range(n_desc):
                (etype, _c, sec_no, sec_cnt, c_off, c_len) = struct.unpack(
                    ">IIQQQQ", mish[off:off + 40])
                off += 40
                if etype == TERM:
                    break
                seen[NAMES.get(etype, hex(etype))] = seen.get(NAMES.get(etype, hex(etype)), 0) + 1
                pos = (start_sector + sec_no) * SECTOR
                size = sec_cnt * SECTOR
                if etype == IGNORE or etype == COMMENT:
                    data = b"\0" * size
                elif etype == RAW:
                    data = d[c_off:c_off + c_len]
                elif etype == ZLIB:
                    data = zlib.decompress(d[c_off:c_off + c_len])
                elif etype == BZ2:
                    data = bz2.decompress(d[c_off:c_off + c_len])
                else:
                    print(f"  !! unsupported chunk {NAMES.get(etype, hex(etype))}, zero-filling")
                    data = b"\0" * size
                if etype == COMMENT:
                    continue
                f.seek(pos)
                f.write(data)
                total += len(data)
    print("chunk types:", seen)
    print(f"wrote {out} ({total} bytes of payload)")


if __name__ == "__main__":
    extract(sys.argv[1], sys.argv[2])
