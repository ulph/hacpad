# webcam-viewer

Dev tool combining two independent things on one page:

1. A live USB webcam feed (e.g. pointed at the Panorama P1's screen), for visually
   confirming what a SysEx write actually did against real hardware.
2. An interactive **screen + LED simulator** for the P1, talking directly to
   `prototypes/panorama-p1/src/bin/service.rs` over its own WebSocket (port 8091) --
   `server.py` has no involvement in the simulator at all, it only serves the webcam
   feed and this static page.

- `server.py` — launches `ffmpeg` **once**, piping MJPEG frames straight to memory
  (never touches disk), served to any number of browser tabs as a
  `multipart/x-mixed-replace` stream. Also hot-reloads `index.html`: the page polls
  `/state.json` every 700ms and reloads itself if the file's mtime changed, so edits
  here show up without a manual refresh.
- `index.html` — the feed plus the simulator UI.
- `../../prototypes/panorama-p1/src/bin/service.rs` — the one process that actually
  holds the MIDI connection to the device and applies whatever the simulator sends.

## Run

```
python3 server.py [/dev/videoN] [port]        # defaults: /dev/video0, 8090
cargo run --bin service                       # from prototypes/panorama-p1, separately
```

Then open `http://localhost:<port>/`.

## Simulator requirements (durable checklist -- keep this in sync with index.html)

These came up repeatedly across the session that built the simulator; recorded here
so future changes can be checked against them instead of conversation history.

- Every `displayId` (0 pad_state, 1 title_bar, 2 big_font, 3 current_value, 4
  menu_button, 5 page_labels, 6 ctrl_element_name, 7 ctrl_element_value, 8
  page_menu/popup) is its own independently-addressed field in the UI, plus the
  pageTemplate=1 `message` one-shot overlay. None of them are gated by which
  `page_template` is selected -- the template only chooses which mock widget draws
  the content fields (see "Thirty-second finding" in the protocol notes for the
  hardware-verified mechanics: a template switch is carried on *some* field's write,
  clears per-template content as a side effect, and chrome fields survive untouched).
- The `page_template` selector lists all confirmed templates (0-9, 16-22), and is the
  first control on the page.
- Comma-separated fields are exhaustive to their real slot count: names/values 16,
  page_labels 3, tabs 5, menu items 8, pad_state 16 -- not a truncated subset.
- Field defaults are descriptive of the field itself (`N1`..`N16`, `V1`..`V16`,
  `TB1`-`TB3`, `Tab1`-`Tab5`, `PL1`-`PL3`, `Item1`-`Item8`, `BIGFONT`, `PARAMVAL`),
  never arbitrary demo content -- except `message`, which defaults to `"hacpad"`, the
  established resting-baseline text.
- Field defaults live in `service.rs`'s `default_state()` (applied to the real device
  at boot, handed to the first connecting client), **not** hardcoded in index.html --
  the WebSocket state is the single bidirectional source of truth, same as any other
  client's edit.
- An LED panel covers every confirmed on/off LED CC (`LED_CCS` in lib.rs) with a
  visible on/off dot per LED -- color is a placeholder (no RGB evidence on this
  hardware, on/off only is confirmed).
- Labels are terse (`displayId N -- name`) -- full rationale/evidence lives in
  `research/panorama-p1-protocol-notes.md` and `protocol.json`, not duplicated here.
- The diffing push logic is visible: a "last sent" line shows exactly which field
  names just went out, so "did my edit actually send" is never a guess.
- Switching `page_template` re-supplies whatever content this tab currently holds
  for the fields the real device just cleared as a side effect (names/values/
  padState/pageLabels) -- otherwise the mockup silently drifts from the real screen.
- The very first state sync of a fresh WebSocket connection always applies,
  regardless of the "don't stomp an in-progress edit" focus guard -- otherwise a
  dropped initial sync leaves every field blank with nothing to fall back to.

## Push debug data (webcam overlay only, unrelated to the simulator)

```
curl -X POST http://localhost:8090/set -d '{"note": "testing", "focus_absolute": 12, "crop_box": [140, 60, 450, 430]}'
```

`crop_box` is `[x, y, w, h]` in the source frame's coordinates (1280x720 by default)
and gets drawn as a green rectangle on the overlay. Add more fields to `STATE` and
extend `draw()` in `index.html` as new debug needs come up -- no restart required for
the client-side change, since it hot-reloads.

## Why this design

Earlier attempts wrote JPEG stills to disk repeatedly and reopened the camera device
per capture -- this caused real device wedging (the webcam dropped off the USB bus
entirely after repeated hard-kills mid-I/O). This version opens the device exactly
once and keeps the frame only in memory, which avoids both problems.
