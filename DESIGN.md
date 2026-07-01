# hacpad Design: Host-Agnostic Semantic Control

This design centers on two big pieces:

- **Mapping markup**: the portable description of how DAW/plugin parameters, actions, gestures, and feedback map to actual controller hardware.
- **Hardware communication**: the transport path between software and controller hardware, which can be delivered either through a host-level SDK or through a plugin-layer SDK.

When both are available, host-level SDK integration takes precedence. The plugin layer can still declare custom mappings, but those mappings are always expressed as markup and can be sourced externally to the plugin.

Host vs plugin capability resolution should be negotiated via versioning and capability metadata, so the runtime can choose the most authoritative source and fall back cleanly.

Supporting pieces:
- **Host endpoint**: the DAW/host provider that owns canonical parameter state, automation, persistence, and undo.
- **Semantic provider**: a portable contract that describes plugin control surfaces, actions, gestures, and rich feedback.
- **Controller runtime**: the runtime/provider router that merges provider descriptions, routes control events, evaluates mappings, and renders the controller surface.

The host and semantic providers are both capability providers. The controller runtime is intended to run as a separate OS-level process, enabling a true sidecar architecture that isolates hardware communication and mapping evaluation from the host or plugin process.

## Semantic mapping layer
The semantic provider is the portable contract. A host without a native plugin SDK can consume the same semantics through mapping files, sidecar metadata, or an adapter layer.
The mapping markup is the core contract, and it can be sourced from the plugin, from external sidecars, or from the host.

1. **Declarative surface description**
   - A `describeControllerSurface()` contract describes how a plugin or mapping wants to appear on controllers.
   - It returns pages, slots, labels, conditions, enums, gestures, meters, preferred controls, and other metadata.

2. **Live control channel (optional)**
   - `controllerEndpoint()` exposes a realtime-ish command/event channel when a plugin directly supports it.
   - For hosts without a native plugin endpoint, the same semantics can still be delivered by host-side actions and mapping-driven gesture routing.

### Declarative metadata example
```cpp
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
```cpp
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

A dedicated **sidecar process** is required to make this architecture make sense. The sidecar should run as a separate OS-level process, acting as the bridge between controller hardware and the runtime, abstracting transport details, handling firmware/protocol versions, and enabling host or plugin transport negotiation.

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
