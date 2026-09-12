# PANORAMA_P1.control.js — driver deobfuscation working notes

Standing, multi-session task (checklist item 12). Goal: exhaustively understand every SysEx and MIDI
call site in Bitwig's own shipped Panorama P1 driver. Updated incrementally across loop cycles — do not
try to finish this in one pass.

**Source file**: `PANORAMA_P1.control.js`, from Nektar's official
`Panorama_P1_P4_P6_Bitwig_Studio_Integration_Files_Linux_2023-06` package (bundled with Bitwig Studio),
found on this machine via `find / -iname PANORAMA_P1.control.js` at:
```
~/Downloads/support_871eda6c83a540c70/Panorama_P1_P4_P6_Bitwig_Studio_Integration_Files_Linux_2023-06/Nektar/PANORAMA_P1.control.js
```
508 lines, ~233KB, minified but not obfuscated beyond identifier renaming (short symbol names, no
control-flow flattening or string encryption observed so far).

## Methodology

Two complementary approaches (per explicit direction — use whichever makes progress on a given cycle):

**(a) Semantic deobfuscation** — work OUTWARDS from known, already-readable Bitwig Controller API
anchors (`transport.*`, `application.*`, `host.*`, `cursorTrack.*`, `cursorDevice.*`, the
`sendSysex`/`sendChannelController`/`sendMidi` wrappers, numeric `DISPLAY_ID`/`pageTemplate` literals
already confirmed on real hardware), and use each anchor's well-documented real behavior to infer/rename
the obfuscated wrapper functions and state variables immediately around it. This is the mirror image of
the old "panomod" project's approach (which started from known plugin/CC strings and renamed forward
until structure emerged) — here we start from the SDK/output side and work backward/inward.

**(b) Structural precondition graph** — when semantic naming is too slow or ambiguous, just map WHICH
obfuscated boolean/state variables and object fields gate a given branch (read-sites), and WHERE each
such variable gets written (write-sites: MIDI-input-reachable `onMidi`/`onPad`/`onScreenButton`/`onView`
handlers vs. DAW-internal `addValueObserver` callbacks only, which we cannot trigger from a bare MIDI
replay). This doesn't require real names for anything — it just tells us what has to be "built up" to
reach a given `sendSysex` call.

**Key structural insight** (confirmed): the obfuscation is NOT uniform across the file. The top-level
Bitwig Controller API glue is already in **plain, readable names** (`transport`, `application`,
`cursorTrack`, `cursorDevice`, `host`, `groove`, `cursorClip`, `masterTrack`, `sceneTrackBank`, `tracks`,
and dozens of `.setter()`-pattern state names like `transportPosition`, `punchIn`, `overdub`,
`launcherOverdub`, `cursorTrackPanText`). Only Nektar's own custom display/page/menu/pad logic layered
on top of that API — exactly the part most relevant to this project — uses opaque `Z81[A-F0-9]+`
hash-style identifiers. **317 distinct such identifiers** counted (`grep -oE '\bZ81[0-9A-F]+\b' | sort -u
| wc -l`). This means the highest-value moves are exactly (a)/(b) above, not a blanket rename pass.

## Confirmed renames / identities (high confidence — direct code evidence)

