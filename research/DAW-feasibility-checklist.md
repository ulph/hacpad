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

## Logic Pro (macOS)
- Persistent process capability: Limited — Logic exposes Control Surface integrations, but lacks a general persistent scripting host for third parties comparable to Reaper.
- IPC options: External helpers can use CoreMIDI, CoreAudio, sockets or XPC; embedding functionality inside Logic is constrained by Apple's ecosystem and Logic's plugin model.
- Plugin sandboxing and permissions: AU and third-party plugins run under Apple's plugin policies and sandboxing; direct hardware access from plugins is limited.
- Hardware access: CoreMIDI/CoreAudio are the supported paths; raw USB/HID access usually must be performed by an external helper process.
- Extension API completeness: Control Surface SDK provides targeted control-surface hooks but is narrower than Reaper/Bitwig/Ableton scripting.
- Versioning and update model: Drivers and plugins updated via installers; control-surface scripts are delivered by vendors and may require signing on macOS.
- Multi-instance handling: Rarely needed; external sidecar is the practical approach for multiplexing.
- Performance/latency constraints: Embedding risks host stability; prefer sidecar for high-rate tasks.
- Security/stability concerns: Apple platform security and signing increase complexity for embedding; external signed helpers recommended.
- Notes / next verification steps: Review Apple's Control Surface docs and CoreMIDI examples for building companion helpers.

## Cubase / Nuendo
- Persistent process capability: Moderate — Cubase provides controller APIs and Generic Remote mappings; deep embedded scripting is less common and often vendor-specific.
- IPC options: Primarily MIDI/Generic Remote; some vendor SDKs enable richer IPC but typically require Steinberg SDK access.
- Plugin sandboxing and permissions: Standard VST/AU restrictions apply; dedicated controller integrations are handled via the controller API.
- Hardware access: Via OS MIDI drivers; raw USB access from inside the host is unlikely.
- Extension API completeness: Varies by version; consult Steinberg developer docs for specific capabilities.
- Versioning and update model: File-based templates or SDK installers; vendor partnerships may be required for advanced integrations.
- Multi-instance handling: External sidecar recommended for coordination.
- Performance/latency constraints: Controller templates suitable for control UI; high-rate paths are better handled by an external sidecar.
- Security/stability concerns: May require SDK licensing for deep integration.
- Notes / next verification steps: Obtain Steinberg developer docs and sample controller integrations.

## Pro Tools
- Persistent process capability: Limited — deep third-party extension points are constrained; control-surface integrations are typically via established protocols (EUCON, HUI) or vendor partnerships.
- IPC options: EuCon or HUI for control surfaces; proprietary vendor SDKs for advanced integrations.
- Plugin sandboxing and permissions: AAX plugin model and Avid's policies restrict arbitrary embedding; partner programs often required for deep integration.
- Hardware access: Control surfaces supported via approved protocols; raw hardware access inside Pro Tools is uncommon.
- Extension API completeness: Not as open as other DAWs; expect to rely on vendor protocols or partnership SDKs.
- Versioning and update model: Strict compatibility and installer-based updates; coordinate closely with Avid practices.
- Multi-instance handling: External sidecar recommended.
- Performance/latency constraints: Real-time and compatibility constraints are strict.
- Security/stability concerns: High; avoid embedding untrusted native code inside the host.
- Notes / next verification steps: Investigate EUCON partner program and vendor SDK availability if Pro Tools integration is required.

## Studio One
- Persistent process capability: Moderate — Studio One provides remote control APIs and some extension points, but not as open as Reaper.
- IPC options: MIDI, OSC, and vendor SDKs where available; verify current PreSonus developer offerings.
- Plugin sandboxing and permissions: Standard VST/AU behaviour applies.
- Hardware access: Via OS MIDI; raw USB/HID access requires a companion helper.
- Extension API completeness: Moderate — sufficient for many control-surface mappings but limited for deep embedding.
- Versioning and update model: Vendor-managed installers and updates; use vendor tools for distribution.
- Multi-instance handling: Use external sidecar for coordination.
- Performance/latency constraints: Control-rate operations supported; low-latency native paths require sidecar or native modules.
- Security/stability concerns: Prefer vendor SDKs for deep work.
- Notes / next verification steps: Fetch PreSonus SDK/docs and sample remote integrations.

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

---

## Vendor / Host Add-on Notes (examples)
## Vendor / Host Add-on Notes (examples)
- Komplete Kontrol / Native Instruments: integration primarily via NKS, host templates, and plugin/host bridges. Deep sync may require vendor cooperation or template generation.
- AKAI VIP, Nektar Panorama: vendor-specific integrations often expose custom templates/mappings and proprietary protocols; treat them as target adapters and approach via vendor SDKs or template generation.

---

## Verification plan and sources to fetch
- For each DAW above, fetch and cite the official developer docs (controller API, scripting guides, SDK pages) and 2-3 community examples or forum posts that demonstrate practical adapter patterns.
- Prioritize verification for: Reaper, Bitwig, Ableton Live, and web hosts (highest ROI for prototypes).


---

## Next actions
- Verify uncertain items by consulting the latest DAW developer docs and community examples (links and verification steps per-DAW).
- For high-potential targets (Reaper, Bitwig, Ableton Live), prototype small adapters (ReaScript, Bitwig controller script, Ableton Remote Script / M4L wrapper) that communicate with a local sidecar via OSC/TCP.
