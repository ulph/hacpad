#!/usr/bin/env python3
"""
Capture real MIDI/SysEx traffic that Bitwig Studio sends to the Panorama P1,
by opening a virtual ALSA sequencer input port and using `aconnect` to fan
Bitwig's own output connection to the device out to us as well (in addition
to its real connection to the hardware -- this does not interrupt Bitwig).

Run this in a real terminal (NOT the sandboxed Claude Code session) on the
same machine Bitwig and the P1 are connected to, since it needs `aconnect`
and full ALSA sequencer access.

Usage:
    pip install --user python-rtmidi   # if not already installed
    python3 sniff_bitwig.py

Then, in a SEPARATE terminal, once this script prints its own port name:
    aconnect -l                                  # find Bitwig's client name
    aconnect 'Bitwig Studio':0 'hacpad-sniffer':0 # or whatever ports show up
    aconnect 'Bitwig Studio':1 'hacpad-sniffer':0 # repeat for every Bitwig
                                                   # output port connected to
                                                   # the Panorama P1

Then in Bitwig: load a plugin, open the Mixer, tweak a macro knob, switch
tracks, etc -- anything that would change what's on the P1's screen. Every
raw MIDI/SysEx message logged here is now genuine ground truth for exactly
what Bitwig sends the device, no reverse-engineering guesswork involved.

Everything printed is also appended to sniff_log.txt (timestamped, one line
per message) in this directory, so you can just paste the file back rather
than the whole terminal scrollback.
"""
import sys
import time

try:
    import rtmidi
except ImportError:
    print("Missing dependency. Run: pip install --user python-rtmidi", file=sys.stderr)
    sys.exit(1)

LOG_PATH = "sniff_log.txt"


def main():
    midiin = rtmidi.MidiIn()
    midiin.open_virtual_port("hacpad-sniffer")
    midiin.ignore_types(sysex=False, timing=False, active_sense=False)

    print("Opened virtual ALSA sequencer port 'hacpad-sniffer'.")
    print("In another terminal, run `aconnect -l` to find Bitwig's client,")
    print("then `aconnect <bitwig-client>:<port> hacpad-sniffer:0` for each")
    print("output port Bitwig has connected to the Panorama P1.")
    print(f"Logging to {LOG_PATH} -- Ctrl-C to stop.\n")

    with open(LOG_PATH, "a", buffering=1) as log:
        log.write(f"\n--- session started {time.strftime('%Y-%m-%d %H:%M:%S')} ---\n")
        try:
            while True:
                msg = midiin.get_message()
                if msg:
                    data, deltatime = msg
                    hexstr = " ".join(f"{b:02X}" for b in data)
                    line = f"[{time.time():.3f}] ({len(data):3d} bytes) {hexstr}"
                    print(line)
                    log.write(line + "\n")
                else:
                    time.sleep(0.001)
        except KeyboardInterrupt:
            print("\nStopped.")


if __name__ == "__main__":
    main()
