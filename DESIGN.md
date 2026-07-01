# hacpad Design: Plugin SDK and Control Providers

This design centers on three cooperating pieces:

- **Host endpoint**: the DAW/host provider that owns canonical parameter state, automation, persistence, and undo.
- **Plugin endpoint**: the plugin provider that exposes semantic surface descriptions, plugin-private actions, rich gesture handling, and high-rate feedback.
- **Controller runtime**: the runtime/provider router that merges provider descriptions, routes control events, evaluates mappings, and renders the controller surface.

The host and plugin endpoints are both capability providers, while the controller runtime is the orchestrator that keeps host-aware parameter changes safe and still allows richer plugin-driven behavior.

## Plugin SDK split
The SDK should be split into two complementary surfaces:

1. **Declarative surface description**
   - `plugin.describeControllerSurface()` describes how the plugin wants to appear on controllers.
   - It returns pages, slots, labels, conditions, enums, gestures, meters, preferred controls, and other metadata.

2. **Live control endpoint**
   - `plugin.controllerEndpoint()` exposes a realtime-ish command/event channel.
   - This channel is for semantic control, feedback, and plugin-private actions.

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

- **Plugin endpoint owns**:
  - rich gestures
  - semantic actions
  - high-rate feedback
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
The host component is a first-class provider in the model. It can expose DAW-specific capabilities and react to host-visible state changes, while the plugin endpoint supplies semantic enrichment.

The host endpoint should support:
- canonical parameter writes and automation commits
- selected track/device observation
- DAW action invocation
- preset browsing if supported by the host
- host-visible state notifications such as track/device/focus changes
- reactive updates for parameter/automation state

The host and plugin endpoints are complementary:
- host endpoint = canonical/project-aware lane
- plugin endpoint = semantic/private/enrichment lane

The runtime should not treat the plugin endpoint as special. Both provider types can offer describe/subscribe/begin/adjust/set/invoke/end capabilities.

## Provider model
The runtime should treat DAW and plugin endpoints as capability providers.

```
Controller runtime
├─ DAW endpoint
│  ├─ write parameter
│  ├─ observe selected track/device
│  ├─ invoke DAW action
│  ├─ browse presets if supported
│  └─ receive reactive updates
└─ Plugin endpoint
   ├─ describe semantic pages
   ├─ format values
   ├─ invoke plugin-private actions
   └─ receive reactive updates
```

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
