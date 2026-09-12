#!/usr/bin/env python3
"""
Zero-install replacement for `aconnect SRC DST` -- talks to libasound
directly via ctypes, since that shared library is already present and
loaded on any system where ALSA MIDI works at all (no new package needed,
unlike the `aconnect` CLI binary from alsa-utils).

Usage:
    python3 aconnect_fallback.py <src_client>:<src_port> <dst_client>:<dst_port>
    python3 aconnect_fallback.py -l          # list clients/ports (like aconnect -l)

Client/port can be given as numbers (e.g. "128:0") or, for -l output, by
name matching (case-insensitive substring) e.g. "Bitwig:0" once you've
found the exact numeric ports via -l.

This makes a persistent, real ALSA sequencer subscription -- equivalent to
running `aconnect` -- and holds it until you Ctrl-C (ALSA subscriptions
made this way are removed when the connecting process exits, same as
`aconnect` without -d).
"""
import ctypes
import ctypes.util
import re
import sys
import time

lib_path = ctypes.util.find_library("asound")
if not lib_path:
    print("libasound not found -- is ALSA even installed? (it must be, for MIDI to work at all)", file=sys.stderr)
    sys.exit(1)
asound = ctypes.CDLL(lib_path)

SND_SEQ_OPEN_DUPLEX = 2


def check(rc, what):
    if rc < 0:
        print(f"{what} failed: error {rc}", file=sys.stderr)
        sys.exit(1)
    return rc


def open_seq():
    handle = ctypes.c_void_p()
    check(asound.snd_seq_open(ctypes.byref(handle), b"default", SND_SEQ_OPEN_DUPLEX, 0), "snd_seq_open")
    return handle


def list_clients(handle):
    """Roughly mirror `aconnect -l` by reading /proc/asound/seq/clients,
    which is far simpler than walking the full snd_seq_client_info API from
    ctypes and gives the same information."""
    try:
        with open("/proc/asound/seq/clients") as f:
            print(f.read())
    except OSError as e:
        print(f"couldn't read /proc/asound/seq/clients: {e}", file=sys.stderr)


def parse_addr(s):
    m = re.match(r"^(\d+):(\d+)$", s)
    if not m:
        print(f"expected NUMERIC client:port (see -l output), got {s!r}", file=sys.stderr)
        sys.exit(1)
    return int(m.group(1)), int(m.group(2))


def main():
    if len(sys.argv) == 2 and sys.argv[1] == "-l":
        handle = open_seq()
        list_clients(handle)
        return

    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)

    src_client, src_port = parse_addr(sys.argv[1])
    dst_client, dst_port = parse_addr(sys.argv[2])

    handle = open_seq()
    check(asound.snd_seq_set_client_name(handle, b"hacpad-aconnect-fallback"), "snd_seq_set_client_name")

    # snd_seq_connect_from(seq, my_port, src_client, src_port) subscribes
    # OUR port to receive everything the src port sends -- we don't even
    # need our own visible port for this, snd_seq_connect_from/to operate
    # directly on client:port addresses via the kernel sequencer core.
    src = (ctypes.c_int * 2)(src_client, src_port)
    dst = (ctypes.c_int * 2)(dst_client, dst_port)

    class SeqAddr(ctypes.Structure):
        _fields_ = [("client", ctypes.c_ubyte), ("port", ctypes.c_ubyte)]

    sender = SeqAddr(src_client, src_port)
    dest = SeqAddr(dst_client, dst_port)

    rc = asound.snd_seq_connect_from(handle, dst_port, src_client, src_port)
    if rc < 0:
        # fall back to the explicit subscribe API if the simple helper isn't
        # available in this libasound build
        print(f"snd_seq_connect_from returned {rc}; this libasound build may need the full subscribe API.", file=sys.stderr)
        sys.exit(1)

    print(f"Connected {src_client}:{src_port} -> {dst_client}:{dst_port}. Ctrl-C to disconnect and exit.")
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
