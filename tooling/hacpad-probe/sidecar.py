#!/usr/bin/env python3
"""Host-side sidecar for the hacpad probe VST.

The plugin (running under Wine inside VIP) connects OUT to this listener and
speaks a line protocol. This end drives it and logs what comes back, so a probe
session is a controlled experiment: we set a parameter name from here, and watch
for those bytes on the Advance's wire (via tooling/firmware/spy.py or an ALSA
capture) and on the panel (via the webcam rig).

The plugin is the client and this is the server on purpose: a listening socket
inside a Wine prefix is exactly the kind of thing that quietly fails, and a
stable listener here means plugin instances can come and go with the host's
plugin scanning without anything needing to track them.

Protocol (newline-delimited UTF-8, deliberately hand-typeable with netcat):

  plugin -> us          us -> plugin
  ------------          ------------
  hello <v> <n> <name>  describe            (ask for a fresh announce)
  named <i> <name>      name <i> <string>   (rename one parameter)
  valued <i> <v>        names <prefix>      (rename all: prefix0000, prefix0001 ...)
  midi <hex>            value <i> <0..1>    (move a parameter)
  dropped <n>           tap <0|1>           (start/stop MIDI capture)
  pong                  ping

Usage:
  python3 sidecar.py                 # listen, then drop into a REPL
  python3 sidecar.py --port 8131
"""
import argparse
import socket
import sys
import threading
import time

PORT = 8131


class Peer:
    def __init__(self, conn, addr, log):
        self.conn = conn
        self.addr = addr
        self.log = log
        self.buf = b""
        self.alive = True
        self.params = {}          # index -> current name, as the plugin reports it

    def send(self, line):
        try:
            self.conn.sendall((line + "\n").encode("utf-8"))
        except OSError as e:
            self.log(f"! send failed: {e}")
            self.alive = False

    def reader(self):
        while self.alive:
            try:
                chunk = self.conn.recv(4096)
            except OSError:
                break
            if not chunk:
                break
            self.buf += chunk
            while b"\n" in self.buf:
                raw, self.buf = self.buf.split(b"\n", 1)
                self.handle(raw.decode("utf-8", "replace").strip())
        self.alive = False
        self.log(f"# {self.addr} disconnected")

    def handle(self, line):
        if not line:
            return
        cmd, _, rest = line.partition(" ")
        ts = time.strftime("%H:%M:%S")
        if cmd == "named":
            idx, _, name = rest.partition(" ")
            self.params[idx] = name
            self.log(f"[{ts}] param {idx:>2} = {name!r}")
        elif cmd == "midi":
            self.log(f"[{ts}] MIDI {rest}")
        elif cmd == "hello":
            self.log(f"[{ts}] plugin: {line}")
        elif cmd == "dropped":
            self.log(f"[{ts}] ! plugin dropped {rest} MIDI messages (buffer full)")
        else:
            self.log(f"[{ts}] {line}")


HELP = """\
commands (typed here, sent to the plugin):
  names <prefix>     rename every parameter to <prefix>NNNN  (find the name field)
  name <i> <text>    rename one parameter
  value <i> <f>      move parameter i to 0..1
  tap <0|1>          start/stop the plugin echoing incoming MIDI
  ping / describe    liveness / re-request the parameter list
  params             show what the plugin last reported (local)
  help / quit
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=PORT)
    ap.add_argument("--host", default="127.0.0.1")
    args = ap.parse_args()

    lock = threading.Lock()

    def log(msg):
        with lock:
            sys.stdout.write("\r\033[K" + msg + "\n> ")
            sys.stdout.flush()

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((args.host, args.port))
    srv.listen(1)
    print(f"# listening on {args.host}:{args.port} — start the plugin (or nc) now")

    peer_box = {"peer": None}

    def accept_loop():
        while True:
            conn, addr = srv.accept()
            p = Peer(conn, addr, log)
            peer_box["peer"] = p
            log(f"# connected: {addr}")
            p.send("describe")
            threading.Thread(target=p.reader, daemon=True).start()

    threading.Thread(target=accept_loop, daemon=True).start()

    print(HELP)
    try:
        while True:
            line = input("> ").strip()
            if not line:
                continue
            if line in ("quit", "exit"):
                break
            if line == "help":
                print(HELP)
                continue
            p = peer_box["peer"]
            if line == "params":
                if p:
                    for i in sorted(p.params, key=lambda x: int(x)):
                        print(f"  {i:>2} = {p.params[i]!r}")
                else:
                    print("  (no plugin connected)")
                continue
            if not p or not p.alive:
                print("  (no plugin connected)")
                continue
            p.send(line)
    except (EOFError, KeyboardInterrupt):
        pass
    print("\n# bye")


if __name__ == "__main__":
    main()
