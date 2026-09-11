# DAW Feasibility Checklist

Purpose: enumerate the actual add-on mechanism(s) per DAW — some DAWs have more than one — plus the DAW-agnostic client types (Host SDK, Plugin wrapper, Plugin SDK), against a common set of capabilities.

✓ yes · ✗ no · ⚠ partial/complicated (see parenthetical)

| Add-on type | Long-running | Host control (transport/mixer/track) | Raw USB/HID | Reaches hacpad service | Deep plugin state (tier 2) | Instance identity in messages |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Bitwig — Controller Script (JS)** | ✓ | ✓ | ✗ | ✓ via OS MIDI ports (OSC/TCP possible too, host-dependent, unverified) | ⚠ (only if plugin exposes it) | ✓ (script can tag messages with project/session identity) |
| **Ableton — Remote Script (Python)** | ✓ | ✓ | ✗ | ✓ via OS MIDI ports (no direct socket access) | ✗ | ✓ (same — script-generated identity) |
| **Ableton — M4L (Max for Live)** | ✓ | ✓ | ⚠ (via native Max externals) | ✓ via OSC/UDP/TCP (Max externals) or OS MIDI ports | ✗ | ✓ |
| **FL Studio — Python controller script** | ✓ | ✓ (smaller surface) | ✗ | ✓ via virtual MIDI ports (direct sockets uncommon, verify per version) | ✗ | ✓ |
| **Reaper — ReaScript (Lua/Python/EEL)** | ✓ | ✓ | ✗ | ✓ via OSC/TCP/UDP directly, or virtual MIDI | ⚠ (only if plugin exposes it) | ✓ |
| **Reaper — native extension (C/C++ SDK)** | ✓ | ✓ | ✓ | ✓ via OSC/TCP/UDP, virtual MIDI, or any native IPC | ⚠ (only if plugin exposes it) | ✓ |
| **Reaper — built-in OSC (control surface)** | ✓ | ✓ | ✗ | ✓ via OSC directly, no script needed | ✗ | ⚠ (needs a distinct port/prefix configured per instance, not automatic) |
| **Web/Browser — WebMIDI / WebUSB / WebHID** | ✗ (transient pages) | ✗ (not a DAW — device access only) | ✓ | ✓ via WebSocket to a native companion sidecar, or WebMIDI virtual ports | ✗ | ✓ (page generates its own session/tab identity) |
| **Logic Pro — Control Surface SDK** *(deprioritized, unverified)* | ✗ | ✓ | ✗ | ✓ via CoreMIDI virtual ports (no direct socket access) | ✗ | ⚠ (plausible, unverified) |
| **Cubase/Nuendo — Generic Remote / controller API** *(deprioritized, unverified)* | ⚠ (moderate) | ✓ | ✗ | ✓ via MIDI (Generic Remote); deeper IPC needs Steinberg SDK access | ✗ | ⚠ (plausible, unverified) |
| **Pro Tools — EUCON/HUI** *(deprioritized, unverified)* | ✗ | ✓ | ✗ | ⚠ via HUI (MIDI-based); EUCON itself is Ethernet-based but partner-gated | ✗ | ⚠ (EUCON has workstation identity; HUI less clear — unverified) |
| **Studio One — remote control API** *(deprioritized, unverified)* | ⚠ (moderate) | ✓ | ✗ | ✓ via MIDI; OSC/vendor SDK where available | ✗ | ⚠ (plausible, unverified) |
| **Host SDK** *(aux, aspirational — doesn't exist for any DAW in scope)* | ✓ | ✓ | ⚠ (native; vendor-dependent whether exposed) | ⚠ via whatever protocol the vendor's SDK chooses to expose | ✗ | ⚠ (vendor-dependent, but any real host SDK exposes project/track identity) |
| **Plugin wrapper** *(aux, DAW-agnostic)* | ✓ | ✗ | ✓ | ✓ via OSC/socket/WebSocket — native code, any protocol we choose | ✗ | ✓ (host already addresses each plugin instance separately) |
| **Plugin SDK** *(aux, DAW-agnostic)* | ✓ | ✗ | ✓ | ✓ via OSC/socket/WebSocket — native code, any protocol we choose | ✓ | ✓ (same — one instance per loaded plugin) |

---

### Practical integration patterns and prototype steps (FL Studio)
- Recommended pattern (robust): external sidecar + FL controller script
	- Use FL's Python controller script for UI mapping and subscriptions; delegate hardware driver duties to an external sidecar that owns USB/HID and exposes a virtual MIDI port to FL.
	- Sidecar responsibilities: device firmware/transport, HID handling, high-rate telemetry aggregation, capability negotiation with runtime, and exposing a TCP/WebSocket control channel for UI/bridge tooling.
	- FL responsibilities: controller script maps UI controls to host parameters and forwards high-level intents to the sidecar via the virtual MIDI port or via a small TCP helper process that the script can call.

- Lightweight pattern (when device is MIDI-only): virtual MIDI loop
	- Use loopMIDI (Windows) or IAC Driver (macOS) to create virtual MIDI ports; hardware -> sidecar -> virtual MIDI -> FL script.

- Prototype steps
	1. Create a minimal FL Python controller script that listens on a named virtual MIDI port and maps simple controls (e.g., knob -> volume) using FL's script API.
 2. Implement a small sidecar (Node.js/Python/Go) that opens the hardware device, exposes it as a virtual MIDI port, and forwards a compact JSON-over-TCP control channel for additional metadata.
 3. Validate end-to-end: hardware -> sidecar -> virtual MIDI -> FL script -> parameter change, and test feedback path (parameter change -> FL -> script -> sidecar -> hardware LED).
 4. Measure latency and adjust batching or use higher-rate channel on sidecar if needed.

- Tools & notes
	- Windows: loopMIDI, rtpMIDI for networked setups.
	- macOS: IAC Driver for virtual MIDI; use CoreMIDI in sidecar.
	- Use user-space MIDI routing libraries (e.g., `mido` for Python, `web-midi`/`midi` node modules) for rapid prototyping.

---

### Web use case (we control client source) — recommended approaches
When we control the client source, the web-hosted DAW case becomes flexible: prefer direct hardware access via browser APIs where possible, and fall back to a companion native sidecar when needed.

- Primary (pure-web) approach: WebMIDI / WebUSB / WebHID
	- Use WebMIDI for MIDI-capable devices (supported in Chromium-based browsers and Opera; Safari support varies). Requires user permission and secure origin (HTTPS or localhost).
	- For non-MIDI HID devices, use WebHID or WebUSB where available. These APIs require explicit user interaction and permission but allow direct device access without a native helper.
	- Use Service Workers and IndexedDB to persist controller presets and connection state where applicable.

- Hybrid approach (recommended for full reliability)
	- Provide a native companion sidecar (Electron, Node, or lightweight Rust/Go binary) that exposes a local WebSocket/TCP API and a WebSocket handshake from the web client (localhost) to the sidecar.
	- Sidecar responsibilities: claim exclusive USB/HID when necessary, expose a WebMIDI-like API over WebSocket, handle firmware updates, and advertise capabilities to the web client.
	- The web client negotiates features with the sidecar on startup and falls back to WebMIDI/WebUSB if the sidecar is absent.

- Message model (compact proposal)
	- `announce`: sidecar -> client: {id, name, capabilities: ["midi","hid","high-rate"]}
	- `negotiate`: client -> sidecar: {desired: ["midi","high-rate"]}
	- `control`: client -> sidecar: {target, value, ts}
	- `subscribe`: client -> sidecar: {target}
	- `feedback`: sidecar -> client: {target, value, ts}

- Prototype steps (web-first, full-control assumption)
	1. Build a minimal web client that uses WebMIDI to enumerate devices and maps a simple control to a UI element.
 2. Implement a sidecar that exposes the same message model over WebSocket and bridges to physical HID when WebMIDI is insufficient.
 3. Add capability negotiation: client prefers sidecar high-rate channel, falls back to WebMIDI/WebUSB if unavailable.
 4. Test across Chrome, Edge, and Firefox (Firefox requires enabling WebMIDI via flags historically); test macOS, Windows, Linux differences.

- Security & UX notes
	- Browsers require user gestures to open WebUSB/WebHID; design onboarding flows to guide permission granting.
	- Use `localhost` + HTTPS or loopback negotiation (e.g., open a short-lived WebSocket with an ephemeral token) to avoid cross-origin issues.

---

## Vendor / Host Add-on Notes (examples)
- Komplete Kontrol / Native Instruments: integration primarily via NKS, host templates, and plugin/host bridges. Deep sync may require vendor cooperation or template generation.
- AKAI VIP, Nektar Panorama: vendor-specific integrations often expose custom templates/mappings and proprietary protocols; treat them as target adapters and approach via vendor SDKs or template generation.

## USB device bridge prototype targets
These exercise the actual **USB device bridges** node (see `DESIGN.md`'s architecture diagram) — bespoke, non-class-compliant hardware, as opposed to anything that already shows up as a standard OS MIDI port (that's the MIDI Bridge's job, not this one). This is where real reverse-engineering work is required, and is the differentiating part of the project — not incidental to it. Ties to `PLAN.md`'s "Vendor-specific hardware bridge accommodation" research tasks.

- **Nektar Panorama**: proprietary control protocol layered over its USB-MIDI interface; the interesting part (LCD text, motorized faders, mode state) lives outside standard MIDI messages. Needs USB traffic capture (Wireshark + USBPcap, or a HID/USB sniffer) against the vendor's own software to reverse the extra protocol.
- **AKAI Advance / APC (VIP-integrated devices)**: similar story — VIP-mode features (display feedback, per-plugin templates) go over a vendor-specific channel, not just class-compliant MIDI. Needs the same capture-and-reverse approach; AKAI VIP's own host software is the reference implementation to observe.
- Prototype steps (either device):
  1. Capture raw USB traffic between the vendor's own software and the device to characterize the non-MIDI protocol.
  2. Implement a minimal `hacpad USB Bridge` (e.g. via `node-hid`/`hidapi`) that claims the device and round-trips one simple case (read one control, write one LED/display update).
  3. Confirm it coexists with — or deliberately takes over from — any class-compliant MIDI interface the same device also exposes.

## Next actions
- Verify uncertain items by consulting the latest DAW developer docs and community examples (links and verification steps per-DAW).
- For high-potential DAW targets (Reaper, Bitwig, Ableton Live, FL Studio, and web hosts — highest ROI), prototype small adapters (ReaScript, Bitwig controller script, Ableton Remote Script / M4L wrapper, FL's Python controller script + sidecar, or a WebMIDI client + sidecar) that communicate with a local sidecar via OSC/TCP; for each, fetch and cite the official developer docs and 2-3 community examples that demonstrate practical adapter patterns.
- For USB device bridges, start with the Nektar Panorama or AKAI Advance capture-and-reverse steps above.
