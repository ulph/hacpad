# hacpad Design: Host-Agnostic Semantic Control

This design centers on two core pieces:

- **Mapping markup**: the portable description of how DAW/plugin parameters, actions, gestures, and feedback map to actual controller hardware. This is the semantic contract that is shared across hosts, plugins, and external maps.
- **Hardware communication**: the transport path between software and controller hardware, which can be delivered through a host-specific add-on, a host-level SDK, or through a plugin-layer SDK.

A research task is needed to determine which DAWs expose integration paths that allow external parties to build host-specific add-ons.

These are the two main axes: what the controller should do, and how the controller is actually driven.

When both host-level and plugin-layer communication are available, host-level SDK integration takes precedence. The plugin layer can still declare custom mappings, but those mappings are always expressed as markup and can be sourced externally to the plugin.

Host vs plugin capability resolution should be negotiated via versioning and capability metadata, so the runtime can choose the most authoritative source and fall back cleanly.

Our design goal is also to ensure the out-of-box experience with our provided DAW add-ons is as consistent as possible.

Supporting roles:
- **Host endpoint**: the DAW/host provider that owns canonical parameter state, automation, persistence, and undo.
- **Semantic provider**: a two-tiered contract for parameter-level mappings plus deeper plugin state semantics.
- **Controller runtime**: the separate OS-level sidecar process that merges provider descriptions, routes control events, evaluates mappings, and renders the controller surface.

Core software components:
1. **Plugin SDK**: enriches plugin-host communication with semantic descriptions and custom mappings.
2. **Host SDK**: provides the native DAW integration path for hardware-aware controller support.
3. **Host add-ons**: host-specific extension modules or add-ons that expose integration paths for external parties, but may only be able to realize a subset of host capabilities due to DAW-specific constraints.
4. **Sidecar runtime app**: the OS-level process that owns mapping evaluation, routing, and the runtime state.
5. **Runtime-to-hardware bridge**: the transport layer that connects the sidecar to actual controller hardware.
6. **Mapping markup format**: the portable schema that describes the controller surface and semantics.

For a plugin to work, the minimum required component is the mapping markup format.
For DAW-level integration, the host must provide either a host SDK or host add-ons (component 2 or 3).

The host and semantic providers are both capability providers. The controller runtime is intentionally a separate OS-level process, enabling a true sidecar architecture that isolates hardware communication and mapping evaluation from the host or plugin process.

## Semantic mapping layer
The semantic provider is a two-tier contract:

1. **Scalar parameter mappings and presentation rules**
   - This tier is host- and plugin-capable because it relies on observable parameter state.
   - It includes parameter mappings, read-only or read-write parameter exposure, display labels, conditional slot selection, enum value labels, and visibility rules.
   - Hosts and plugins can provide this tier through mappings, sidecar metadata, or adapter layers.
   - It is the portable foundation for controller surfaces and can work without deep plugin integration.

2. **Deep plugin-state semantics**
   - This tier is plugin-only and requires a plugin SDK extension.
   - It exposes richer state, custom actions, semantic gestures, browser state, non-parameter UI state, and plugin-specific navigation semantics.
   - Only the plugin can supply this level of contract because it can observe internal plugin state and semantics that the host cannot reliably infer.

A DAW mapping layer may expose a standardized vocabulary for common semantics, but it should still allow the host to provide free-form, host-specific extensions when needed.

Integration can be implemented in any host-supported environment, such as JavaScript, Max for Live, or other host-specific extension systems. For DAW authors who want a deeper native integration path, an optional C++ SDK can be provided by the host, but that is a separate host-author SDK and not required for the portable mapping contract.

1. **Declarative surface description**
   - A `describeControllerSurface()` contract describes how a plugin or mapping wants to appear on controllers.
   - It returns pages, slots, labels, conditions, enums, gestures, meters, preferred controls, and other metadata.

### Declarative metadata example
```text
plugin.describeControllerSurface();
```
Returns:
- pages
- slots
- labels
- conditions
- enums
- gestures
- meters
- preferred controls
- formatting hints
- dynamic slot selection rules

### Live endpoint example
```text
plugin.controllerEndpoint();
```
Allows:
- controller -> plugin:
  - `beginGesture(param)`
  - `adjust(param, delta)`
  - `setNormalized(param, value)`
  - `invokeAction(action)`
  - `selectMode(enumValue)`
  - `browsePreset(delta)`
  - `endGesture(param)`
- plugin -> controller:
  - `valueChanged(param)`
  - `textChanged(param)`
  - `pageInvalidated(page)`
  - `meterChanged(id)`
  - `modeChanged(id)`
  - `mappingChanged()`

## Ownership rules
The ownership split is critical:

- **Host/DAW owns**:
  - automation
  - project persistence
  - undo if supported by host
  - canonical parameter values
  - offline render correctness

