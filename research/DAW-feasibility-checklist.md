# DAW Feasibility Checklist

Purpose: capture per-DAW answers to the embedding feasibility checklist from `DESIGN.md` so we can recommend a default architecture (standalone sidecar by default) and identify where embedded adapters are realistic.

Template (answer each item per-DAW):
- Persistent process capability: (Can the DAW host long-running controller/add-on scripts?)
- IPC options: (sockets, OSC, shared memory, named pipes, other?)
- Plugin sandboxing and permissions: (Are plugins/add-ons prevented from network/hardware access?)
- Hardware access: (Can add-ons access USB/CoreMIDI/OS-level hardware directly?)
- Extension API completeness: (Mixer/track/parameter events, transport, action invocation?)
- Versioning and update model: (How are add-ons/scripts updated and versioned?)
- Multi-instance handling: (Can an embedded adapter detect/coordinate multiple DAW instances?)
- Performance/latency constraints: (Any known limits or host-imposed sample-rate/real-time restrictions?)
- Security/stability concerns: (Crash risk, sandboxing, policy limitations?)
- Notes / next verification steps: (Links to docs, community notes, or experiments to run)

---

## Bitwig Studio
- Persistent process capability: Yes — Bitwig provides a controller API (controller scripts) that runs while the host is open; scripts can maintain state and subscriptions for the controller surface lifecycle.
- IPC options: Controller scripts run inside Bitwig's scripting host; common approaches for external communication are: OS-level MIDI ports, OSC or TCP/UDP to a local helper (if scripting language allows sockets). Treat external sockets as host-dependent — verify with current Bitwig docs.
- Plugin sandboxing and permissions: VST/AU plugins are sandboxed normally; controller scripts execute in Bitwig's scripting environment and are constrained by the API.
- Hardware access: Controller scripts use OS MIDI drivers for hardware; direct raw USB access from inside the script is not typical. For advanced device access (firmware update, HID), use a companion sidecar/bridge.
- Extension API completeness: High for control-surface workflows: transport, tracks, devices, parameter observers, and user action hooks exist — sufficient for Tier 1 mappings and many Tier 2 semantics when the plugin exposes them.
- Versioning and update model: Scripts are installed to Bitwig's controller scripts folder or packaged; updates are file-based and generally require a restart of the host to reload scripts.
- Multi-instance handling: No built-in cross-instance coordination; use external sidecar to multiplex or coordinate multiple Bitwig instances.
- Performance/latency constraints: Scripting is not on the audio thread — appropriate for control-rate interactions. For very high-rate telemetry, prefer a native sidecar with a high-rate feedback channel.
- Security/stability concerns: Faulty scripts can affect the host process; prefer conservative operations and isolate heavy I/O to external helpers.
- Notes / next verification steps: Confirm current Bitwig scripting language capabilities for opening outbound sockets; collect examples of community controller scripts that do IPC.

## Ableton Live
- Persistent process capability: Yes — Live provides Python-based MIDI Remote Scripts for control surfaces; these run while Live is running and maintain state.
- IPC options: Remote Scripts are embedded and historically limited in direct network access; Max for Live (M4L) can provide additional IPC via Max externals (OSC, UDP/TCP) when Live Suite is available. External sidecars commonly integrate via virtual MIDI ports or a small helper communicating over sockets.
- Plugin sandboxing and permissions: VST/AU plugins are sandboxed; Remote Scripts have a host-provided API with limited low-level access. M4L devices run inside Live but operate within Max's environment.
- Hardware access: MIDI I/O via OS drivers is available to Remote Scripts/M4L; raw USB/HID access requires an external process or Max externals with native code.
- Extension API completeness: Remote Scripts provide comprehensive control-surface primitives (tracks, devices, parameters, transport); M4L exposes device/clip data inside Live Suite.
- Versioning and update model: Scripts are file-based and require host reload/restart to take effect; distribution via packages or user Library.
- Multi-instance handling: Live doesn't provide cross-instance coordination; external sidecar required for multiplexing.
- Performance/latency constraints: Control-rate operations are supported; host scripting is not on audio thread so use sidecar for sub-millisecond or high-rate feedback.
- Security/stability concerns: Python scripts and M4L devices can affect Live stability if they block or misbehave; isolate heavy I/O to external helpers.
- Notes / next verification steps: Validate current Live version's Remote Script networking capabilities and test a simple Remote Script <-> sidecar via virtual MIDI + TCP.

