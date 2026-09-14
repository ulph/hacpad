#!/usr/bin/env python3
"""
Quick live demo: Panorama P1 hardware -> real system control.

CC 37  -> system output volume (0-127 mapped to 0-100%)
CC 111 -> toggle mute (fires on value==127 only, ignores the 0 release)

Not the final CC map -- this is a fast empirical demo to prove the
input path drives something real, while research/panorama-p1-protocol-notes.md
gets updated with the actual observed CC numbers.
"""
import subprocess
import sys

DEVICE = sys.argv[1] if len(sys.argv) > 1 else "/dev/snd/midiC1D0"


def set_volume_pct(pct: int) -> None:
    subprocess.run(["pactl", "set-sink-volume", "@DEFAULT_SINK@", f"{pct}%"], check=False)


def toggle_mute() -> None:
    subprocess.run(["pactl", "set-sink-mute", "@DEFAULT_SINK@", "toggle"], check=False)


def main() -> None:
    print(f"Opening {DEVICE} ...")
    with open(DEVICE, "rb", buffering=0) as f:
        print("Live: move the fader (CC 37) for volume, press the button (CC 111) to mute.")
        msg = bytearray()
        while True:
            byte = f.read(1)
            if not byte:
                continue
            b = byte[0]
            if b & 0x80:
                msg = bytearray([b])
            else:
                msg.append(b)
            if len(msg) == 3 and 0xB0 <= msg[0] <= 0xBF:
                cc, value = msg[1], msg[2]
                if cc == 37:
                    pct = round(value / 127 * 100)
                    set_volume_pct(pct)
                    print(f"volume -> {pct}%")
                elif cc == 111 and value == 127:
                    toggle_mute()
                    print("mute toggled")
                msg = bytearray()


if __name__ == "__main__":
    main()
