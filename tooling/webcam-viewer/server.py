#!/usr/bin/env python3
"""
In-memory webcam viewer server for Panorama P1 debugging.

- Launches ffmpeg ONCE, piping MJPEG frames to this process's stdin (no disk
  writes, ever). A background thread parses frames (JPEG SOI/EOI markers)
  and keeps only the latest one in memory, behind a Condition so waiting
  clients wake up exactly when a new frame lands.
- Serves that latest frame to any number of simultaneous HTTP clients as a
  proper multipart/x-mixed-replace stream (GET /stream.mjpg) -- no device
  contention, no repeated open/close of the camera.
- Serves index.html fresh from disk on every request (GET /), so editing
  that file changes what's served on the next load -- combined with the
  client-side poll below, that's the hot-reload path.
- Serves GET /state.json with whatever's in STATE (edit via /set, see
  below, or just edit this dict directly and the file mtime bump forces
  a client reload). The client polls this and redraws a canvas overlay
  from it every tick, and reloads the whole page if the html's mtime
  changed since it last loaded.
- POST /set with a JSON body merges into STATE -- this is the channel for
  pushing live debug info (focus value, sharpness score, crop box, notes)
  without restarting anything.
"""
import json
import os
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DEVICE = sys.argv[1] if len(sys.argv) > 1 else "/dev/video0"
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8090
HERE = os.path.dirname(os.path.abspath(__file__))
INDEX_HTML = os.path.join(HERE, "index.html")
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ASSETS_DIR = os.path.join(REPO_ROOT, "assets")
LOGO_PNG = os.path.join(ASSETS_DIR, "logo.png")


def find_logo_txt():
    """Pick the logo_NxM.txt ASCII-art file in assets/, if any (dimensions
    are encoded in the filename since we regenerate this at different sizes
    -- rows x columns -- while iterating)."""
    try:
        candidates = sorted(f for f in os.listdir(ASSETS_DIR) if f.startswith("logo_") and f.endswith(".txt"))
    except OSError:
        return None
    return os.path.join(ASSETS_DIR, candidates[0]) if candidates else None

SOI = b"\xff\xd8"
EOI = b"\xff\xd9"

frame_lock = threading.Condition()
latest_frame = None  # bytes of the most recent complete JPEG
frame_seq = 0        # increments each time latest_frame is replaced

STATE = {"note": "no debug data pushed yet"}
state_lock = threading.Lock()


def capture_thread():
    global latest_frame, frame_seq
    cmd = [
        "ffmpeg", "-f", "v4l2", "-input_format", "yuyv422",
        "-video_size", "1280x720", "-r", "15", "-i", DEVICE,
        "-f", "mjpeg", "-q:v", "4", "-",
    ]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=0)
    buf = b""
    while True:
        chunk = proc.stdout.read(4096)
        if not chunk:
            break
        buf += chunk
        while True:
            start = buf.find(SOI)
            if start == -1:
                buf = b""
                break
            end = buf.find(EOI, start + 2)
            if end == -1:
                if start > 0:
                    buf = buf[start:]
                break
            frame = buf[start:end + 2]
            buf = buf[end + 2:]
            with frame_lock:
                latest_frame = frame
                frame_seq += 1
                frame_lock.notify_all()


def get_frame(min_seq=None, timeout=5.0):
    """Block until a frame newer than min_seq is available, then return (frame, seq)."""
    with frame_lock:
        deadline = time.time() + timeout
        while latest_frame is None or (min_seq is not None and frame_seq <= min_seq):
            remaining = deadline - time.time()
            if remaining <= 0:
                break
            frame_lock.wait(timeout=remaining)
        return latest_frame, frame_seq


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass  # quiet

    def do_GET(self):
        if self.path == "/" or self.path == "/index.html":
            self._serve_index()
        elif self.path == "/stream.mjpg":
            self._serve_stream()
        elif self.path == "/state.json":
            self._serve_state()
        elif self.path == "/logo.png":
            self._serve_file(LOGO_PNG, "image/png")
        elif self.path == "/logo.txt":
            self._serve_logo_txt()
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path == "/set":
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length) if length else b"{}"
            try:
                data = json.loads(body or b"{}")
            except json.JSONDecodeError:
                data = {}
            with state_lock:
                STATE.update(data)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"ok":true}')
        else:
            self.send_response(404)
            self.end_headers()

    def _serve_index(self):
        try:
            with open(INDEX_HTML, "rb") as f:
                body = f.read()
        except FileNotFoundError:
            body = b"<h1>index.html not found</h1>"
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _serve_file(self, path, content_type):
        try:
            with open(path, "rb") as f:
                body = f.read()
        except FileNotFoundError:
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _serve_logo_txt(self):
        path = find_logo_txt()
        if path is None:
            self.send_response(404)
            self.end_headers()
            return
        self._serve_file(path, "text/plain; charset=utf-8")

    def _serve_state(self):
        with state_lock:
            payload = dict(STATE)
        try:
            payload["_html_mtime"] = os.path.getmtime(INDEX_HTML)
        except OSError:
            payload["_html_mtime"] = 0
        body = json.dumps(payload).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _serve_stream(self):
        boundary = "frame"
        self.send_response(200)
        self.send_header("Age", "0")
        self.send_header("Cache-Control", "no-cache, private")
        self.send_header("Pragma", "no-cache")
        self.send_header("Content-Type", f"multipart/x-mixed-replace; boundary={boundary}")
        self.end_headers()
        seq = None
        try:
            while True:
                frame, seq = get_frame(min_seq=seq, timeout=10.0)
                if frame is None:
                    continue
                self.wfile.write(f"--{boundary}\r\n".encode())
                self.wfile.write(b"Content-Type: image/jpeg\r\n")
                self.wfile.write(f"Content-Length: {len(frame)}\r\n\r\n".encode())
                self.wfile.write(frame)
                self.wfile.write(b"\r\n")
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    t = threading.Thread(target=capture_thread, daemon=True)
    t.start()
    server = ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    print(f"Serving on http://0.0.0.0:{PORT}/  (device={DEVICE})")
    server.serve_forever()


if __name__ == "__main__":
    main()
