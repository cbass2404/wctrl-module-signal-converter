# Config and UI model

## The shape of it

**The LED is the primary key.** A profile is not a list of interesting signals that
happen to drive lamps; it is the device's lamp inventory, each with an answer to
"what drives this?". The UI is therefore a fixed list of every LED the connected
hardware has, and the user fills in the ones they care about.

One profile per aircraft. Aircraft are matched on the names DCS reports at
runtime, so a single profile can serve variants.

Any readable signal may be a source, **not only lamps**. In the AH-64D, 400 of its
708 signals are switch selectors against 49 lamps. A user binding the A/G lamp to
the Ground Override _pushbutton state_ is a first-class case, not a workaround.

```text
Aircraft: AH-64D_BLK_II                              Devices: PTO2, Orion II

  Orion Throttle Base II
  ┌────────────────┬──────────────────────────────────┬──────────────┬────────┐
  │ LED            │ Driven by                        │ When         │ Output │
  ├────────────────┼──────────────────────────────────┼──────────────┼────────┤
  │ Panel backlight│ PLT_INT_LIGHT_PRIMARY        ▾   │ scale        │ 0-255  │
  │ A/A            │  none                      ▾   │              │        │
  │ A/G            │ PLT_GROUND_OVERRIDE_BTN      ▾   │ = 1          │ 255    │
  └────────────────┴──────────────────────────────────┴──────────────┴────────┘
```

## Profile file

```jsonc
{
  "schema_version": 1,
  "name": "AH-64D  Ground override on A/G",
  "author": "coryb",
  "profile_version": "1.0.0",
  "aircraft": ["AH-64D_BLK_II"],
  "module": "AH-64D", // catalogue key the signal ids resolve against

  "bindings": [
    {
      "device": "Orion_Throttle_Base_II",
      "led": "A/G",
      "source": "PLT_GROUND_OVERRIDE_BTN",
      "on_when": { "equals": 1 },
      "on": 255,
      "off": 0,
    },
    {
      "device": "Orion_Throttle_Base_II",
      "led": "Backlight",
      "source": "PLT_INT_LIGHT_PRIMARY",
      "on_when": { "scale": [0, 65535] },
    },
    {
      "device": "TAKEOFF_PLANEL_2",
      "led": "Master_Caution",
      "source": "PLT_MASTER_CAUTION_L",
      "on_when": { "equals": 1 },
      "on": 1,
      "off": 0,
    },
  ],
}
```

An LED with no binding is simply absent from the file. The daemon still owns it.

## Module load: one sweep, not a reset

On aircraft change the daemon walks **every LED on every connected device** once
and writes its computed value, using `0` for any LED the profile does not bind.
Configured lamps land on their correct state and unconfigured ones go dark, in a
single pass.

This replaces an earlier reset-then-sync design. The cost difference is trivial
around twenty extra writes once per mission load but the single sweep avoids a
visible blackout as every lamp drops to zero and comes back, and it is one
operation rather than two code paths doing one job. The same sweep is reused for
resync and for recovery after an error.

**The state is there when we need it.** DCS-BIOS sends deltas, but on aircraft
change `BIOSStateMachine` calls `memoryMap:clearValues()`, marking every entry
dirty, so the module's entire state is re-sent immediately after load. The daemon
waits for that flood to settle before sweeping, then switches to writing
individual LEDs as values change.

One caveat: a daemon started **mid-flight** misses the flood and begins with an
incomplete picture, so lamps whose signals happen not to change will be wrong
until they do. The honest fixes are to start before entering the aircraft, or to
offer a manual resync that waits for the next aircraft change.

## Picking the signal, then the value

Two controls per LED: **which signal**, then **which value of that signal turns
the light on**. The second is constrained by the first the editor only ever
offers values the selected signal can actually report, so an unsatisfiable
binding cannot be authored.

The catalogue carries what is needed to enforce that. Each output has a
`max_value`, and where the range is small it also carries a `values` list with
labels taken from the definition's `positions` array and from the value legend in
its description:

```jsonc
"outputs": [{
  "address": 32872, "mask": 49152, "shift": 14,
  "max_value": 2, "discrete": true,
  "values": [
    { "value": 0, "label": "DISABLE (Down)" },
    { "value": 1, "label": "Off (Mid)" },
    { "value": 2, "label": "ENABLE (Up)" }
  ]
}]
```

This covers most of what a user will touch. In the AH-64D, **558 of 708 signals
are discrete** and get a labelled dropdown; the remaining 150 are dials and gauges
and get a numeric input clamped to `0..max_value`.

### Condition forms

| `on_when`                    | Offered for | Editor control                              |
| ---------------------------- | ----------- | ------------------------------------------- |
| `{ "equals": 1 }`            | discrete    | dropdown of labelled values                 |
| `{ "in": [1, 2] }`           | discrete    | multi-select of labelled values             |
| `{ "gte": 32768 }`           | continuous  | number input, clamped to the signal's range |
| `{ "between": [100, 4000] }` | continuous  | two number inputs, clamped                  |
| `{ "scale": [0, 65535] }`    | continuous  | no threshold; brightness tracks the value   |

