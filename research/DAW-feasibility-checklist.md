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
- Persistent process capability: Yes — Bitwig supports JavaScript-based controller scripts which run persistently while the host is open.
- IPC options: Controller scripts run inside Bitwig's process (JVM). External IPC typically done via network sockets/OSC from scripts or via an external bridge; confirm official support for outbound sockets.
- Plugin sandboxing and permissions: Controller scripts are run in host scripting API; VST/AU plugins are separate and sandboxed as usual.
- Hardware access: Controller scripts can receive/send MIDI; direct USB access from the host script is limited — use OS MIDI drivers or a bridge.
- Extension API completeness: Rich controller API exposing tracks, devices, parameters, transport, and observer APIs.
- Versioning and update model: Controller scripts packaged with Bitwig or installed into the controller scripts folder; updating requires replacing script files or packaging updates.
- Multi-instance handling: Bitwig runs per-user instance; coordinating multiple DAW instances requires external sidecar or scripts that detect/multiplex instances.
- Performance/latency constraints: Scripting is not real-time audio thread — suitable for control paths; high-rate feedback may need a dedicated bridge.
- Security/stability concerns: Scripts run in the Bitwig scripting environment; misbehaving scripts can affect the host.
- Notes / next verification steps: Confirm whether controller scripts can open outbound TCP/UDP sockets reliably (community examples exist). Link: Bitwig Controller API docs (verify).

## Ableton Live
- Persistent process capability: Yes — Live supports persistent Python MIDI Remote Scripts; Max for Live (M4L) provides additional extension capabilities inside Live Suite.
- IPC options: Python Remote Scripts are embedded; direct network access from the Python environment is historically limited/undocumented. Max for Live can use externals to reach sockets/OSC.
- Plugin sandboxing and permissions: VST/AU plugins are sandboxed; Remote Scripts run within Live and have restricted APIs.
- Hardware access: Remote Scripts and M4L can send/receive MIDI; direct USB device access is not provided — use OS MIDI routing or dedicated bridge.
- Extension API completeness: Remote Scripts provide many control surface primitives (parameters, transport, tracks); M4L can access clip/device state but requires Suite.
- Versioning and update model: Remote Scripts are installed to Live's User Library or system folders; updates require replacing scripts and may need Live restart.
- Multi-instance handling: Live typically runs single instance per user; coordination across instances needs external sidecar.
- Performance/latency constraints: MIDI Remote Scripts are not real-time audio threads, but provide responsive control; high-rate streaming may be limited.
- Security/stability concerns: Python environment restrictions vary by Live version; M4L can run user code (with its own sandboxing) — test for stability.
- Notes / next verification steps: Verify what networking primitives are available from Remote Scripts in the current Live version; test M4L externals for OSC sockets.

## FL Studio
- Persistent process capability: Yes — FL Studio supports controller scripts (Python-based) for persistent controller mappings.
- IPC options: Scripting runs inside FL Studio; external IPC commonly uses MIDI or network-to-MIDI bridges; direct socket support from scripts requires verification.
- Plugin sandboxing and permissions: VSTs/AU are sandboxed normally; controller scripts operate in the host scripting environment.
- Hardware access: Controller scripts can use OS MIDI drivers; direct USB access is not typical from scripts.
- Extension API completeness: FL's MIDI scripting exposes transport and parameter mapping; completeness varies vs other DAWs.
- Versioning and update model: Scripts are installed into FL's MIDI Scripts folder; updating requires replacing files and restarting FL.
- Multi-instance handling: Multiple FL instances need external coordination via a sidecar.
- Performance/latency constraints: Scripting not suitable for hard real-time; acceptable for control UI.
- Security/stability concerns: Scripting errors can affect FL's scripting host.
- Notes / next verification steps: Collect samples of FL Python scripts that open sockets (if any) and test feasibility.

## Reaper
- Persistent process capability: Yes — Reaper is extremely extensible; ReaScript (Lua/Python/EEL) and native extensions can be long-running.
- IPC options: Reaper supports OSC, TCP/UDP, and ReaScript can call OS-level calls (subject to scripting language capabilities). ReaPack and ReaScripts can be used to coordinate external processes.
- Plugin sandboxing and permissions: Reaper is permissive; extensions can access filesystem and network depending on script language.
- Hardware access: Reaper can send/receive MIDI; extensions can often access OS-level features; embedding a bridge is generally feasible.
- Extension API completeness: Very complete — track/device/parameter/transport APIs are accessible via ReaScript and native extensions.
- Versioning and update model: Scripts and extensions are installed per-user; ReaPack simplifies distribution/updates.
- Multi-instance handling: Multiple instances are possible; coordination may be possible via OSC or external sidecar.
- Performance/latency constraints: ReaScript runs outside audio thread (unless native extension); high-rate paths possible via native extensions.
- Security/stability concerns: Native extensions may crash host; use careful isolation.
- Notes / next verification steps: Reaper is a prime candidate for embedded adapters — prototype a small ReaScript adapter that communicates with a local sidecar.