- **Plugin semantics layer owns**:
  - rich gestures
  - semantic actions
  - high-rate feedback when available
  - non-parameter UI state
  - contextual mappings
  - custom browser/actions

- **Controller runtime owns**:
  - merging provider data
  - routing control events
  - layout and display evaluation
  - fallback behavior when providers do not cover a target

Direct plugin control must not secretly bypass the host for automatable parameter state.

For automatable parameters, the direct endpoint must either:
- call back into host parameter writes when available,
- mirror the same gesture/value protocol the host sees, or
- mark the action as non-automatable/plugin-private.

Otherwise automation, undo, recall, and host UI sync break.

## Host endpoint component
The host/DAW component is a first-class provider, and its role is broader than just plugin parameter ownership.
It also owns DAW-level control surfaces, transport, mixer state, track/device selection, and project context.

The host endpoint should support:
- canonical parameter writes and automation commits
- host-level parameter control (track levels, send levels, mixer controls)
- transport control and synchronization state
- track/scene/bank navigation and selection
- DAW action invocation (save, undo, track arming, etc.)
- preset browsing if supported by the host
- selected track/device observation
- host-visible state notifications such as track/device/focus changes
- reactive updates for parameter/automation state

The host endpoint and semantic provider are complementary:
- host endpoint = DAW/project-aware lane for canonical parameters, transport, mixer, and track-level control
- semantic provider = host-agnostic lane for surface descriptions, actions, gestures, and rich feedback

The runtime should not treat the semantic provider as special. Both provider types can offer describe/subscribe/begin/adjust/set/invoke/end capabilities.

## Provider model
The runtime should treat DAW/host and semantic providers as capability providers.
Hosts without a native plugin SDK can still participate by offering the same semantics through mappings, sidecar files, or an adapter layer.

```
Controller runtime
├─ DAW endpoint
│  ├─ write parameter
│  ├─ observe selected track/device
│  ├─ write mixer/track/send controls
│  ├─ control transport and session state
│  ├─ invoke DAW actions
│  ├─ browse presets if supported
│  └─ receive reactive updates
└─ Semantic provider
   ├─ describe semantic pages
   ├─ format values
   ├─ invoke semantic actions
   ├─ provide high-rate feedback when available
   ├─ support mapping-only host integration
   └─ receive reactive updates
```

## Hardware communication
Hardware integration can come from either:
- **Host-level SDK**: the preferred path when the DAW exposes controller hardware integration directly.
- **Plugin-layer SDK**: an alternate path when the host does not provide hardware integration.

The plugin-layer SDK may declare custom mappings, but those mappings are still markup and can be sourced externally.

Hardware only needs one integration path: either the DAW host SDK or the plugin-layer SDK. In either case, the mapping markup is the shared contract that describes how the controller should behave.

A dedicated **sidecar process** is required to make this architecture make sense. The sidecar should run as a separate OS-level process and perform the actual communication to/from hardware. It should own the implementation drivers that connect the sidecar runtime to the physical controller and relay host/plugin events to the hardware.

The runtime protocol should also handle multiple DAW instances and any associated conflict resolution, even if that usage is rare.

Host vs plugin authority should be resolved via explicit versioning and capability metadata, letting the sidecar or controller runtime choose the authoritative integration path and gracefully degrade when a newer host or plugin capability is absent.

Both endpoints can implement the same control contract:

- `describe()`
- `subscribe()`
- `beginGesture()`
- `adjust()`
- `set()`
- `invoke()`
- `endGesture()`

The merge/router decides who handles each target.

## Control semantics and routing
Command classes can describe intent:

```cpp
enum class ControlSemantics {
  AutomatableParameter,
  PluginPrivateState,
  MomentaryAction,
  NavigationAction,
  BrowserAction,
  GestureOnly,
  MeterFeedback
};
```

Binding routes declare the control path:

```yaml
slots:
  - parameter: filter.cutoff
    route: host_parameter
    direct_feedback: true
  - action: browser.next_preset
    route: plugin_endpoint
  - action: mod.assign_source
    route: plugin_endpoint
    affects_project_state: true
```

## Mapping file as universal contract
The mapping file is the portable semantic contract.
DAW adapters and plugin endpoints should consume the same schema.
The DAW layer may publish a standard vocabulary for common domain concepts, while still supporting free-form mappings and host-specific vocabularies.

Supported sources:
- built into plugin
- installed sidecar file
- DAW-provided map
- community map
- user map
- runtime-generated fallback

A mapping should cover:
- pages
- slots
- conditions
- labels/value formatters
- enum-dependent remapping
- gestures
- actions
- display hints

