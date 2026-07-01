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

## Design-derived priorities (from DESIGN.md)
- Define the two-tier semantic contract: scalar parameter mappings plus deeper plugin-only state semantics.
- Document the shared runtime integration channel for host SDK/add-on and the plugin semantic provider.
- Research DAW-specific host add-on/sdk constraints for FL Studio, Bitwig, Ableton Live, Reaper, and custom/web hosts.
- Define transport-agnostic runtime-to-bridge support, since bridges may use USB, MIDI, or intermediate transports.
- Treat runtime-to-hardware bridges as extension points for community contributors and hardware vendors.
- Define feature declaration and capability negotiation for bridge and provider support.
- Define a conflict-resolution and multi-DAW instance strategy for the runtime.

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

## Success criteria
- The repo contains a clear architecture and roadmap.
- Core abstractions are documented and ready for implementation.
- Remaining work is broken into actionable, short-term tasks.

## Conversation reference
- Shared ChatGPT plan: https://chatgpt.com/share/e/6a44e945-5bb0-83ed-9982-6795872543d4

## Next step
Import the shared AI conversation, extract requirements, and turn them into prioritized implementation tasks.
