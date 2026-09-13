#!/usr/bin/env python3
"""Bidirectional live camera-tuning WebSocket server.

Why this exists: exposure/contrast/focus for this webcam had to be
hand-tuned repeatedly this session via one-off shell commands, guessing at
values -- slow, and the person who can actually SEE the result (looking at
the physical device, not a photo) had no direct way to adjust it themselves.
This exposes every relevant v4l2 control as a live slider, same
state-broadcast pattern as prototypes/panorama-p1/src/bin/service.rs: any
connected client's change applies immediately and is broadcast to every
other connected client, so multiple tabs stay in sync.

Protocol (client -> server): one JSON object per message, any subset of the
controls in CONTROLS below, e.g. {"exposure_time_absolute": 50}. Only the
keys present are changed. Server -> client: the full current value of every
control, sent to a newly-connected client immediately and re-broadcast to
everyone after any change (including the sender, for confirmation).

Usage:
    python3 camera_control_server.py [--device /dev/video0] [--port 8092]
"""
import argparse
import asyncio
import json
import subprocess

import websockets

# name -> (min, max) shown to the client as slider bounds; kept in sync by
# hand with what /dev/video0 actually reports (see v4l2-ctl --list-ctrls).
CONTROLS = {
    "brightness": (30, 255),
    "contrast": (0, 10),
    "saturation": (0, 200),
    "sharpness": (0, 50),
    "backlight_compensation": (0, 10),
    "white_balance_automatic": (0, 1),
    "white_balance_temperature": (2500, 10000),
    "auto_exposure": (0, 3),
    "exposure_time_absolute": (1, 10000),
    "focus_automatic_continuous": (0, 1),
    "focus_absolute": (0, 40),
}

DEVICE = "/dev/video0"
CLIENTS = set()


def get_ctrl(name):
    out = subprocess.run(
        ["v4l2-ctl", "-d", DEVICE, "--get-ctrl", name],
        capture_output=True, text=True, check=True,
    ).stdout
    # menu-type controls (e.g. auto_exposure) print "1 (Manual Mode)" -- take
    # only the leading integer, not the parenthetical label.
    value_part = out.strip().split(":")[-1].strip()
    return int(value_part.split()[0])


def set_ctrl(name, value):
    subprocess.run(
        ["v4l2-ctl", "-d", DEVICE, f"--set-ctrl={name}={value}"],
        capture_output=True, text=True, check=True,
    )


def current_state():
    state = {}
    for name in CONTROLS:
        try:
            state[name] = get_ctrl(name)
        except subprocess.CalledProcessError as e:
            print(f"warning: couldn't read {name}: {e.stderr.strip()}")
    return state


async def broadcast(state, exclude=None):
    if not CLIENTS:
        return
    msg = json.dumps(state)
    for ws in list(CLIENTS):
        if ws is exclude:
            continue
        try:
            await ws.send(msg)
        except websockets.exceptions.ConnectionClosed:
            CLIENTS.discard(ws)


async def handle_client(ws):
    CLIENTS.add(ws)
    print(f"camera-control client connected ({len(CLIENTS)} total)")
    try:
        await ws.send(json.dumps(current_state()))
        async for raw in ws:
            try:
                update = json.loads(raw)
            except json.JSONDecodeError:
                continue
            applied = {}
            for name, value in update.items():
                if name not in CONTROLS:
                    continue
                try:
                    set_ctrl(name, int(value))
                    applied[name] = get_ctrl(name)  # read back the real applied value
                except (subprocess.CalledProcessError, ValueError) as e:
                    print(f"failed to set {name}={value}: {e}")
            if applied:
                await broadcast(applied)  # to everyone, sender included, as confirmation
    finally:
        CLIENTS.discard(ws)
        print(f"camera-control client disconnected ({len(CLIENTS)} total)")


async def main(port):
    print(f"Camera control server on ws://0.0.0.0:{port}, device={DEVICE}")
    async with websockets.serve(handle_client, "0.0.0.0", port):
        await asyncio.Future()  # run forever


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--device", default="/dev/video0")
    p.add_argument("--port", type=int, default=8092)
    args = p.parse_args()
    DEVICE = args.device
    asyncio.run(main(args.port))