## Logic Pro (macOS)
- Persistent process capability: Limited — Control Surface support exists but third-party persistent add-ons are constrained; no general-purpose embedded scripting like Reaper.
- IPC options: CoreMIDI and macOS IPC (XPC, sockets) are available to external helpers, but embedding inside Logic is limited.
- Plugin sandboxing and permissions: AU/VST plugins run sandboxed with restrictions; Apple's security model limits arbitrary hardware access from plugins.
- Hardware access: Use CoreMIDI/CoreAudio; direct USB access from logic-internal scripts is not a typical extension point.
- Extension API completeness: Control Surface SDK and MIDI Learn exist but are narrower than full scripting APIs.
- Versioning and update model: Control Surface drivers and plugins updated via installers; tight platform integration complicates live updates.
- Multi-instance handling: macOS typically runs one Logic instance; coordination across instances rarely needed.
- Performance/latency constraints: Audio thread constraints apply; embedding risks host stability.
- Security/stability concerns: Apple policies and sandboxing reduce embedding feasibility — prefer external sidecar on macOS.
- Notes / next verification steps: Investigate Apple's Control Surface SDK details and whether third-party adapters can run as persistent helpers alongside Logic.

## Cubase / Nuendo
- Persistent process capability: Cubase supports Controller APIs and Generic Remote mappings; scripting/embedded adapters are more limited and vendor-specific.
- IPC options: Typically MIDI/Generic Remote; external adapters via MIDI or proprietary APIs (e.g., Steinberg extensions) may exist.
- Plugin sandboxing and permissions: Standard VST/AU restrictions; controller integrations typically run as configured mappings.
- Hardware access: Via OS MIDI drivers; direct USB from inside host unlikely.
- Extension API completeness: Controller APIs exist but vary by Cubase version; check Steinberg docs for developer SDKs.
- Versioning and update model: Controller scripts and templates updated via file installs or SDK-provided installers.
- Multi-instance handling: Multiple instances coordination requires external sidecar.
- Performance/latency constraints: Controller templates are adequate for UI mapping; deep integration may be limited.
- Security/stability concerns: Using vendor SDKs may be required; check licensing/SDK access.
- Notes / next verification steps: Collect Steinberg controller API docs and community examples.

## Pro Tools
- Persistent process capability: Limited — Pro Tools uses AAX plugins and dedicated control surface integrations (E.g., EUCON) but third-party embedding is constrained.
- IPC options: Typically hardware control via EuCon or HUI; vendor SDKs may be required.
- Plugin sandboxing and permissions: AAX plugins are restricted; embedding runtime inside Avid host is unlikely.
- Hardware access: Control surfaces via supported protocols; direct USB/hardware access from within Pro Tools is constrained.
- Extension API completeness: Not as open as other DAWs; vendor relationships often required for deep integration.
- Versioning and update model: Enterprise-style installers and strict compatibility requirements.
- Multi-instance handling: Not commonly addressed; external sidecar recommended.
- Performance/latency constraints: Real-time constraints and AAX sandboxing important.
- Security/stability concerns: High — incompatible code can destabilize large studio environments.
- Notes / next verification steps: Evaluate EUCON and Avid partner SDK options if vendor integration is needed.

## Studio One
- Persistent process capability: Studio One supports extensions and remote control (Amazon/OSC style integrations) to an extent, but third-party persistent embedding is limited compared to Reaper.
- IPC options: MIDI, OSC, and remote APIs where available; confirm current SDK offerings from PreSonus.
- Plugin sandboxing and permissions: Standard VST/AU restrictions.
- Hardware access: Via OS MIDI; direct USB within host is uncommon.
- Extension API completeness: Moderate; check PreSonus SDK/docs for details.
- Versioning and update model: Extensions updated via installers or package managers provided by vendor.
- Multi-instance handling: External sidecar recommended for multi-instance coordination.
- Performance/latency constraints: Acceptable for control surfaces; real-time paths better handled by sidecar/native extensions.
- Security/stability concerns: Use vendor SDKs when possible.
- Notes / next verification steps: Review Studio One's Remote API and developer docs.

## Web / Browser-based hosts
- Persistent process capability: Browser pages are ephemeral; persistent adapters must be external services or browser extensions.
- IPC options: WebSockets, WebMIDI, WebUSB where supported, postMessage, and Service Workers for limited persistence.
- Plugin sandboxing and permissions: Browsers strongly sandbox hardware access; WebMIDI/WebUSB require user permission and browser support.
- Hardware access: WebMIDI/WebUSB available in modern browsers with user permission; reliability varies across browsers/OS.
- Extension API completeness: Highly variable; embedding runtime into browser-hosted DAW requires bridge via WebSocket or native companion app.
- Versioning and update model: Web deployments update centrally; browser extension/update cycles apply.
- Multi-instance handling: Browser tabs/hosts can be multiplexed via a central sidecar or signaling server.
- Performance/latency constraints: Network and browser event loop introduces latency; not ideal for high-rate feedback without native helpers.
- Security/stability concerns: Browser permissions and user prompts; recommend external native helper for reliable hardware access.
- Notes / next verification steps: Prototype a companion sidecar that exposes a WebSocket + WebMIDI fallback.

---

## Vendor / Host Add-on Notes (examples)
- Komplete Kontrol / Native Instruments: integration primarily via host templates, NKS, and plugin/host bridges; investigate SDKs and template distribution.
- AKAI VIP, Nektar Panorama: vendor-specific host integrations often expose custom templates or mappings; treat as adapter targets rather than general embedding platforms.

---

## Next actions
- Verify uncertain items by consulting the latest DAW developer docs and community examples (links and verification steps per-DAW).
- For high-potential targets (Reaper, Bitwig, Ableton Live), prototype small adapters (ReaScript, Bitwig controller script, Ableton Remote Script / M4L wrapper) that communicate with a local sidecar via OSC/TCP.
