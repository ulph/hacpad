#!/usr/bin/env bash
# Arm the USB bus spy for the Advance 25. Run with sudo:
#
#     sudo tooling/firmware/arm-spy.sh
#
# Enables usbmon, mounts debugfs if needed, then runs spy.py and mirrors its
# output to a timestamped log so a capture can be reviewed afterwards. Leave it
# running, then do the thing you want to observe (launch VIP, load a plugin,
# move a control). Ctrl-C to stop.
#
# usbmon sees every byte on the wire regardless of who sent it -- Wine, VIP, our
# own tools -- which is exactly why it works for watching VIP drive the device.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

# Where to keep captures. Owned by the invoking user, not root, so we can read
# them back without sudo.
REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME="$(getent passwd "$REAL_USER" | cut -d: -f6)"
LOGDIR="${REAL_HOME}/.cache/hacpad-captures"
LOG="${LOGDIR}/advance-$(date +%Y%m%d-%H%M%S).log"

[ "$(id -u)" -eq 0 ] || { echo "Run me with sudo (need usbmon + debugfs)." >&2; exit 1; }

echo "==> loading usbmon"
modprobe usbmon 2>/dev/null || { echo "modprobe usbmon failed" >&2; exit 1; }

if ! mountpoint -q /sys/kernel/debug; then
    echo "==> mounting debugfs"
    mount -t debugfs none /sys/kernel/debug
fi

install -d -o "$REAL_USER" -g "$REAL_USER" "$LOGDIR"

echo "==> capturing to $LOG"
echo "    launch VIP / use the device now; Ctrl-C to stop"
echo

# --raw also prints non-SysEx MIDI, so nothing is silently missed on first look.
# spy.py runs as root (reads usbmon); the log is chowned back afterwards.
#
# Caveat: usbmon's text interface caps the bytes shown per URB, so this is
# reliable for the short messages of the initial handshake and page setup, but
# will truncate a long SysEx such as a script upload. spy.py detects this and
# warns. For full-length capture we'd read the binary usbmon device instead.
python3 "$HERE/spy.py" --raw 2>&1 | tee "$LOG"
chown "$REAL_USER:$REAL_USER" "$LOG" 2>/dev/null || true
