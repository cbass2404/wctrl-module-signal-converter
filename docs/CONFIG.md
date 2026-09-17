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
      "conditions": [
        { "source": "PLT_GROUND_OVERRIDE_BTN", "on_when": { "equals": 1 } },
      ],
      "on": 255,
      "off": 0,
    },
    {
      "device": "Orion_Throttle_Base_II",
      "led": "Backlight",
      "conditions": [
        { "source": "PLT_INT_LIGHT_PRIMARY", "on_when": { "scale": [0, 65535] } },
      ],
    },
    {
      // No conditions is a placeholder: the lamp is listed but not configured.
      // A normal state, not an error. It is swept off like any unbound lamp.
      "device": "TAKEOFF_PLANEL_2",
      "led": "Master_Caution",
      "conditions": [],
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

**The state is there when we need it.** On aircraft change `BIOSStateMachine`
calls `memoryMap:clearValues()`, marking every entry dirty, so the module's
entire state is re-sent immediately after load. The daemon waits for that flood
to settle before sweeping, then switches to writing individual LEDs as values
change.

**A daemon started mid-flight syncs too.** This was written the other way round,
on the assumption that the stream carries deltas only and the aircraft name is a
one-shot on module load. Measured 2026-09-16: word 0 of `_ACFT_NAME` arrived 67
times in 20 seconds, roughly every 300 ms, so DCS-BIOS re-exports on a cycle. A
daemon started mid-mission sees the name almost immediately, treats it as an
aircraft change because it had none, and sweeps once the stream settles. No
re-slot, no mission restart, and no manual resync command is needed.

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

A response curve is still left for later, since LED PWM is linear while
perceived brightness is not. It does not block v1.

**A floor at the dim end already exists**, and it needs no new field. `scale`
resolves a zero source to zero, and a binding that resolves to zero takes its
`off` value, so `off` is the value at the bottom of the scale:

```jsonc
{
  "device": "TAKEOFF_PLANEL_2",
  "led": "FLAG",
  "conditions": [{ "source": "LCP_CONSOLE", "on_when": { "scale": [0, 65535] } }],
  "off": 255,
}
```

Both shipped profiles use exactly that for `FLAG`, the dimmer over the PTO2's
seven flag lamps. Console lights off means daylight, not lamps off, so the flags
go full bright rather than dark. `Backlight` deliberately does not get the same
treatment: unlit panel labels in daylight are correct.

One contention note: backlight is the one LED SimAppPro may also drive, via the
per-device "Sync with DCS" mode its DCS integration gates on
(`DCSAPULight.js` applies that gate to `Backlight`, `INST_PNL_Backlight` and
`Screen_Backlight` only). If a user runs both, the two will fight over that lamp
and the last writer wins. Detect it and warn rather than silently flickering.

### Output brightness

Constrained by the **LED**, not the signal: `data/devices.json` records each
lamp's `max`. A dimmable lamp offers 0-255; `Master_Caution`, recorded as `max: 1`,
offers only on/off. The editor should not present a brightness slider for a lamp
that cannot dim.

### Sensible defaults

The editor picks from the catalogue rather than interrogating the user. A signal
with `max_value == 1` defaults to `equals: 1` at full brightness. A wide
continuous signal onto a dimmable lamp defaults to `scale`. Both are one click to
change; the point is that choosing a signal should usually be the only step.

### Matching another lamp

`same_as` points one lamp at another on the same device, and it follows whatever
that lamp resolved to:

```jsonc
{ "device": "TAKEOFF_PLANEL_2", "led": "FLAG", "same_as": "Backlight", "off": 255 }
```

This is a link, not a copy. The PTO2 is the case it exists for: it carries three
independent brightness governors that are usually meant to sit at one level, and
writing the same conditions into all three means every later change has to be
made three times or they drift apart without anyone noticing.

**Only between lamps that dim, on both ends.** An indicator takes 0 or 1, so it
has no level to follow and none to offer; mirroring one either way would be a
setting that cannot mean what it says. `validate` rejects it and the editor
offers the option only on a dimmer with another dimmer to point at.

**The mirroring lamp keeps its own `off`.** The value is taken from the target,
clamped to what this lamp accepts, and this lamp's `off` applies when that value
is zero. So `FLAG` can follow the backlight through the night and still go full
bright when the console knob reaches zero, which is the daylight floor that kept
the flap and hook lamps readable. Without that, syncing `FLAG` to the backlight
would reintroduce exactly the fault that made those lamps look dead.

The same caution applies to `SL`, harder. `SL` is a hard gate over all 14
indicators, so pointing it at the backlight with no `off` means every lamp on the
panel goes out whenever the console knob is down, which in daylight is always.
Give it a floor, or leave it at `"always": true`.

**Chains are not allowed.** The target must read signals of its own, which rules
out cycles with no cycle detection to get wrong. A mirroring lamp reads nothing
directly, so the engine indexes it under its target's addresses; otherwise it
would be written once by the sweep and then never follow anything.

`same_as` is mutually exclusive with `conditions`, `any_of` and `always`.

## Blink comes from the source

Where a DCS lamp flashes F/A-18 gear in transit, for instance the module's own
argument is oscillating, and mirroring it reproduces the flash. There is no blink
setting to configure for those cases, and none should be offered, or users will
apply it on top of an already-blinking source and get a beat frequency.

A synthetic blink belongs only where DCS does not already express one. It is a
later addition, not part of v1.

## The editor window

Nothing fancy. A profile list with new, edit and reset, and one profile open at
a time.

**New profile asks for a module from a dropdown, never a typed name.** The list
comes from the catalogue index, so it offers only what the user's own DCS-BIOS
supports. A module can cover several runtime aircraft names, `A-10C` covering
both `A-10C_2` and `A-10C`, so the entry shows the names underneath it. The new
profile is then populated with every LED of every inventoried device, all
unassigned, which is the same starter the CLI writes.

**One collapsible section per device**, ordered alphabetically by `display_name`
and all collapsed on open, with a single control to expand and collapse
everything. Ordering is done once when the inventory loads, not per render and
not by hand in `devices.json`: that file is edited by hand, so a sort order
maintained there would drift the first time a device was appended at the bottom.
It also has to be `display_name` rather than the key, because `PTO2` and
`TAKEOFF_PLANEL_2` do not sort the same way and the user only ever sees one of
them.

**Every inventoried device is listed, not only the connected ones**, with
connected ones marked. A profile has to be editable with the panels unplugged,
which is most of the time.

## Where profiles live

Shipped profiles are a product, not a sample. They live read-only in `DEFAULT/`
beside the executable. The folder the daemon and editor actually read is a
separate, writable one, and it starts as a copy of `DEFAULT/`.

- **Install** copies every default in.
- **Update** copies in only the names that are not already there. A profile the
  user has is theirs, and an update never rewrites it.
- **Reset** copies one default back over the active file.

There is exactly one folder in use, so what a user sees in it is what runs.
Nothing is shadowed at load time and `--profiles` keeps pointing at one place.

The cost, accepted deliberately: a correction shipped to a default never reaches
a user who already has that profile, including one who never opened it. Reset is
the manual remedy. A profile the user deletes reappears on the next update
unless the seeded names are tracked.

## The source dropdown

This is the hard part of the UI. A module carries hundreds to 1,440 signals, so
a plain `<select>` is unusable. Only the profile's own module is offered; the
catalogue is never pooled across modules.

**It is a typeahead, not a list.** Nothing is shown until three characters are
typed. The match runs over description, category and identifier.

Each row is two lines plus a dim identifier:

```text
  Call Button Light (Yellow)
  Gunner (L) Low Profile Audio Panel          LG_LPCAP_CALL_LIGHT
```

The second line is not decoration. Descriptions are human-readable but far from
unique: every one of the 21,644 signals across the 51 catalogued modules has a
description, yet CH-47F alone has 696 signals that share one with another signal
in the same module. `Call Button Light (Yellow)` occurs six times there,
separated only by category: PLT, CPLT, Gunner (L), Gunner (R), Ramp and TC.
Description with category leaves 143 signals ambiguous across everything, 0.7%,
and the identifier on the row settles those.

Matching the identifier as well as the description costs nothing and serves the
user who already knows `FLAP_POS` and would rather type it than describe it.

**Ordering** puts likely intent first: lamps, then selectors, then the rest.

**A usage hint per selected signal**, behind an info icon so it costs no space
until wanted. It opens on hover and on focus or click, because a hover-only hint
cannot be reached from the keyboard or on a touch screen. Everything it shows is
already in the catalogue:

```text
  FLAPS_SWITCH   selector   Landing Gear and Flap Control Panel
  selector position, 0..2
    0 = DN    1 = MVR    2 = UP
```

For a lamp that is `0 if light is off, 1 if light is on` and a range of 0..1.
Discrete signals carry their position labels, so the hint states the rules and
parameters rather than paraphrasing them.

**Learn mode.** With DCS running, the user flips the switch in the cockpit and
the editor shows which signals just changed, filtering the list to those. This
is the feature that makes unfamiliar modules tractable and is worth more than
any amount of search polish. It is also the answer to "I do not know what this
is called", which is why the typeahead needs no browse-everything mode: an empty
box until three characters is acceptable precisely because learn mode fills it
without typing.

## Conditions: every one must hold

A binding drives its lamp in one of four ways, and exactly one of them at a
time: `conditions`, `any_of`, `always` or `same_as`. This section is about the
first, which is the common case; the others are described above.

A binding carries a **list** of conditions, and the lamp lights only when all of
them are satisfied. A single-condition list is the common case and the one the
editor offers first, but the list is the shape, not an escape hatch bolted onto a
scalar. Decided 2026-09-16, after the A-10C mapping made the need concrete with
two profiles written rather than twenty.

The combining rule is one sentence: **the lamp takes the dimmest value any
condition asks for.** For on/off tests each condition resolves to either the
lamp's `on` value or zero, so the minimum is exactly boolean AND. For a
continuous source the minimum leaves the scaled value intact, which means a
backlight can be gated behind a switch without a second mechanism:

```jsonc
{
  "device": "TAKEOFF_PLANEL_2",
  "led": "HALF",
  "conditions": [
    { "source": "FLAPS_SWITCH", "on_when": { "equals": 1 } },
    { "source": "FLAP_POS", "on_when": { "gte": 300 } },
  ],
}
```

That is the A-10C half-flaps lamp, and it shows why one source per lamp was not
enough. `FLAP_POS` alone would light HALF for a second or so during travel to
DN, because the flaps pass through the half angle on the way down. Adding the
lever removes the flash entirely: by the time the flaps reach that angle the
lever already reads DN, so the binding is false and never writes. The lever
alone would be wrong in the other direction, lighting the lamp the instant the
detent moves rather than when the flaps arrive.

An **unseen** signal makes the whole binding unresolved rather than false, so
the lamp holds its swept value instead of flickering while the post-load flood
arrives. An **empty** list is a placeholder, and the lamp is swept off.

### Alternatives: any one may hold

`conditions` is one group, and every condition in it must hold. `any_of` is a
list of such groups, and the lamp lights if **any one of them** holds. That is a
list of ANDs joined by OR, which can express any boolean rule without
parentheses or precedence, the two things that make a general expression editor
easy to misread.

The case it exists for is a multicrew aircraft, where a lamp should follow
whichever seat the player is actually in:

```jsonc
{
  "device": "TAKEOFF_PLANEL_2",
  "led": "Backlight",
  "any_of": [
    {
      "conditions": [
        { "source": "SEAT_POSITION", "on_when": { "equals": 1 } },
        { "source": "CPG_LIGHT_PANEL", "on_when": { "scale": [0, 65535] } }
      ]
    },
    {
      "conditions": [
        { "source": "SEAT_POSITION", "on_when": { "equals": 0 } },
        { "source": "PLT_LIGHT_PANEL", "on_when": { "scale": [0, 65535] } }
      ]
    }
  ]
}
```

**The combining rule is the exact dual of the one within a group.** A group
takes the dimmest value any of its conditions asks for; `any_of` takes the
brightest value any group produces. For on/off tests that is boolean OR, and for
a continuous source it means the branch that is actually live supplies the value
while the gated branches sit at zero. One sentence each way, and no third
mechanism.

A branch that is already false stops there rather than reading the rest of
itself. That is what keeps the example above working: the empty seat's dimmer
may never have been touched, so it may never have been exported, and requiring
every branch to be fully readable would leave the lamp unresolved and therefore
dark. A signal that actually decides the outcome, `SEAT_POSITION` here, still
leaves the binding unresolved until it arrives.

`any_of` is mutually exclusive with `conditions` and with `always`. A binding
carrying more than one form is rejected by `validate` rather than resolved on a
guess, and the editor collapses a single remaining alternative back to
`conditions` so the file never carries two spellings of one thing.

**Seat position comes from DCS-BIOS**, not from anything we add. `SEAT_POSITION`
is exported by the five multicrew modules that have it: AH-64D (0 = Pilot,
1 = CP/G), CH-47F, C-101, Mi-24P and UH-1H. The F-14 does not export it, so a
Pilot/RIO profile has to find another discriminator.

### Always on

Some lamps have no counterpart in the cockpit, and the honest answer is that the
user wants them lit. `always` says so, and reads nothing:

```jsonc
{ "device": "TAKEOFF_PLANEL_2", "led": "SL", "always": true }
```

It is deliberately not the same as an empty condition list. Empty means "not
decided yet" and drives the lamp off, which is the right default for a lamp
nobody has looked at. `always` is a decision, so such a lamp counts as
configured and is never swept away as unassigned.

On a lamp that dims it is also how a fixed brightness is set, because `on` still
applies: a panel backlight held at one level rather than following the cockpit
dimmer.

```jsonc
{ "device": "TAKEOFF_PLANEL_2", "led": "Backlight", "always": true, "on": 40 }
```

It resolves the same at every moment, so the module-load sweep writes it once
and nothing revisits it.

`always` and `conditions` are mutually exclusive. One reads signals and the
other deliberately reads none, so a binding carrying both is rejected by
`validate` rather than silently resolved one way, and the editor offers
whichever the lamp does not already have.

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
