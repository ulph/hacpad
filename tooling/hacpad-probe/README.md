# hacpad probe

A VST3 plugin with an RPC channel to a host-side sidecar. It is a
**reverse-engineering instrument**, not a shipping component — the word *bridge*
belongs to the device bridges in [`DESIGN.md`](../../DESIGN.md), and this is not
one.

Hosted inside Akai's VIP, it exposes parameters whose names we can change at
runtime. VIP renders plugin parameters onto the Advance 25's screen, so setting a
name to a known marker and finding those bytes on the wire turns protocol work
into a chosen-plaintext exercise instead of guesswork. Once VIP's screen protocol
is understood, the probe has done its job; what we learn feeds the actual (tier 1)
Advance bridge, which talks to the device directly and does not involve VIP.

## Layout

| Path | Role |
|---|---|
| `CMakeLists.txt` | plugin target; JUCE pulled by CPM (`cmake/get_cpm.cmake`) |
| `cmake/mingw-w64.cmake` | toolchain file for cross-compiling to Windows |
| `Source/PluginProcessor.*` | the plugin: renameable parameters + MIDI tap |
| `Source/RpcLink.*` | self-reconnecting TCP client to the sidecar |
| `sidecar.py` | host-side listener + REPL that drives the plugin |

## Building

### Native (Linux) — sanity build

Proves the project compiles and links. Not loadable by VIP (that needs Windows),
but useful for catching mistakes without a cross toolchain.

```sh
cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build -j1        # JUCE units are memory-hungry; -j1 on <8 GB
```

### Windows VST3 — the real target

```sh
sudo apt install mingw-w64
cmake -B build-win -G Ninja \
      -DCMAKE_TOOLCHAIN_FILE=cmake/mingw-w64.cmake \
      -DCMAKE_BUILD_TYPE=Release
cmake --build build-win

cp -r "build-win/HacpadProbe_artefacts/Release/VST3/hacpad probe.vst3" \
      ~/.wine-vip/drive_c/Program\ Files/Common\ Files/VST3/
```

## Using it to probe VIP

1. Start the sidecar on Linux: `python3 sidecar.py`
2. Launch VIP under Wine (see [`../wine/install-wine-reaper.sh`](../wine/install-wine-reaper.sh)).
3. Load **hacpad probe** inside VIP. It connects out to the sidecar; you'll see
   `connected` and the initial parameter list.
4. Start the bus capture: `sudo python3 ../firmware/spy.py`
5. In the sidecar REPL:
   ```
   > names AAAA          # every parameter becomes AAAA0000, AAAA0001, ...
   ```
   Now watch spy.py: the bytes `41 41 41 41 30 30 30 30` locate the name field
   and reveal its encoding. Then:
   ```
   > name 3 ZZ           # a 2-char name — find the length field
   > name 3 <60 chars>   # find the cap
   ```

The port defaults to 8131; override with `HACPAD_RPC_PORT` (plugin) and
`--port` (sidecar) to probe several hosts at once.
