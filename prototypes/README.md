# Prototypes

This folder contains small prototypes to validate DAW scripting/IPC capabilities.

Bitwig test:
- `prototypes/bitwig/mini_controller.js` — a minimal Bitwig controller script that attempts to open a TCP socket to `localhost:9000` and send a short handshake. Drop into your Bitwig controller scripts folder and reload Bitwig to run.
- `prototypes/sidecar/bitwig-sidecar.js` — Node.js TCP server to receive the handshake. Run locally with `node prototypes/sidecar/bitwig-sidecar.js` before loading the Bitwig script.

Notes:
- If Bitwig's scripting environment allows Java socket classes, the script will print logs (view Bitwig's controller console) and the sidecar will receive the handshake. If not, the script will log that Java socket classes are unavailable.
- These prototypes are diagnostic only — they are safe to run and meant to verify whether Bitwig supports outbound sockets from controller scripts.