## Example mapping fragment
```yaml
plugin:
  match:
    vendor: "Example Audio"
    name: "SemiModular"
parameters:
  osc_mode:
    match: "Osc Mode"
    enum:
      0: Classic
      1: Wavetable
      2: FM
pages:
  - name: Oscillator
    slots:
      - index: 0
        parameter: osc_mode
        label: Mode
      - index: 1
        select:
          - when: param("osc_mode") == "Classic"
            parameter: osc_shape
            label: Shape
          - when: param("osc_mode") == "Wavetable"
            parameter: wt_position
            label: WT Pos
          - when: param("osc_mode") == "FM"
            parameter: fm_ratio
            label: Ratio
      - index: 2
        select:
          - when: param("osc_mode") == "Classic"
            parameter: pwm
            label: PWM
          - when: param("osc_mode") == "Wavetable"
            parameter: wt_warp
            label: Warp
          - when: param("osc_mode") == "FM"
            parameter: fm_amount
            label: FM Amt
```

## Runtime priority rules
Suggested route preferences:
- Automatable parameter write: DAW endpoint preferred.
- Plugin-private action: plugin endpoint preferred.
- Page/label/value formatting: plugin or user map preferred.
- Track/device/session navigation: DAW endpoint preferred.
- Fallback: runtime generic mapping.

## Architecture summary
- Mapping schema = universal integration language.
- DAW adapter = executes mappings using host-visible state.
- Plugin SDK = supplies mappings and optionally exposes extra state/actions.
- Controller runtime = merges, evaluates, lays out, renders.

Do not encode plugin intelligence only in DAW scripts.
Do not encode controller intelligence only in plugin SDKs.
Put semantic intelligence in portable mapping files.

## Research topics
### Vendor-specific runtime accommodation
- AKAI VIP: investigate whether VIP can expose an external control interface or mapping bridge that the runtime can drive, and whether it can be extended from the OS-level sidecar rather than only from a VIP host plugin.
- Nektar Panorama: research Panorama's integration API and whether its custom host templates or MIDI/DAW mappings can be generated or driven from the sidecar runtime while preserving its proprietary layout semantics.
- Komplete Kontrol: determine how Komplete Kontrol exposes DAW/host integration (e.g. via NKS, DAW templates, or host scripts) and what runtime-level hooks are needed to synchronize Komplete Kontrol's own browser/transport/mapping state with our controller runtime.

### Runtime communication model
The runtime should be able to communicate with each integration path through explicit channels and fallbacks:
- Plugin SDK channel: used when a plugin is present and can provide mapping markup, rich semantics, and custom actions. This channel should support describe/subscribe/begin/adjust/set/invoke/end and should expose enough metadata for the runtime to merge plugin-driven mappings with host state.
- Host SDK channel: used when the DAW provides a native controller integration API. This channel should expose canonical parameter writes, transport/navigation, mixer and track state, action invocation, and host-aware notifications.
- Host add-on channel: used when the DAW exposes an external extension or add-on mechanism. This channel may be less standardized, may only support a subset of host capabilities, and should be treated as an adapter layer that translates host add-on callbacks/events into the runtime's capability provider model.

### Communication constraints
- Versioning and capability negotiation: every channel must advertise version and capability metadata so the runtime can choose the best authoritative source and fall back when a newer capability is absent.
- Conflict resolution: the runtime must decide which provider owns each control target, especially when plugin and host both expose the same parameter or action.
- Latency and rate: host SDK and plugin SDK communication should support both command and high-rate feedback paths, but the runtime must gracefully degrade on hosts that only offer lower-rate updates.
- Process separation: the runtime is a separate OS-level process, so inter-process transport must be reliable, authenticated, and able to multiplex multiple DAW instances or host sessions.
- Free-form host adapter support: host add-ons may expose only partial or custom semantics, so the runtime must allow free-form mappings and explicit vendor-specific extensions while preserving the portable mapping contract.
- Consistency: out-of-box controller experience should remain consistent by preferring host SDK integration when available, with plugin mappings or host add-on adapters as fallback sources.

### DAW-specific integration research
- FL Studio: investigate FL Studio's native controller scripting and MIDI remote support, and whether the sidecar can drive templates or mappings through wrapper scripts and shared state.
- Bitwig: research Bitwig's controller API and JavaScript-based controller scripts, which may offer a strong host SDK path and a good model for sidecar integration via external scripts or adapters.
- Ableton Live: determine how Live's Python MIDI Remote Scripts and Max for Live devices can be used as host add-ons or adapter layers to surface runtime mappings and transport/state events.
- Reaper: assess Reaper's extensibility through ReaScript, JSFX, and extension APIs, which may allow a flexible host add-on approach or a lightweight bridge from the runtime to host state.
- Custom/web-based hosts: consider browser-based DAW-style environments or web-hosted control panels, focusing on how the runtime can communicate with them through WebSockets, Web MIDI, browser extensions, or embedded JavaScript adapters.