## FL Studio
- Persistent process capability: Yes — FL Studio includes a Python-based controller scripting system that runs while the host is open.
- IPC options: Scripts run inside FL; common integration patterns use virtual MIDI ports or external helpers; direct socket access from FL scripts is uncommon and should be verified per FL version.
- Plugin sandboxing and permissions: VST/AU sandboxing applies; controller scripts are constrained to the host scripting API.
- Hardware access: MIDI via OS drivers is supported; raw USB/HID access requires an external companion process.
- Extension API completeness: Provides MIDI mapping and transport hooks; API surface is smaller than Live/Bitwig but sufficient for many controller mappings.
- Versioning and update model: Scripts are file-based and require restart to reload.
- Multi-instance handling: Not built-in — use an external sidecar for coordination.
- Performance/latency constraints: Scripting is control-rate only; use sidecar for high-rate telemetry.
- Security/stability concerns: Avoid heavy I/O in scripts; delegate to external helpers.
- Notes / next verification steps: Test a simple Python script + helper pattern using virtual MIDI to validate latency and reliability.

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

## Reaper
- Persistent process capability: Yes — Reaper provides ReaScript (Lua/Python/EEL) and an SDK for native extensions; scripts and extensions can run persistently and are well-suited for adapters.
- IPC options: Strong: OSC, TCP/UDP, and direct OS calls from scripts (depending on language) are supported. Reaper's extensibility makes it easy to integrate external helpers via OSC/UDP/TCP and virtual MIDI.
- Plugin sandboxing and permissions: Reaper is permissive compared to other DAWs; scripting languages have OS access consistent with their runtime.
- Hardware access: MIDI and system-level access for extensions is available; native extensions can access more low-level APIs if needed.
- Extension API completeness: Very complete — provides track/device/parameter/transport APIs and observer hooks; ideal for both Tier 1 and many Tier 2 semantics when plugin exposes state.
- Versioning and update model: Scripts/extensions are user-installable; ReaPack simplifies distribution and updates.
- Multi-instance handling: Reaper supports multiple instances; coordination via OSC or a sidecar is feasible.
- Performance/latency constraints: Native extensions can achieve low latency; scripting is not on audio thread unless using native modules.
- Security/stability concerns: Native code can crash host — isolate risky operations; prefer a sidecar for hardware drivers or heavy IO.
- Notes / next verification steps: Prioritize Reaper for an embedded-adapter prototype (ReaScript) communicating to a sidecar via OSC/TCP.

## Deprioritized DAWs (not yet verified, not near-term targets)
- **Logic Pro (macOS)**: no general persistent scripting host for third parties; Control Surface SDK + CoreMIDI/CoreAudio only. Apple signing/sandboxing raises the bar for embedding.
- **Cubase / Nuendo**: controller API + Generic Remote mappings; deeper IPC needs Steinberg SDK access.
- **Pro Tools**: constrained third-party extension points; control-surface integration goes through EUCON/HUI or a partner program.
- **Studio One**: remote control APIs exist but are less open than Reaper's; verify current PreSonus SDK offerings if this becomes a target.

## Web / Browser-based hosts
- Persistent process capability: Limited — browser pages are transient; persistence requires either a service worker (limited) or an external native companion / server.
- IPC options: WebSocket, WebRTC, WebMIDI, WebUSB (where supported), and postMessage; reliability and permissions vary by browser and OS.
- Plugin sandboxing and permissions: Browsers enforce user consent for hardware APIs; WebMIDI/WebUSB require explicit user approval and are limited in capability.
- Hardware access: WebMIDI/WebUSB can access MIDI and USB devices in modern browsers with permission; for consistent hardware support, a native sidecar is recommended.
- Extension API completeness: Varies; web-hosted DAWs should offer a documented API (if any) — otherwise use a companion sidecar.
- Versioning and update model: Centralized for web apps; browser extension updates follow store policies.
- Multi-instance handling: Use a central sidecar or signaling server to coordinate multiple browser tabs or hosts.
- Performance/latency constraints: Browser event loop and network latency limit high-rate telemetry; native helpers provide better performance.
- Security/stability concerns: Browser permissions and CORS/security model limit direct hardware access; use secure, consent-driven flows.
- Notes / next verification steps: Prototype a companion sidecar + WebSocket bridge and test WebMIDI fallbacks across major browsers.

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
