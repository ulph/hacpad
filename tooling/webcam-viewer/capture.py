#!/usr/bin/env python3
"""THE one canonical way to capture a verified-legible photo of the Panorama
P1's screen. Supersedes every one-off inline capture/calibration snippet
written ad hoc during testing -- do not write another throwaway script for
this, use this one (or extend it).

Why this exists: ambient lighting drifts in real time (confirmed directly --
the same exposure_time_absolute value that was clipping-free at one point
was clipped 10 minutes later, nothing else changed), and this device's LCD
has a genuinely narrow, non-uniform exposure window across its own surface
(one region can be legible while another is already sensor-clipped at the
very same exposure_time -- confirmed by direct per-region pixel sampling,
not assumed). So there is no fixed "good" setting to remember; every capture
that matters should re-calibrate against the CURRENT moment, not trust a
value from even a few minutes ago.

What it does, every time you call `capture()`/run this as a script:
  1. Binary-searches `exposure_time_absolute` against actual per-pixel
     clipping in the screen ROI (not a lenient whole-frame histogram
     fraction, which was tried first and missed a locally-clipped region
     while the overall frame looked fine) -- narrows until no sampled point
     in the ROI is clipped white AND the frame isn't globally black.
  2. Saves the final frame and reports whether it converged.

Usage as a script:
    python3 capture.py out.jpg [--device /dev/video0] [--port 8090]

Usage as a library:
    from capture import capture
    img, ok, exposure = capture()   # img is a PIL.Image, already calibrated
"""
import argparse
import io
import subprocess
import sys
import urllib.request

from PIL import Image

# The P1's screen in the ROTATED (post `.rotate(90, expand=True)`) frame.
# Re-measure if the camera is ever physically repositioned -- these are
# pixel coordinates in a ~720-wide frame, not fractions.
SCREEN_BOX = (30, 510, 480, 910)

# Sample points spread across the screen (title bar, content rows, tabs) --
# checking several fixed points is what catches a LOCALIZED clip that a
# whole-ROI histogram average would miss (confirmed: one region clipped
# solid while the histogram fraction for the whole ROI still looked fine).
SAMPLE_POINTS = [
    (160, 605),  # title bar area
    (160, 645),  # content row 1
    (160, 695),  # content row 2
    (160, 738),  # content row 3
    (160, 780),  # content row 4
    (160, 830),  # tabs area
]

CLIP_WHITE = 250   # a channel at/above this is treated as sensor-clipped
CLIP_BLACK = 4     # a channel at/below this is treated as crushed black
MAX_ITERATIONS = 12


def _run(args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout


def get_ctrl(device, name):
    out = _run(["v4l2-ctl", "-d", device, "--get-ctrl", name])
    return int(out.strip().split(":")[-1].strip())


def set_ctrl(device, **kwargs):
    pairs = ",".join(f"{k}={v}" for k, v in kwargs.items())
    _run(["v4l2-ctl", "-d", device, f"--set-ctrl={pairs}"])


def snap(port):
    req = urllib.request.urlopen(f"http://localhost:{port}/stream.mjpg", timeout=3)
    data = b""
    while data.count(b"--frame") < 2:
        chunk = req.read(4096)
        if not chunk:
            break
        data += chunk
        if len(data) > 2_000_000:
            break
    req.close()
    s = data.find(b"\xff\xd8")
    e = data.rfind(b"\xff\xd9")
    return Image.open(io.BytesIO(data[s : e + 2])).rotate(90, expand=True)


def _sample_status(img):
    """Returns (any_clipped_white, all_crushed_black) across SAMPLE_POINTS."""
    any_white = False
    all_black = True
    for pt in SAMPLE_POINTS:
        r, g, b = img.getpixel(pt)[:3]
        if max(r, g, b) >= CLIP_WHITE:
            any_white = True
        if max(r, g, b) > CLIP_BLACK:
            all_black = False
    return any_white, all_black


def capture(device="/dev/video0", port=8090, verbose=True):
    """Re-calibrates exposure against the CURRENT moment (never trusts a
    remembered value -- see module docstring) and returns
    (PIL.Image, converged: bool, final_exposure_time: int)."""
    lo, hi = 1, 300  # exposure_time_absolute search bounds
    exposure = get_ctrl(device, "exposure_time_absolute")
    exposure = max(lo, min(hi, exposure))  # start from wherever it currently is

    img = None
    for i in range(MAX_ITERATIONS):
        set_ctrl(device, exposure_time_absolute=exposure)
        img = snap(port)
        clipped, crushed = _sample_status(img)
        if verbose:
            print(f"iter {i}: exposure_time={exposure} clipped={clipped} crushed={crushed}")

        if clipped and not crushed:
            hi = exposure
            exposure = (lo + exposure) // 2
        elif crushed and not clipped:
            lo = exposure
            exposure = (exposure + hi) // 2
        elif not clipped and not crushed:
            if verbose:
                print(f"Converged: exposure_time={exposure}")
            return img, True, exposure
        else:
            # both clipped AND crushed somewhere -- genuinely no good window
            # right now (matches the confirmed "narrow/shifting window,
            # possibly localized glare" behavior). Split the difference and
            # keep trying a few more times in case it's transient.
            exposure = (lo + hi) // 2

        if hi - lo <= 1:
            break

    if verbose:
        print(f"Did NOT converge after {MAX_ITERATIONS} iterations (last exposure_time={exposure}). "
              "This can be a genuine dynamic-range limit, not a wrong setting: the pad-grid "
              "widget itself renders a vertical brightness gradient by design (darkest row "
              "at top, near-white at bottom -- \"Thirty-second finding\"), so a full grid can "
              "span a wider luminance range than one exposure can hold at all. Use "
              "capture_bracket() instead if you need every row legible in one pass.")
    return img, False, exposure


def capture_bracket(device="/dev/video0", port=8090, exposures=(15, 40, 90), verbose=True):
    """For scenes with more dynamic range than one exposure can hold (e.g. the
    pad grid's confirmed vertical gradient) -- takes one photo per exposure in
    `exposures` and returns the list, spanning dark-detail to bright-detail,
    instead of pretending a single "correct" exposure exists. Caller picks
    (or visually compares) whichever frame has the specific row/region they
    need legible; there is no single merged image, no HDR blending -- just
    the raw set, which is enough for "does this region show X" verification.
    """
    images = []
    for exp in exposures:
        set_ctrl(device, exposure_time_absolute=exp)
        img = snap(port)
        images.append((exp, img))
        if verbose:
            print(f"bracket: exposure_time={exp} captured")
    return images


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("out", nargs="?", default=None, help="path to save the photo")
    p.add_argument("--device", default="/dev/video0")
    p.add_argument("--port", type=int, default=8090)
    args = p.parse_args()

    img, ok, exposure = capture(args.device, args.port)
    if args.out:
        img.save(args.out)
        print(f"saved to {args.out}")
    sys.exit(0 if ok else 1)
