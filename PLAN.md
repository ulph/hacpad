# hacpad Big Picture Plan

## Purpose
Make hardware-agnostic control of plugins and DAWs easy to configure, extend, and share.

## Vision
Provide a small, extensible project that lets users map physical controls to plugin and DAW actions without being locked to one controller, one DAW, or one plugin format.

## Core goals
- Define a generic control-routing model for hardware, plugins, and DAWs.
- Support application-agnostic mapping so the same physical control can work across different audio tools.
- Enable an MVP with a clear CLI or config-driven API.
- Ship docs, examples, and automated tests.
- Define a two-tier semantic contract for scalar parameter mappings and deeper plugin-only state semantics.
- Treat runtime-to-hardware bridges as extensible community/vendor extension points with feature declaration.

## Research tasks
- Vendor-specific hardware bridge accommodation
	- AKAI VIP: investigate external control interfaces, VIP-host plugin vs sidecar options.
	- Nektar Panorama: investigate integration API and template/mapping generation options.
	- Komplete Kontrol: evaluate NKS/templates/host script hooks needed for synchronization.
	- Define a bridge feature-declaration API and protocol model.
	- Enumerate bridge primitives and negotiation flow.
	- Treat bridges as community/vendor extension points.
    - Define feature declaration and capability negotiation for bridge and provider support.
    - Define transport-agnostic runtime-to-bridge support, since bridges may use USB, MIDI, or intermediate transports.

- Runtime communication model
	- Document channels (plugin SDK, host integration channel) and required primitives.
    - Sidecar runtime research
	    - Run feasibility checklist per DAW and recommend default (standalone vs embedded).
	    - Test embedding feasibility where DAW supports persistent adapters.
    - Define a conflict-resolution and multi-DAW instance strategy for the runtime.

- DAW-specific integration research
	- Collect capability matrices for FL Studio, Bitwig, Ableton Live, Reaper, and custom/web hosts.
        - Research DAW-specific host add-on/sdk constraints

## Hardware Abstraction Layer (HAL)
- Create a HAL that decouples physical controllers from plugin/DAW actions.
- Define a stable interface for input devices, mappings, and output adapters.
- Make it easy to add new controllers or host integrations without changing core logic.
- Keep HAL extensible so future device/plugin adapters can plug in cleanly.
- Deliver proof-of-concept HAL implementations for at least one controller type and one host integration.

## Minimum Viable Product
1. A reusable data model for control targets and source devices.
2. A simple configuration format for mappings.
3. A proof-of-concept command or module that loads mappings and reports/control targets.
4. A README and plan file that explain the architecture and next steps.

## Follow-on milestones
- Add device/plugin discovery and syncing.
- Add DAW integration layers or adapters.
- Add live mode / runtime control with a GUI or CLI.
- Add tests, linting, and CI.
- Publish examples and usage guides.
- Conduct feasibility study for embedding runtime into host SDKs/add-ons (replace prototyping requirement when constrained by DAW capabilities).

## Success criteria
- The repo contains a clear architecture and roadmap.
- Core abstractions are documented and ready for implementation.
- Remaining work is broken into actionable, short-term tasks.

## Conversation reference
- Shared ChatGPT plan: https://chatgpt.com/share/e/6a44e945-5bb0-83ed-9982-6795872543d4
