#!/usr/bin/env bash
# Install Wine + REAPER into an isolated prefix, for hosting Akai's VIP.
#
# VIP ships only for macOS and Windows, and it is the thing that knows how to
# drive the Advance 25's screen. Native Linux REAPER cannot load it, so we need
# a Windows host. Audio quality is irrelevant here: we only need MIDI to flow.
#
# Everything lands in one prefix (default ~/.wine-vip) so the whole experiment
# is disposable -- delete that directory and nothing else is affected.
#
# Run as your normal user. It calls sudo only for the apt steps; wine itself
# must NOT run as root or the prefix ends up owned by root.
set -euo pipefail

PREFIX="${WINEPREFIX:-$HOME/.wine-vip}"
REAPER_VER="${REAPER_VER:-780}"
REAPER_EXE="reaper${REAPER_VER}_x64-install.exe"
REAPER_URL="https://www.reaper.fm/files/7.x/${REAPER_EXE}"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/hacpad-wine"

say()  { printf '\n\033[1;33m==>\033[0m \033[1m%s\033[0m\n' "$*"; }
info() { printf '    %s\n' "$*"; }
die()  { printf '\n\033[1;31mERROR:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" -ne 0 ] || die "Do not run this as root. Wine would create a root-owned prefix."
command -v sudo >/dev/null || die "sudo not found; this needs it for the apt steps."
command -v apt-get >/dev/null || die "This script targets Debian/Ubuntu (apt-get not found)."

CODENAME="$(. /etc/os-release && echo "${UBUNTU_CODENAME:-${VERSION_CODENAME:-}}")"
[ -n "$CODENAME" ] || die "Could not determine the distro codename from /etc/os-release."

say "Target"
info "prefix:   $PREFIX"
info "codename: $CODENAME"
info "reaper:   $REAPER_EXE"

# ---------------------------------------------------------------- 1. wine ----
if command -v wine >/dev/null; then
    say "Wine already installed ($(wine --version)) -- skipping install"
else
    say "Enabling i386 architecture"
    # Wine needs 32-bit support even for 64-bit apps; installers often have
    # 32-bit stubs that fail confusingly without it.
    sudo dpkg --add-architecture i386

    say "Adding the WineHQ repository"
    # Ubuntu's packaged Wine lags badly; use WineHQ's own builds.
    sudo mkdir -pm755 /etc/apt/keyrings
    sudo wget -q -O /etc/apt/keyrings/winehq-archive.key \
        https://dl.winehq.org/wine-builds/winehq.key
    SOURCES_URL="https://dl.winehq.org/wine-builds/ubuntu/dists/${CODENAME}/winehq-${CODENAME}.sources"
    if ! wget -q --spider "$SOURCES_URL"; then
        die "WineHQ has no build for '${CODENAME}'. Set CODENAME to the nearest LTS
       (e.g. CODENAME=noble) and re-run, or install wine from Ubuntu's repo."
    fi
    sudo wget -q -NP /etc/apt/sources.list.d/ "$SOURCES_URL"

    say "Installing wine + winetricks (this pulls a lot)"
    sudo apt-get update
    sudo apt-get install -y --install-recommends winehq-stable winetricks
fi

command -v wine >/dev/null || die "wine still not on PATH after install."

# -------------------------------------------------------------- 2. prefix ----
export WINEPREFIX="$PREFIX"
export WINEARCH=win64
export WINEDEBUG="${WINEDEBUG:--all}"   # quiet by default; override to debug

if [ -d "$PREFIX/drive_c" ]; then
    say "Prefix already exists -- skipping creation"
else
    say "Creating 64-bit prefix at $PREFIX"
    wineboot --init
    wineserver -w
fi

say "Setting Windows version to win10"
# Newer app installers refuse to run when the reported version is too old.
wine reg add 'HKCU\Software\Wine' /v Version /t REG_SZ /d win10 /f >/dev/null 2>&1 || true

# ------------------------------------------------------------ 3. runtimes ----
if [ -f "$PREFIX/.hacpad-runtimes" ]; then
    say "Runtimes already installed -- skipping"
else
    say "Installing VC++ runtimes and core fonts"
    info "REAPER and VIP are both MSVC-built and need these."
    if winetricks -q vcrun2019 corefonts; then
        touch "$PREFIX/.hacpad-runtimes"
    else
        info "winetricks reported a problem; continuing anyway (often still fine)."
    fi
fi

# -------------------------------------------------------------- 4. reaper ----
REAPER_DIR="$PREFIX/drive_c/Program Files/REAPER (x64)"
if [ -f "$REAPER_DIR/reaper.exe" ]; then
    say "REAPER already installed -- skipping"
else
    mkdir -p "$CACHE"
    if [ ! -s "$CACHE/$REAPER_EXE" ]; then
        say "Downloading REAPER $REAPER_VER"
        wget -q --show-progress -O "$CACHE/$REAPER_EXE.part" "$REAPER_URL" \
            || die "Download failed. Check https://www.reaper.fm/download.php for the current version and set REAPER_VER."
        mv "$CACHE/$REAPER_EXE.part" "$CACHE/$REAPER_EXE"
    else
        say "Using cached installer"
    fi

    say "Installing REAPER (accept the defaults in the installer window)"
    info "Free, fully functional for a 60-day evaluation. No registration."
    wine "$CACHE/$REAPER_EXE" /S || true    # /S is silent; falls back to GUI
    wineserver -w
fi

# --------------------------------------------------------------- 5. check ----
say "Checkpoint: is the Advance visible to ALSA?"
if grep -q 'ADVANCE25' /proc/asound/seq/clients 2>/dev/null; then
    grep -E '^Client|Port ' /proc/asound/seq/clients | grep -A4 ADVANCE25 | sed 's/^/    /'
else
    info "ADVANCE25 not found in /proc/asound/seq/clients."
    info "Plug the device in and power it on; nothing downstream works without it."
fi

cat <<EOF

$(say "Done")
    Launch REAPER:

      WINEPREFIX=$PREFIX wine "C:\\Program Files\\REAPER (x64)\\reaper.exe"

    Then Options -> Preferences -> Audio -> MIDI Devices, and enable BOTH
    input and output on all three ADVANCE25 ports. The screen protocol goes
    out on MIDI 3 and replies arrive on MIDI 1, so a one-directional setup
    looks broken for reasons that have nothing to do with Wine.

    THE CHECK THAT MATTERS -- with REAPER open, from a normal shell:

      grep -E '^Client' /proc/asound/seq/clients

    A new client alongside ADVANCE25 means Wine's MIDI is bridged to ALSA,
    and the capture plan works with no usbmon and no root. If nothing
    appears, stop and fix MIDI before installing VIP on top of it.

    Next: install the FULL Windows VIP installer (not the update package)
    into this same prefix:

      WINEPREFIX=$PREFIX wine ~/Downloads/VIP-Setup.exe

EOF