`scale` is the exception that has no on/off value at all it is for backlights
and dimmers, where the lamp should follow the source continuously rather than
switch at a point.

### Continuous sources and backlights

Cockpit dimmers are first-class sources. Every module exposes its lighting
potentiometers as continuous signals with a 0..65535 range, which `scale` maps
onto a lamp's 0..255:

```jsonc
{
  "device": "TAKEOFF_PLANEL_2",
  "led": "Backlight",
  "source": "PLT_INT_LIGHT_INSTRUMENT_PANEL",
  "on_when": { "scale": [0, 65535] },
}
```

**Prefer the gauge over the knob.** Modules commonly publish both, and they are
not the same signal:

| Signal                                      | `control_type` | What it reports                        |
| ------------------------------------------- | -------------- | -------------------------------------- |
| `PLT_INT_LIGHT_INSTRUMENT_PANEL_BRIGHTNESS` | `limited_dial` | where the pilot set the knob           |
| `PLT_INT_LIGHT_INSTRUMENT_PANEL`            | `analog_gauge` | how brightly the panel is actually lit |

Binding the knob leaves the panel backlight glowing with the battery off, because
the knob keeps its position through a cold start. The gauge follows actual
illumination. The editor should therefore sort `analog_gauge` above
`limited_dial` for backlight LEDs and say why, rather than leaving users to
discover it in a dark cockpit.

Two refinements deliberately left for later: a response curve, since LED PWM is
linear while perceived brightness is not, and a floor/ceiling clamp so a lamp can
be kept readable at the dim end. Neither blocks v1.

One contention note: backlight is the one LED SimAppPro may also drive, via the
per-device "Sync with DCS" mode its DCS integration gates on
(`DCSAPULight.js` applies that gate to `Backlight`, `INST_PNL_Backlight` and
`Screen_Backlight` only). If a user runs both, the two will fight over that lamp
and the last writer wins. Detect it and warn rather than silently flickering.

### Output brightness

Constrained by the **LED**, not the signal: `data/devices.json` records each
lamp's `max`. A dimmable lamp offers 0–255; `Master_Caution`, recorded as `max: 1`,
offers only on/off. The editor should not present a brightness slider for a lamp
that cannot dim.

### Sensible defaults

The editor picks from the catalogue rather than interrogating the user. A signal
with `max_value == 1` defaults to `equals: 1` at full brightness. A wide
continuous signal onto a dimmable lamp defaults to `scale`. Both are one click to
change; the point is that choosing a signal should usually be the only step.

## Blink comes from the source

Where a DCS lamp flashes F/A-18 gear in transit, for instance the module's own
argument is oscillating, and mirroring it reproduces the flash. There is no blink
setting to configure for those cases, and none should be offered, or users will
apply it on top of an already-blinking source and get a beat frequency.

A synthetic blink belongs only where DCS does not already express one. It is a
later addition, not part of v1.

## The source dropdown

This is the hard part of the UI. Modules carry hundreds to 1,440 signals, so a
plain `<select>` is unusable. It needs:

- **Search** across identifier and description.
- **Grouping** by the catalogue's category.
- **Ordering** that puts likely intent first: lamps, then selectors, then the rest.
- **Inline context** identifier, description, control type, value range because
  `PLT_WCA_HOOK_DOWN` alone does not tell a user it is a red lamp with range 0..1.
- **Learn mode.** With DCS running, the user flips the switch in the cockpit and
  the editor shows which signals just changed, filtering the dropdown to those.
  This is the feature that makes unfamiliar modules tractable and is worth more
  than any amount of search polish.

## One source per LED, by default

A single dropdown per LED is the model. It covers the overwhelming majority of
bindings and keeps the config readable.

Some cases genuinely need more than one a gear lamp that should light only when
all three gear signals agree. The intended escape hatch is an "add condition"
affordance that promotes `source` to a small `any`/`all` list, kept out of the
default path so the simple case stays simple:

```jsonc
{
  "device": "TAKEOFF_PLANEL_2",
  "led": "Landing_gear_lights",
  "all": [
    { "source": "GEAR_NOSE", "map": { "mode": "equals", "value": 1 } },
    { "source": "GEAR_LEFT", "map": { "mode": "equals", "value": 1 } },
    { "source": "GEAR_RIGHT", "map": { "mode": "equals", "value": 1 } },
  ],
  "on": 255,
  "off": 0,
}
```

## Open questions

1. **Profile inheritance.** Should a profile be able to extend a base, so a
   generic "gear and flaps" profile can be specialised per module? Powerful for
   sharing, but it complicates the editor and conflict resolution. Leaning no for
   v1.
2. **Unbound LEDs.** Clearing everything at mission start is proposed above. The
   alternative is leaving unbound LEDs untouched so another tool can own them.
   Clearing is more predictable; leaving alone is more cooperative.
3. **Per-LED ranges.** `Master_Caution` is treated as 0/1 because SimAppPro
   presents it as a toggle rather than a slider. Whether the firmware accepts
   intermediate values is untested see `data/devices.json`.
