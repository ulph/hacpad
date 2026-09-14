#!/usr/bin/env python3
"""Advance 25 viewer server — same shape as the Panorama P1 webcam viewer.

The single camera is already served as MJPEG by the P1 webcam server
(tooling/webcam-viewer/server.py) on port 8090; only one process can own
/dev/video0, so this server does NOT open the camera. Instead it *proxies* that
MJPEG stream at /stream.mjpg (same-origin for the page) and serves an Advance-
specific index.html (landscape, no rotation, unlike the P1 page).

    python3 serve.py [http_port] [camera_port]
    defaults: 8096, 8090
"""
import http.server
import os
import socketserver
import sys
import urllib.request

HTTP_PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8096
CAM_PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8090
HERE = os.path.dirname(os.path.abspath(__file__))


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        if self.path in ("/", "/index.html"):
            self._serve_index()
        elif self.path == "/stream.mjpg":
            self._proxy_stream()
        else:
            self.send_error(404)

    def _serve_index(self):
        with open(os.path.join(HERE, "index.html"), "rb") as f:
            body = f.read()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _proxy_stream(self):
        try:
            up = urllib.request.urlopen(f"http://localhost:{CAM_PORT}/stream.mjpg", timeout=5)
        except OSError as e:
            self.send_error(502, f"camera feed on :{CAM_PORT} unreachable: {e}")
            return
        self.send_response(200)
        self.send_header("Content-Type", up.headers.get("Content-Type", "multipart/x-mixed-replace; boundary=frame"))
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        try:
            while True:
                chunk = up.read(8192)
                if not chunk:
                    break
                self.wfile.write(chunk)
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            up.close()


class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


if __name__ == "__main__":
    print(f"Advance viewer on http://localhost:{HTTP_PORT}  (proxying camera :{CAM_PORT})")
    Server(("0.0.0.0", HTTP_PORT), Handler).serve_forever()