| Obfuscated identifier | Real identity | Evidence |
|---|---|---|
| `Z81136C4863E8BC4AF` | `sendSysex` wrapper | `function Z81136C4863E8BC4AF(a){sendSysex(a)}` (line 12) |
| `Z81134B25E3E8D3DA8` | `sendChannelController` wrapper | `function Z81134B25E3E8D3DA8(a,b,c){sendChannelController(a,b,c)}` (line 12) |
| `Z81135D0703E8C9884` | `sendNoteOn`-ish wrapper (144=note-on status) | `function Z81135D0703E8C9884(a,b,c){sendMidi(144+a,b,c)}` (line 12) |
| `Z8114CB0CF3E757C0C` | the `pageTemplate` enum object | Values `{0,1,2,3,4,5,16,17,18,19,20,21,22}` match our confirmed set exactly (line 10) |
| `Z8114CB0CF3E757C0C.Z8107F5DAE3F42EDA6` | `pageTemplate.MESSAGE` (=1) | Used directly in `writeMessageToDisplay` |
| `Z8114CB0CF3E757C0C.Z8107EBC8F3F43D1BA` | `pageTemplate.RESET`? (=0) | Used in `resetOutputToUnknown` |
| `Z8114CB0CF3E757C0C.Z8108A6A733F37DC2B` | template id 22 (drum_pads_3row) | Gates `padState`/`padValue` sends alongside id 21 |
| `Z8114CB0CF3E757C0C.Z8108982993F383C94` | template id 21 (drum_pads) | ditto |
| `DISPLAY_ID` (already plain in source!) | the `displayId` enum object | Values `{0,1,2,3,4,5,6,7,8}` match our confirmed set exactly (line 11) — object NAME itself is not obfuscated, only its member keys are |
| `DISPLAY_ID.Z8106468A33F5D8673` | displayId 2 (big_font / currentParameterInfo) | Sent from `Z810A716763F1A2A63` (see below) |
| `DISPLAY_ID.Z81065F7C93F5CCEFB` | displayId 3 (current_parameter_value) | Sent from `Z810A88C163F19D2AD` (see below) |
| `DISPLAY_ID.Z81063D5073F5E13C3` | displayId 1 (title_bar) | `this.titleBar`, 3 entries |
| `DISPLAY_ID.Z8106606B03F5BB688` | displayId 4 (menu_button) | `this.menuButtonLabel`/`menuButtonType` |
| `DISPLAY_ID.Z81067491F3F5A798E` | displayId 5 (page_labels) | `this.pageLabels` |
| `DISPLAY_ID.Z81068C8BA3F599C21` | displayId 6 (ctrl_element_name) | `this.ctrlElementName` |
| `DISPLAY_ID.Z81069924A3F587F47` | displayId 7 (ctrl_element_value / shared with faderElementValue) | `this.ctrlElementValue` (idx 1-8) AND `this.faderElementValue` (idx 9-16) |
| `DISPLAY_ID.Z8106282CD3F5F688E` | displayId 0 (pad_state) | `this.padState`, gated to templates 21/22 only |
| `Z810A716763F1A2A63` | `currentParameterInfo` (the real internal name for our "big_font" field's content) | assigned all over the transport/menu handlers as a human-readable status string, flushed to `DISPLAY_ID.2` |
| `Z810A88C163F19D2AD` | `currentParameterValue` (real name for "paramval"/displayId 3's content) | same pattern, flushed to `DISPLAY_ID.3` |
| `Z8114AF1453E779F61` | `menuButtonType` enum, 2 values | `{Z8106B47793F562F35:0 (used for "Launcher"), Z8106CC3A83F552289:1 (used for "Groove")}` — likely `{NAVIGATE:0, TOGGLE:1}` or similar, not yet certain which is which |
| `Z8114B1CC53E76CFF0` | clip/pad-state enum, 9 values (0-8) | used for `padState[a]` in clip-launcher contexts (recording/queued/playing/etc) |
| `CC.Z8102FB57F3F92C1E7` | CC 16 (trackButtonLed base, confirmed = select/track LEDs) | `=16` in the `CC` enum; matches our own `led_test.rs` CC 16-23 exactly |
| `CC.Z8104568B03F7C4D79` | CC 106 (menuButtonOn base) | `=106` in the `CC` enum; matches our own `led_test.rs` CC 106-110 exactly |
| `gBrowserOpen` (already plain) | browser-panel-open flag | read/written throughout the 0x0B call sites |
| `Z810DEA1F33EE35023` | a page-object with `.onView`/`.onPad`/`.locked`/`.selectedScene`/`.onScreenButton` | `.onView` sends the "Launcher" 0x0B string — candidate: the Launcher/Scenes page object |
| `Z810D62D323EEB8CFB` | a page-object with `.onPad`/`.onScreenButton`/`.patches`/`.categories`/`.creators`/`.macrosInView`/`.patchMenuOpen` | candidate: the Instrument/Device (browse patches, macros) page object |
| `Z810F82B653EC90957` | `new DisplayPage` with `.playingStep`/`.stepSet` | candidate: the step-sequencer page object |
| `Z810EDBF443ED4E1C4` | page-object with `.encSubPage`, referencing `Z8106E8AB83F533169.MACRO` | candidate: encoder/macro page object |
| `Z8106E8AB83F533169` | enum `{MACRO, SENDS}`-shaped (2+ values) | `encSubPage` enum |
| `Z8107103393F50CCA0` | enum with `.ENCODER`, `.PADS` | a `subPage` enum |
| `Z810707FF73F519DC2` | enum with `.CLIPS`, `.SCENES` | a `padsSubPage` enum |
| `Z8114DFD213E74EEB9` | **CONFIRMED: Shift button state** | Write-site found: `case CC.Z8103EE82A3F8314DF: Z8114DFD213E74EEB9=0<e; ...` (in the incoming-CC dispatch, `Z8103EE82A3F8314DF`=CC 96). Read-site gates alternate Play/Stop/Record/Loop behavior everywhere, exactly the "hold for alternate function" pattern. Not yet re-triggered live via holding physical Shift to watch the alternate-button behavior fire, but the flag identity itself is now certain. |
| `Z810D28CA53EEF2221` | a number, compared with `1<` to pick between two 0x0B selector bytes for the same button | candidate: a menu/category "depth" or "zoom" counter, not yet named with confidence |
| `Z81048047C3F79F700` | **CC 109, "screen button 3"** — confirmed live (Twenty-eighth finding) to be the popup-menu **exit/cancel** button: `case CC.Z81048047C3F79F700: ...if(menuHandler.isActive) menuHandler.onMenuCancel(); else ... onScreenButton(3)` | Pressed physically while capturing raw input; matches source exactly |
| `Z8104967483F781F8A` | **CC 110, "menu enter"** — confirmed live: `case CC.Z8104967483F781F8A: if(menuHandler.isActive) menuHandler.onMenuEnter();` | Pressed physically directly after the exit test; matches source exactly |
| `Z8103EE82A3F8314DF` | **CC 96 — Shift** (see `Z8114DFD213E74EEB9` above) | write-site for the Shift flag |
| `Z8106041EB3F613A4B` | **CC 103 — Mode** button: `case CC.Z8106041EB3F613A4B: 0<e?(setActiveDisplayPage(internalPage),...):setActiveDisplayPage(Z810D7CD223EEA2020)` | plausibly the physical Mode button tied to the "Internal mode" bypass from the Fifth finding — not yet cross-checked live |
| `Z810AD4A753F1454A0` | the currently-active page/view object (dispatch target for `onScreenButton(n)` and `.onRemoveMenu()`) | referenced constantly alongside `menuHandler`/`SurfaceStatus` state; candidate: `currentPage` |

## Open threads / next steps

- The `menuButtonType` entry (index 0, length 5, raw enum bytes) has never been replicated on hardware —
  worth a direct test: send `displayId 4` with an index-0 entry (`00 05 <5 bytes of 0/1>`) alongside the
  normal 5 labels, see if button rendering changes (border/style difference between type 0 vs 1).
  Directly relevant to checklist item 5 (index-0 compose entries) and item 7 (menu button type flag).
- Still haven't identified write-sites for `Z810D28CA53EEF2221`, `Z810D572733EEC9820`,
  `Z810D313AB3EEE3590`, `Z810F2699B3ECF37A3`, `Z810CF61D13EF26209` and several other flags that gate the
  0x0B call sites — needed to actually reproduce the state those calls expect, if we want to see them do
  something visible.
- Page-object identities (`Z810DEA1F33EE35023`, `Z810D62D323EEB8CFB`, `Z810F82B653EC90957`,
  `Z810EDBF443ED4E1C4`, `Z810FF3DBF3EC21DB5`) are plausible but not yet cross-checked against which
  physical page/view each corresponds to on hardware (e.g. does switching to template 21 on real
  hardware correspond to whichever page-object's logic sets `pageTemplate=Z8114CB0CF3E757C0C.Z8108982993F383C94`?
  — likely yes, given `Z810DEA1F33EE35023`/similar set `b.pageTemplate=...template21or22...` in a
  clip/pad context, but not confirmed line-by-line).
- `Z8114AF1453E779F61`'s two values (menuButtonType) — which is 0 and which is 1 semantically (i.e. is
  0 "navigate to a page" and 1 "toggle a boolean", or the reverse)? "Launcher" (a page-nav action, tied
  to `.onView`) uses value 0; "Groove" (`groove.getEnabled().set(...)`, a toggle) uses value 1. Tentative:
  `{PAGE_NAV: 0, TOGGLE: 1}`, not yet independently verified against a third example.
