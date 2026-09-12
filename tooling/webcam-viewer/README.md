# webcam-viewer

Small dev tool for watching a USB webcam (e.g. pointed at the Panorama P1's screen)
live in a browser while iterating on the bridge, with a debug overlay.

- `server.py` — launches `ffmpeg` **once**, piping MJPEG frames straight to memory
  (never touches disk). Serves the latest frame to any number of browser tabs as
  a `multipart/x-mixed-replace` stream, multi-client, no device contention.
- `index.html` — the page: video feed + a canvas overlay drawn from whatever's
  currently in server-side state. Polls `/state.json` every 700ms; if `index.html`'s
  own mtime changed since load, the page reloads itself automatically — edit this
  file while the server's running and the browser picks it up without a manual
  refresh.

## Run

```
python3 server.py [/dev/videoN] [port]   # defaults: /dev/video0, 8090
```

Then open `http://localhost:<port>/`.

## Push debug data

```
curl -X POST http://localhost:8090/set -d '{"note": "testing", "focus_absolute": 12, "crop_box": [140, 60, 450, 430]}'
```

`crop_box` is `[x, y, w, h]` in the source frame's coordinates (1280x720 by default)
and gets drawn as a green rectangle on the overlay. Add more fields to `STATE` and
extend `draw()` in `index.html` as new debug needs come up — no restart required for
the client-side change, since it hot-reloads.

## Why this design

Earlier attempts wrote JPEG stills to disk repeatedly and reopened the camera device
per capture — this caused real device wedging (the webcam dropped off the USB bus
entirely after repeated hard-kills mid-I/O). This version opens the device exactly
once and keeps the frame only in memory, which avoids both problems.
