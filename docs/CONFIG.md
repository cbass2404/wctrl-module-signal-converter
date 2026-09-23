# Config and UI model

## The shape of it

**The LED is the primary key.** A profile is not a list of interesting signals that
happen to drive lamps; it is the device's lamp inventory, each with an answer to
"what drives this?". The UI is therefore a fixed list of every LED the connected
hardware has, and the user fills in the ones they care about.

One profile per aircraft. Aircraft are matched on the names DCS reports at
runtime, so a single profile can serve variants, and an aircraft belongs to at
most one profile.

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
  │ A/A            │ none                         ▾   │              │        │
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
  // Which font to upload to a text grid, for an aircraft whose own CDU is not
  // one DCS-BIOS exports. Left out where every aircraft here has its own,
  // which this one does. See "Text grids".
  // "font": "../mcdu/f14bu-font-21x31.json",

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
        {
          "source": "PLT_INT_LIGHT_PRIMARY",
          "on_when": { "scale": [0, 65535] },
        },
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

**Profiles are reloaded while the daemon runs.** The profile directory is polled
twice a second, and a change is acted on only once the directory has looked the
same twice running, so a file caught halfway through being written is never
read. The editor also writes to a temporary file and renames it, which makes a
half-written profile nearly impossible rather than merely unlikely.

A reload keeps the signal state and then sweeps every lamp, rather than waiting
for signals to move. Both halves matter. Keeping the state means the panel is
not blanked while DCS-BIOS gets around to re-exporting it; sweeping means a lamp
whose binding changed is corrected even though nothing it reads has moved, and a
lamp that just became unassigned is driven off. An incremental update would
notice neither.

A reload arriving while the post-load flood is still settling writes nothing.
The sweep that was already coming uses the profiles that just arrived.

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
  "conditions": [
    { "source": "LCP_CONSOLE", "on_when": { "scale": [0, 65535] } },
  ],
  "off": 255,
}
```

Every shipped profile that ties `FLAG` to a dimmer uses exactly that. `FLAG` is
the dimmer over the PTO2's seven flag lamps. Console lights off means daylight, not lamps off, so the flags
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

`same_as` points one lamp at another, and it follows whatever that lamp
resolved to. The lamp is on the same device unless `same_as_device` names
another:

```jsonc
{
  "device": "TAKEOFF_PLANEL_2",
  "led": "FLAG",
  "same_as": "Backlight",
  "off": 255,
}
```

```jsonc
{
  "device": "CarrierAce_MFD_L",
  "led": "INST_PNL_Backlight",
  "same_as": "Backlight",
  "same_as_device": "TAKEOFF_PLANEL_2",
}
```

This is a link, not a copy. The PTO2 is the case it exists for: it carries three
independent brightness governors that are usually meant to sit at one level, and
writing the same conditions into all three means every later change has to be
made three times or they drift apart without anyone noticing. Backlights across
panels are the same problem one level up: every default puts them all on one
knob, and a panel pointed at another's backlight keeps it there when the knob
changes.

`same_as_device` is written only when it names another device, so a profile
from before it existed reads the same. Naming a device that follows another
(see "One panel under several names") reads the one it follows, since the
follower's own rows are not in use; the editor offers only the device followed.

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

In the editor the floor is the **at zero** field under a dimmer's output, shown
whenever the lamp is assigned and not always on. A lamp that hides others at 0
is marked in `devices.json` with `governs`, the names of the lamps beneath it.
For those the check cautions, without blocking Save, when the lamp resolves to 0
with every signal at 0, and the daemon logs the same caution on load. A newly
generated profile starts both gates held at full with the floor already set, so
switching one to follow a dimmer does not blank the panel by day.

**Chains are not allowed**, on one device or across several. The target must
read signals of its own, which rules out cycles with no cycle detection to get
wrong: two panels pointed at each other are refused at both ends, and so is a
lamp pointed at itself through a device that follows its own. The editor does
not offer a lamp that mirrors something as a target, and does not offer
matching at all on a lamp something else already follows. A mirroring lamp
reads nothing directly, so the engine indexes it under its target's addresses;
otherwise it would be written once by the sweep and then never follow anything.

`same_as` is mutually exclusive with `conditions`, `any_of` and `always`.

## Blink comes from the source

Where a DCS lamp flashes F/A-18 gear in transit, for instance the module's own
argument is oscillating, and mirroring it reproduces the flash. There is no blink
setting to configure for those cases, and none should be offered, or users will
apply it on top of an already-blinking source and get a beat frequency.

A synthetic blink belongs only where DCS does not already express one. It is a
later addition, not part of v1.

## The editor window

Nothing fancy. A profile list with new, edit, copy and reset, and one profile
open at a time.

**New profile asks for a module, then its aircraft, never a typed name.** Both
lists come from the catalogue index, so they offer only what the user's own
DCS-BIOS supports. A module can cover several runtime aircraft names, `A-10C`
covering both `A-10C_2` and `A-10C`, and sharing DCS-BIOS outputs does not mean
wanting the same lamps, so the user picks which of them the profile is for. It
starts blank, with every LED of every inventoried device listed and unassigned
except the PTO2 gates, which is the same starter the CLI writes, or as a copy of
a related profile. **Copy to...** makes a copy for aircraft the user types.

**An aircraft belongs to one profile.** A new profile or copy that takes an
aircraft another profile claims moves it: the new one gains it and the old one
gives it up, and the dialog says so before it happens. A move that would leave
a profile with no aircraft is refused before anything is written. The file name
is not the guard: it comes from the profile's name through `file_stem`, the one
rule the daemon and the editor share, and says nothing about which aircraft a
profile claims.

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

### Checked while it is being edited, not when it is flown

The daemon's answer to a profile it cannot load is to skip the whole file, so a
single bad row costs every lamp in that profile, and it costs them on the ramp
with a mission loaded. The editor therefore runs the daemon's own checks after
every edit, through `Profile::problems`, which is `validate` collecting instead
of stopping at the first fault. Same rules, same words, one implementation.

Most faults are unreachable by construction and stay that way: the window
offers `always`, `conditions` and `same_as` as alternatives rather than fields,
clamps `on` to the lamp's range, collapses a lone `any_of` branch, builds the
mirror list from dimmers that are not already mirroring something, and derives
the seat list from the module's own `SEAT_POSITION`. What the checks catch is
everything construction cannot:

- Work that is simply unfinished. A condition or a field with no signal chosen
  is the common one, and it has its own wording rather than being reported as
  an unknown signal named `""`.
- Faults that arrived in the file, such as a profile shared by someone with
  different panels. A signal that is not in this DCS-BIOS is not one of these:
  see "Rows this DCS-BIOS cannot back" below.

Outstanding problems are listed above the rows, in full and naming the lamp or
the cells, and **Save is withheld until there are none**. The trade is
deliberate: refusing costs the time to finish, allowing it costs a sortie, and
the version already on disk is very likely one that flies. `save_profile`
refuses the same way, so the guarantee holds even if the window is wrong about
it.

**Cautions** sit beside problems, for a profile that loads but probably does not
do what was meant. They come from `Profile::cautions` and never withhold Save.
The one so far is a gate, a dimmer marked in `devices.json` with `governs`, that
resolves to 0 with every signal at 0, which hides its lamps in daylight. The
daemon logs the same cautions on load.

### Rows this DCS-BIOS cannot back

A profile is written against one DCS-BIOS release and run on whatever the user
has. A condition whose signal is not in the catalogue, or whose value is above
the signal's highest (a selector position that release does not have), is not
reading what it was written for. Refusing the whole profile over it would cost
every other lamp, so these are **flagged, not refused**, by the same rule for
shipped profiles and the user's own (`Profile::flags`):

- **A flagged condition turns off its whole AND chain.** The rest of the chain
  on its own could light the lamp when nobody meant it to. The lamp is left
  unset and swept dark.
- **In `any_of`, only the branch holding it goes.** Each alternative stands on
  its own, so the others still work. A lamp with no branch left is unset.
- **A display field reading a missing signal is left out**, and its cells stay
  blank.

What runs is `Profile::runnable`, a copy with those rows off. The file keeps
every row, so they work again once DCS-BIOS has the signal. The daemon logs one
warning per profile, grouped by reason. The editor marks each flagged
condition or field under the row itself, saying why and what it costs, and
Save stays available.

The shipped defaults target the DCS-BIOS nightly, and most users run stable.
Each release ships `data/nightly-only.json`, built by `tools/nightly_only.py`:
the signals the defaults read that the latest stable lacks or reports with a
different range. A flag on that list says the nightly has it, and the profile
page shows one line saying so, only when the installed DCS-BIOS is actually
missing some of them. Profiles carry no DCS-BIOS version.

The catalogue itself always matches the installed DCS-BIOS: it is rebuilt at
startup when the version or the `doc/json` files change, and the profiles page
says which release the signals came from.

## Where profiles live

Shipped profiles are a product, not a sample. They live read-only in
`data/defaults`. The folder the daemon and editor actually read, `data/profiles`,
is a separate, writable one, and it starts as a copy of `data/defaults`.

- **Install** copies every default in.
- **Update** copies in only the names that are not already there, and only for
  the aircraft no profile already claims. A profile the user has is theirs, and
  an update never rewrites its lamp rows. Display fields are reconciled, which
  is the one exception and has a section of its own below.
- **Reset** copies one default back over the active file, except for aircraft
  another profile has taken since, which stay where they are.
- **Delete** removes a profile the user made, or a shipped one when another
  profile can take its aircraft. When deleting would leave an aircraft with no
  profile, the editor offers the profiles that can take it; a shipped one must
  hand its aircraft on, since seeding would bring it straight back otherwise,
  so with nowhere to send them it has Reset and no Delete.

  A profile can take an aircraft when it reads the same module and already
  flies an aircraft of the same **family**. The shipped defaults are the
  families (`Profiles::families`): each shipped file groups aircraft on
  purpose, so the F-14 and F-14BU, one module shipped as two, never take each
  other's aircraft, and "No aircraft", which rides on FC3, takes nothing and
  goes nowhere. An aircraft no default lists is grouped by its module.

- **Rename** changes the name the list shows, never the file name. A name
  another profile already has is refused, ignoring case and surrounding space,
  since the name is the only thing that tells two profiles apart. Every path
  that writes a new file refuses one too; renaming was the way round it.

There is exactly one folder in use, so what a user sees in it is what runs.
Nothing is shadowed at load time and `--profiles` keeps pointing at one place.

## Correcting a display field an update changed

Never rewriting anything has a cost that took a while to become visible: a fix
shipped to a default reached nobody who already had that profile, including
somebody who had never opened it. Nothing recorded what a field said when it
shipped, so a field sitting exactly as delivered and a field somebody had spent
an evening on looked identical.

`data/defaults-previous` is that record: the defaults as the **last release**
shipped them, beside the current ones. A field is ours to correct only while it
still matches that exactly. Five cases, keyed on device, display and cells:

| In the profile | Last release | Now | What happens |
| --- | --- | --- | --- |
| matches what shipped | yes | yes | takes the new one |
| matches what shipped | yes | no | taken out |
| changed | yes | either | left alone, it is theirs |
| missing | no | yes | added, it is new |
| missing | yes | yes | stays missing, they deleted it |

The last row is why a snapshot is needed rather than a flag. Deleting a field
used to be undone on the next start, because "deleted" and "never had it" were
the same thing to look at. The fourth and fifth rows differ only in what the
last release shipped.

The cells being part of the key is what handles a field that **moved**: the old
one reads as retired and the new one as new, so it is drawn once at its new row
rather than twice. Comparison is by parsed value, not text, so the arbitrary key
order of `replace` and `aliases` and the choice between a field written flat and
one written as a chain of one are not mistaken for somebody's edit.

**It runs once per version**, recorded in `.updated` in the profiles folder,
not on every start. Otherwise somebody who put a field back the way they liked
it would have it taken away again at the next launch, and every launch after
that. Between releases their file is entirely theirs.

Two deliberate costs. Only one snapshot is kept, so a profile two releases
behind matches nothing and is left alone for good; a frozen field still works,
and the alternative is overwriting somebody who deliberately went back to an
older layout. And a missing, empty or stale snapshot silently means "correct
nothing", which is the safe direction but says nothing, so `tools/snapshot.py
--check` runs in `tools/release.cmd` before the tag and the refresh runs after
the push. Lamp rows are not part of any of this.

In a development checkout the defaults and the active profiles are one folder,
so the whole thing is skipped: there is nothing to reconcile against and the
files are tracked.

The cost, accepted deliberately: a correction shipped to a default never reaches
a user who already has that profile, including one who never opened it. Reset is
the manual remedy.

**One aircraft, one profile.** The daemon flies the first profile, by file name,
that claims the aircraft DCS reports, so a second claim is never used and would
be silent. Nothing may make one:

- Seeding matches a default by file name but checks its claim by aircraft. A
  default whose aircraft are all claimed is skipped, and one with some claimed
  comes in without them. This is what lets a deleted shipped profile stay
  deleted once its aircraft live elsewhere, and what keeps a default shipped in
  an update from doubling up on a profile the user already made.
- New profile and Copy to... move a claimed aircraft rather than share it.
- Import moves one too, but only once the user confirms the move. Unlike Copy
  to..., it may take every aircraft a profile has; that profile is then
  deleted, again only once confirmed, and declining cancels the import with
  nothing written. The move is all or nothing: if any file cannot be written,
  every file touched is put back and the new one removed.
- A starter profile is written only for an aircraft no file claims, including
  one that was skipped for a fault.
- A claim made anyway, by a file copied in by hand, is logged by the daemon and
  named at the top of the editor's profile list.

A shipped default renamed between releases (`a-10c-2.json` to `a-10c.json`,
after the DCS-BIOS module) no longer leaves two profiles behind: the new file
finds its aircraft claimed by the old one and is not seeded.

**Sharing a profile.** Export copies the file as it is on disk, so unsaved
edits are not in it. Import checks the file the way Save does and refuses one
that will not parse, reads a module the installed DCS-BIOS does not have, or
has a problem the daemon would refuse it for. Rows the local DCS-BIOS cannot
back are counted in the dialog, and load and stay off as usual. The import
always gets a new file name from the name given, keeps its author and version,
and may fly only aircraft it came with. Both file dialogs are run by the
backend; the window has no permission to open one. `editor/src-tauri/src/share.rs`
holds it.

**Merging part of a profile.** An import can instead be merged into a profile
already here on the same module, and Merge from... does the same between two
profiles here. Lights are taken a lamp at a time, ticked singly or a panel at
once: each lamp picked that the source assigns replaces the target's row for
it, and a lamp the source leaves unassigned, or that is not picked, keeps the
target's row. Only lamps the source assigns are offered. Screens are taken a line at a time,
a line being a region of the display map and a field belonging to the region
holding its first cell: the line becomes exactly the source's, so fields the
target had there go. A panel the source has following another is not offered,
since its own rows are not what flies. Nothing else moves: name, aircraft,
font, disabled panels and `follows` stay the target's, and merging onto a
panel the target follows with, or has turned off, is said in the confirm. The
merge is worked out first without writing, checked the way a save is, and put
to the user as what is added, replaced and removed; it is written only on
confirm. `crates/dsc-config/src/merge.rs` holds it.

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
unique: every one of the 21,644 signals across the 50 catalogued modules has a
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

**Learn mode**, built 2026-09-17. With DCS running, the user flips the switch in
the cockpit and the editor lists what just changed. This is the feature that
makes unfamiliar modules tractable and is worth more than any amount of search
polish. It is also the answer to "I do not know what this is called", which is
why the typeahead needs no browse-everything mode: an empty box until three
characters is acceptable precisely because learn mode fills it without typing.

Four decisions in it are worth keeping written down.

**Ranked, never filtered.** A cockpit in flight is never still, so "what
changed" on its own returns the scenery along with the answer. Signals are
ordered by how many times they moved since the panel opened: a switch thrown
once outranks a gauge that has moved ninety times. The count is shown. Nothing
is dropped for being busy, because a gauge is exactly what somebody mapping a
display field is looking for, and a filter that hid it would be a bug wearing
the clothes of a feature.

**A word is not a signal.** DCS-BIOS packs several controls into one 16-bit
word, so a word moving says only that one of its occupants did. Each signal is
read through its own mask and compared against its own previous value, which is
where most of the noise goes. A string field is the same problem inverted: six
characters is three words, all changing at once, and counting per word would
rank one edit of a scratchpad as three times busier than it is.

**Arriving is not moving.** The stream re-exports its whole map several times a
second, so the first sighting of a signal is a baseline and never a report. That
takes about one export cycle to gather, and the panel says it is reading the
cockpit until it is done, because before then an empty list means something
different from what it means afterwards.

**It listens only while the panel is open.** Pressing Learn joins the multicast
group and closing the panel leaves it. The alternative, keeping the socket for
the session so the next open is instant, saves about a second and costs the user
knowing whether anything is running while they fly. It reads a copy of a
multicast the daemon is already receiving and never transmits, so the choice is
about predictability rather than cost.

`dcs-signal learn` is the same thing on the command line, over the module DCS is
flying, printing a table per window.

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
        { "source": "CPG_LIGHT_PANEL", "on_when": { "scale": [0, 65535] } },
      ],
    },
    {
      "conditions": [
        { "source": "SEAT_POSITION", "on_when": { "equals": 0 } },
        { "source": "PLT_LIGHT_PANEL", "on_when": { "scale": [0, 65535] } },
      ],
    },
  ],
}
```

**The combining rule is the exact dual of the one within a group.** A group
takes the dimmest value any of its conditions asks for; `any_of` takes the
brightest value any group produces. For on/off tests that is boolean OR, and for
a continuous source it means the branch that is actually live supplies the value
while the gated branches sit at zero. One sentence each way.

**`"pick": "latest"`** is the one alternative to that rule, for alternatives
that are not gated by anything: the branch whose signals changed value most
recently supplies the value, dark or not. It exists for a two-seat aircraft
with a lighting knob per seat and no signal saying which seat is taken, where
brightest would mean turning both knobs down to dim and dimmest both up to
brighten. The knob last turned is the one in the player's hand, and in
multiplayer either crew member takes the panels back by turning theirs.

```jsonc
"any_of": [
  { "conditions": [ { "source": "PLT_LIGHT_INTENT_CONSOLE", "on_when": { "scale": [0, 8] } } ] },
  { "conditions": [ { "source": "RIO_LIGHT_INTENT_CONSOLE", "on_when": { "scale": [0, 8] } } ] }
],
"pick": "latest"
```

A signal "moves" when its own value changes, not when the word carrying it does,
so a neighbouring switch in the same word does not count. The value first seen,
and everything in the module-load flood, is a baseline rather than a movement,
so until a knob is actually turned the brightest branch lights the lamp. Which
knob moved last survives a profile reload and is forgotten on an aircraft
change. Absent, or `"brightest"`, is the rule above; the file only carries
`pick` when it is `"latest"`, and `pick` without `any_of` is rejected.

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
1 = CP/G), CH-47F, C-101, Mi-24P and UH-1H. The F-14 does not export it, so its
defaults use `"pick": "latest"` between the pilot's and RIO's console knobs.

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

## Panels a profile leaves alone

`disabled_devices` lists panels this aircraft should not drive at all:

```jsonc
"disabled_devices": ["CarrierAce_UFC"]
```

That is not the same as binding nothing. An unbound panel is still swept, so
its lamps go dark and its glass blank, which is right for a panel you can see.
A disabled one is never written by any path, and keeps whatever was last on
it. The case is physical: the ICP and UFC share a swing arm, and whichever is in
use covers the other.

Its lamps and fields stay in the profile and are simply ignored, so turning the
panel back on restores exactly what was set up. In the editor the panel's
section stays closed while it is not driven.

## One panel under several names

WinWing sells some panels as several products with the same hardware and a USB
id each: the MCDU as Captain, Co-Pilot and Observer, the MFD as L, C and R.
Each is its own device here, so without help every lamp and field is set up
once per name. `follows` points one at another instead:

```jsonc
"follows": {
  "MCDU_CoPilot": "MCDU_Captain",   // the follower, then the one it copies
  "MCDU_Observer": "MCDU_Captain"
}
```

The follower is driven with a copy of every lamp and field on the device it
follows, under its own name, when the engine loads the profile. Rules:

- **Only the same hardware.** Two devices qualify when their parts carry the
  same lamps at the same indices and the same displays. This is worked out from
  `devices.json` rather than listed, so a new variant needs nothing else.
- **One step deep.** A device that follows cannot be followed, so there is
  always one place to edit.
- **The follower's own rows are kept and not used**, the way a disabled
  panel's are, so stopping gives back what was there.
- **Disabling is separate.** A follower is driven unless it is disabled
  itself, so disabling the device it follows leaves it running.

Two-seat aircraft such as the AH-64D and CH-47F can follow too: the seat on
each field picks the source, so every MCDU name carries the same fields and a
follower loses nothing. Every shipped default points the MCDU Co-Pilot and
Observer at the Captain, and the MFD L and R at the MFD C.

In the editor it is the **uses** dropdown in the panel's header, offered only
on a panel that has variants. A panel that follows stays closed and says
whose setup it uses.

## Display fields

A panel with glass carries `readouts` alongside `bindings`. They have almost
nothing in common: a lamp asks "under what conditions" and resolves to a
brightness, a field asks "which cells, fed by what" and resolves to characters.

```jsonc
"readouts": [
  {
    "device": "CarrierAce_UFC",
    "display": "UFC1",       // key in data/displays
    "cells": "30-33",        // one cell, or a run
    "source": "PLT_RV5_ALT",
    "reads": [0, 750],       // what the dial is marked with, numbers only
    "decimals": 0,
    "round": "down",         // absent rounds to the nearest
    "wrap": 360,             // start again from 0 every this many
    "abs": true,             // draw the reading without its sign
    "value_aliases": {       // what to draw instead of the number
      "3": "SEMI",
      "-1.5..-0.1": "ND",
    },
    "align": "right",        // only means something across several cells
    "seat": 0,               // only where the module reports one
    "aliases": { "--": "_" },
    "format": "DED_L1_FORMAT", // text marking inverse cells, where the glass has them
    "note": "",
  },
]
```

**A field has one owner.** Nothing arbitrates between two fields claiming a
cell, because nothing needs to: the cockpit has already decided what belongs
there, or you have. Overlapping runs are rejected by `validate`. The one
exception is two different seats, below.

**`cells` is what gets stored, but not what you pick.** A display map names its
areas, and the editor offers those names: "Option 5 label" rather than `30-33`.
The run is still the thing written to the file, because a region is only a
label for one, and a field is free to take part of a region or a display that
has no regions mapped.

**`reads` is required for a number and refused for text.** DCS-BIOS reports a
needle as a position, 0 to 65535, and says nothing about what the face is
marked with, so it is yours to give. Faces that start below zero or run
backwards both work: a g meter is `[-10, 12]`, and a gauge whose numbers
descend is `[100, 0]`. A signal that already reports characters needs no
conversion, and giving it a range is an error rather than a no-op.

**`round` and `wrap` are for readings that click over or go round.** The
conversion is still a straight line from 0 to 65535 onto `reads`. After it,
the number is rounded to `decimals` places, to the nearest unless `round` is
`"down"`, and then `wrap` takes the remainder, so the reading starts again
from 0 every `wrap`. Rounding comes first, so a compass at 359.7 draws 0
rather than 360.

- One odometer drum digit, such as each of the F-16's `FUELTOTALIZER_*`
  drums, is `reads [0, 10]`, `round "down"`, `wrap 10`. DCS-BIOS reports a
  drum as how far it has turned, and a full turn passes all ten digits.
  Rounding down shows a digit only once the drum has reached it, which keeps
  the drums beside it agreeing during a roll-over. A drum sitting exactly on
  a digit draws that digit: the conversion allows for DCS-BIOS rounding the
  position to its nearest step.
- A signal that makes several turns is its whole travel wrapping at one turn:
  twelve turns of 0 to 999 is `reads [0, 12000]`, `wrap 1000`.
- A full circle is `reads [0, 360]`, `wrap 360`.

The width check allows for the wrap: a reading that wraps is measured up to
the last value before it starts over, not by the ends of `reads`.

**`value_aliases` draws a word in place of a number.** A knob reports its
position, and `3` on a screen says less than `SEMI` does. A needle on a face
marked each way from zero is read as a direction, and `-1.0` says less than
`1.0 ND` does. Each key says which readings it claims and the value says what
to draw for them; a reading no key claims draws as the number.

The key is matched against **what the face reads**, not the raw count
DCS-BIOS sends: `reads`, `decimals` and `wrap` all have their turn first. So a
band is written in the units the dial is marked with and survives the range
being retuned. A signal with no `reads` converts through its own range, which
is the identity, so a key naming a position still names that position.

Three spellings, and a value may be bare characters or an object with a colour
of its own:

- `"3"` is one reading. `"0,1,2"` is a list of them.
- `"-1.5..-0.1"` is a closed band, ends included. `to` may be written in place
  of `..`. `-` is not the separator, unlike `cells`: a cell is never negative
  and `"-1.5--1.0"` has no unambiguous reading.
- `{ "text": "NU", "colour": "red" }` draws in its own colour, which a plain
  string leaves to the piece. A band is often a caution, and one drawn in the
  colour of the row around it is one nobody catches.
- `{ "text": " ", "inverse": true }` draws inverse, on glass that draws inverse
  at all; anywhere else the profile is refused, as for an inverse piece. It is
  the colour of a screen with none, such as the DED, and a blank drawn inverse
  is a solid block.

**Two keys claiming one reading is a caution, and the lower one draws.** Keys
are held in order of where they start, so which one draws is settled and does
not depend on how the file was written. Unlike two fields claiming one cell it
is not refused: a profile is refused whole, and taking every lamp and screen
dark over one row drawing the first of two words you wrote is the wrong trade.

A key wholly outside `reads` is a caution too, since it draws nothing rather
than drawing something wrong. That is the check that catches the likeliest
mistake, which is banding a converted face in raw counts.

A needle between two bands is not a reading nothing claims: the reading is
rounded to `decimals` first, so bands a step apart leave no gap for it to sit
in. The width check allows for this too, and measures only the words when the
bands cover the whole face, since then no number is ever drawn.

**`abs` drops the sign after converting.** For a face read as a magnitude and a
direction: the F-16's trim indicators are marked in units nose up and nose
down, so `-1.0 ND` says the same thing twice. With `abs` the number is the
magnitude and a band beside it names the direction. It applies last, after a
band has had its turn, so a band written for negative readings still matches on
a piece that draws magnitudes.

**`aliases` is for a value the glass cannot draw.** DCS-BIOS reports the Hornet
scratchpad cursor as `--` where the cockpit shows `_`, and `--` is not a glyph,
so without the substitution that cell goes dark. Nothing can guess this, which
is why it is per profile rather than in the display map: the map describes the
hardware, the alias describes what one module calls something.

**`format` marks characters to draw inverse.** It names a second text signal,
laid out across the run the same way as `source`, and a cell whose mark is `i`
is drawn as a filled box with the character knocked out. The F-16 DED is the
case: DCS-BIOS sends each line as `DED_Ln` and its highlighting as
`DED_Ln_FORMAT`. Only a display that can draw inverse accepts it, which today
is the ICP's DED, and any other mark draws normally.

### Content: what fills a field

A field's content is a chain of pieces drawn end to end, each one either
characters the user typed or a signal:

```jsonc
{
  "device": "MCDU_Captain",
  "display": "MCDU",
  "cells": "0-23", // row 1, which the A-10C's own CDU does not use
  "content": [
    { "text": "RALT", "small": true, "colour": "red" },
    { "source": "PLT_RV5_ALT", "reads": [0, 300], "colour": "green" },
    { "text": "M", "small": true, "colour": "green" },
  ],
}
```

A reading on its own is rarely a readout. `250` says nothing that `RALT 250M`
does not say better, and the label, the number and the unit each want their own
colour and size. Writing them as three fields would mean counting cells by hand
and would come apart the moment the number changed width.

**A piece carries `text` or `source`, never both**, and naming both is refused
rather than resolved one way. Everything else on a piece shapes the one value
it draws, which is why `reads`, `decimals`, `round`, `wrap`, `aliases`, `format`, `colours`,
`replace`, `colour` and `small` all belong to the piece: one chain can hold two
signals that need different treatment. What belongs to the field is what is
about the run of cells as a whole, which is `cells`, `align`, `seat` and `note`.

**A field with one piece is written flat**, with that piece's `source` and
styling beside the cells, and only a chain of two or more is written as
`content`:

```jsonc
{ "device": "MCDU_Captain", "display": "MCDU", "cells": "96-119", "source": "CDU_LINE0", "colour": "green" }
```

That is not cosmetic. Every profile written before chains existed is in the
flat shape, and an update never rewrites a row the user has changed, so a field
that came back from a save as a `content` array where a `source` used to be
would turn every row into a row the user owns and freeze it against every later
fix. `crates/dsc-config/tests/profile_round_trip.rs` holds that shut.

**Nothing says when content runs past its cells.** A run is a fixed width: the
content is cropped from whichever end `align` anchors away from, the write goes
out looking healthy, and the panel shows a reading with its end missing. So the
editor works the width out ahead of time and warns, with the count, wherever
every piece is bounded. A string signal is bounded by the `max_length`
DCS-BIOS declares, and a gauge by the `reads` range it was given; a gauge with
no range is the one thing nothing bounds, and that is said rather than guessed
at. It is a warning and never a refusal: whether the aircraft ever sends a
reading that wide is the user's to judge.

**A gap takes whatever the rest of the line leaves.** It carries no `text` and
no `source`, draws blank cells, and is measured after everything else is laid
out:

```jsonc
{
  "cells": "0-23",
  "content": [
    { "text": "FUEL" },
    { "gap": true },
    { "source": "FUEL_TOTAL", "reads": [0, 11000] },
  ],
}
```

That is the thing a CDU page does constantly: a label at the left and its value
hard against the right. Typing the blanks in works until the value changes
width, which is the moment it matters, and a gap holds both ends in place
whatever happens between them. Two or more gaps split what is left evenly, the
remainder going to the earlier ones, which spaces three pieces across a line.

A gap asks for no room of its own, so it never causes an overflow warning, and
where nothing is spare it draws nothing rather than pushing anything off the
end. `align` stops meaning anything beside one, since the content already fills
the run exactly, and the editor hides it. A gap with anything written on it is
refused, as is a field of nothing but gaps: that is a blank run with extra
steps.

**A piece with nothing in it is refused**, the same as an unfinished lamp. It
is work somebody started and left, and drawing the rest of the chain around a
hole would hide it.

#### A fixed width, so a piece stays where it is put

A chain only holds still at its ends. A reading that goes from four characters
to three pulls everything after it one cell left, so a layout built around one
width comes apart at another, and there is no way to decide ahead of time
whether something will eventually run off the edge of the screen. `width` is a
piece held to that many cells whatever it reads, and `align` on the piece says
where the value sits inside it:

```jsonc
{
  "cells": "0-23",
  "content": [
    { "text": "W " },
    { "source": "WIND_SPEED", "reads": [0, 200], "width": 8, "align": "right" },
    { "text": "KT" },
  ],
}
```

The unit is in the same two cells at every reading. A box is measured before
the gaps are, so it counts as fixed room like typed characters do, and content
too long for it is cropped from the end `align` anchors away from, the way a
field is cropped by its run.

**The alignment is the user's because the two useful answers pull opposite
ways.** `centre` puts equal blanks each side, the odd one going left, which is
what a label wants and what a number does not: every character a reading sheds
moves both its edges inward half a cell at a time, so a counter drifts. `right`
keeps the digits pinned and grows the blanks in front of them, which is what
`1000` counting down to `9` should look like. `left` is the default and means
the piece starts where it starts.

A box also bounds the one thing nothing else bounds. A gauge with no `reads`
range can draw any width at all, and in a box it draws `width`, which turns the
editor's "this may run past its cells" into an exact answer.

`align` on a piece means nothing without a `width`, and the editor drops it
when the width goes. A width wider than the field's own run is refused rather
than cautioned: unlike an overflow, that one is certain before a single frame
arrives.

**A piece with a width, an alignment or a rule is always written as a chain**,
even on its own. The flat shape has an `align` and a `label` already and they
belong to the field, so a box aligned right inside a field aligned left has no
flat spelling. Every profile written before this still round trips byte for
byte, which is what `profile_round_trip.rs` checks.

#### A rule between two pieces

A gap can draw a line of dashes instead of blanks, which is the rule a
[divider](#dividers) draws as one piece of a row rather than the whole of it:

```jsonc
{
  "cells": "0-23",
  "content": [
    { "text": "NAV" },
    { "gap": true, "rule": true },
    { "source": "CDU_LINE1" },
  ],
}
```

It goes through the same `divider_rule` the whole-field version does, so the
two cannot drift, and the editor asks for it rather than drawing its own. Being
a gap, it is measured last and takes whatever the two ends leave, and a reading
that grows eats into the dashes rather than pushing anything off the end. That
is what this replaces: three fields with hand counted cell runs, where the rule
could not move, so a reading one character wider than planned overran into its
cells and was cropped.

A rule reads nothing, so it is on the glass from the moment the aircraft loads,
and it is the one kind of gap that keeps a `colour`: it is the user's own
addition rather than something the cockpit decided. A rule on a piece that
draws its own content is refused, since there would be nowhere to put it, and
so is one on glass that is not a text grid.

**A labelled rule needs a width that holds still, which is not the same as a
`width`.** The label, its blank each side and its own `label_colour` work
exactly as they do on a divider, and the room the label has to fit is whatever
the rest of the line leaves the rule. Where every other piece on the line is
itself fixed, that leftover is the same in every frame, so the check is exact
and a label too wide for it is refused the way a divider's is. A rule with the
line to itself is the plain case of that: nothing is taking cells off it, so it
is the whole run, and a label there needs no `width` at all.

Where the line carries a reading as wide as whatever it reads, the leftover
moves with it. `divider_rule` leaves a label it cannot fit off the line and
draws a plain rule, so the label comes and goes with the reading beside it.
That is a caution on the field and not a refusal: the rule draws either way,
only the user knows how wide their readings really get, and a `width` on the
rule is the fix where it bites.

### Text grids

The MCDU's screen is a grid of 24 by 14 characters that the panel draws from
a font of its own, so a field there carries text and a colour rather than
glyph bitmaps. `colour`, `small` and `colours` apply to a text grid only, and
`validate` refuses them anywhere else:

```jsonc
{
  "device": "MCDU_Captain",
  "display": "MCDU",
  "cells": "313-334", // row 14, one column in
  "source": "PLT_KU_DISPLAY",
  "seat": 0,
  "colour": "green", // white when left out
  "small": false, // the small font, for a CDU's labels
  "replace": { "~": "█" }, // one character for one, inside the line
  "colours": {
    // per character, where the module sends them
    "source": "PLT_CDU_LINE1_COLOR",
    "codes": { "g": "green", "p": "magenta" },
  },
}
```

The colours are black, amber, white, cyan, green, magenta, red, yellow, brown,
grey and khaki. Cell n is row n / 24 and column n % 24, counting from 0, so row
14 is cells 312 to 335.

**The font belongs to the aircraft, where the aircraft has one.** DCS-BIOS
cannot export every symbol a CDU draws, so each module sends stand-ins, and
each font files those symbols under characters of its own choosing.
`data/displays/mcdu.json` names the font for each aircraft whose CDU it
matches, in `native_fonts`, and that font wins over anything the profile says:
the glyphs were drawn to match what the module sends, so another font would
draw the wrong symbol rather than the same one differently.

**An aircraft with no CDU of its own takes the profile's `font`.** Nothing has
an opinion about what that screen should look like, so the choice is the
user's, named by font file exactly as a `native_fonts` value is. Until one is
picked there is no alphabet to check anything against and fields there are
refused, which is the same refusal as before and now has an answer.

**The fonts differ in what they can draw**, and not only in shape. Only the
F-14BU font has lowercase, `!`, `#`, `?` or `@`; the other three are uppercase
only. Every one of them draws fewer characters small than large, so marking a
piece `small` can take away a character that was fine at full size. Both sizes
are checked, against the size the piece actually asks for.

**`replace` joins the two.** It rewrites the module's stand-ins, one character
for one, into the characters the font draws the symbol under: the A-10C sends
its arrows as `»` and `«`. Unlike `aliases`, which swaps a whole value, it
works inside a line, and on any display. On a text grid, every character it
writes must be one the aircraft's font draws, or the profile is refused. Draw
the font's glyphs before writing one, because fonts reuse slots: in the A-10C
font `%` draws a question mark.

**`colours` is a second signal, one letter per cell.** The CH-47F sends each
CDU line with a `_COLOR` twin, and the letters are the module's own, so
`codes` says what each means. A letter with no entry, and every cell until the
signal arrives, draws in `colour`.

### Dividers

A field with `divider` draws a fixed rule instead of reading a signal, and is
how a page that does not fill the screen gets an edge:

```jsonc
{
  "device": "MCDU_Captain",
  "display": "MCDU",
  "cells": "72-95", // row 4, above the first CDU line
  "divider": true,
  "colour": "green",
}
```

It takes no `source`, and naming one is refused rather than ignored: a rule
never changes, so a signal on one is a field somebody meant to finish. There is
no range, no highlighting, no alignment and no seat worth setting, since it
draws the same thing for every station and at every moment.

**`colour` and `label` are what there is to choose**, and the editor offers
both here and nowhere else. A field's colour belongs to the aircraft, matching
what its own CDU draws, so the window leaves it alone; a rule is the user's own
addition. A new one starts on the colour the display's other fields agree on.
Black is the screen's own background, so a rule drawn in it cannot be seen.

**The rule is an unbroken run of dashes, corner to corner of its cells**,
`--------`. Spaced dashes were tried first and read as a dotted line on the
glass rather than a rule. It was inset by a blank at each end for a while,
which was reasoned about rather than looked at: flown beside real CDU lines,
which start in the first cell of their run, the rule was the one thing on the
screen that did not line up with what sat above and below it. There is no
minimum width, since one cell is one dash. The editor shows the rule as the
panel will draw it, asked of the same code that draws it.

**A `label` is set into the middle of that**, naming what the rule divides the
way a CDU does with its own:

```jsonc
{
  "cells": "72-95",
  "divider": true,
  "colour": "green",
  "label": "FUEL",
  "label_colour": "amber",
}
```

`---------- FUEL ---------`. It is centred, and where the dashes will not
divide evenly the odd one goes to the left, the way a gap gives its remainder
to the earlier side. The blank each side of it is what keeps it from reading as
part of the line, and a side with no dash left on it is not a rule any more, so
a label needs its own width plus four cells and one that does not have them is
refused rather than crowded in. Those two blanks are the only ones a rule
draws. Its characters go through the same font check as anything else drawn.

**`label_colour` is its own**, because a label drawn in the line's colour reads
as part of the line, which is the one thing a label should not do. Left out, it
follows the rule, which is what a label added to a rule that already had a
colour should look like until somebody says otherwise. The editor offers "same
as the rule" as a choice rather than leaving it as the only behaviour.

A label reads nothing, like the rest of a divider, so it is on the glass from
the moment the aircraft loads and never changes. Both keys are dropped from
anything that is not a divider, the same way `colour` is, and `label_colour`
goes too where there is no `label` to colour: kept, they would be settings the
window never shows and nothing ever draws. The editor holds on to the colour
while the text is being edited, so clearing a label to retype it costs nothing,
but that does not reach the file.

**Text grids only**, like `colour` and `small`. A segment display draws from a
fixed glyph table and none of them holds a rule, so `validate` refuses one
there rather than leaving a row of dark cells with nothing saying why. The dash
must be in the aircraft's font, and the blank too where there is a label to set
apart, which is checked the same way `replace` is.

**The shipped A-10C and AH-64D profiles carry one.** The A-10C's CDU is ten
lines on a screen of fourteen, so its rule sits on row 4, above the first line.
The Apache exports only its keyboard unit, on the bottom row, so its rule sits
on row 13 directly above, inset to the same 22 cells the keyboard unit uses, so
the two line up. The F-14B (Upgrade) has
none: its CDNU comes within two rows of filling the glass. A screen showing
only a rule still counts as a screen with something on it, so the backlight
comes up with it.

### Crew stations

`seat` restricts a field to one station. DCS-BIOS exports the whole cockpit
whatever seat you are sitting in, so a multicrew aircraft publishes both
stations at once and the field has no way to know which reading you want.

```jsonc
{ "cells": "34", "source": "PLT_CHAN", "seat": 0 },  // pilot
{ "cells": "34", "source": "OP_CHAN",  "seat": 1 },  // operator
```

Those two share cells on purpose, and it is the only case where sharing is
allowed: the stations cannot both be occupied, so they cannot both be painting.
A field with no `seat` paints from every station and therefore shares with
nothing.

The seat comes from `SEAT_POSITION`, which DCS-BIOS spells the same way in
every module that has one, so this is a convention rather than knowledge of any
aircraft. Only 5 of the 50 catalogued modules publish it, and `validate`
rejects a `seat` on the other 45 rather than accepting a field that could never
paint. The editor hides the control entirely there.

Until the seat is known, a field bound to one stays dark. Guessing would put
the other station's reading on the glass, which is worse than a blank cell
because it looks correct.

## MCDU pages

Built 2026-09-23. Profile schema version 2. The shipped defaults have not
moved onto pages yet, so until they do every shipped profile is version 1 and
refused.

A page is a named screen's worth of MCDU fields, kept in a library of its own
rather than in a profile. A profile gives each MCDU six slots that point into
the library, and says which one shows when a mission starts. Pages exist so
the screen can be swapped whole: for now by changing the start slot, later
by holding a keyboard modifier and pressing a line select key.

**Text grids only, for good.** Pages belong to CDU style screens: the MCDU
under each of its names, and any other WinWing CDU with the same kind of
glass, such as a PFP, once it is in `devices.json`. The UFC and the DED keep
their fields in `readouts`, set up once and not swapped, because a fixed panel
readout is what they are. The line is drawn by the display map rather than by
name: a display with `"transport": "text"` takes slots and no other does, so
the engine never learns the word MCDU and a new CDU qualifies with nothing
else said.

### A page file

One file per module, named by its catalogue key, holding every page on that
module:

```jsonc
// data/pages/FA-18C_hornet.json
{
  "module": "FA-18C_hornet", // the only aircraft limit, see below
  "pages": [
    {
      "id": "k3f9x2", // fixed when the page is made
      "name": "IFEI", // what the editor shows, and can change
      "display": "MCDU",
      "fields": [
        // exactly a display field, without "device"
        { "cells": "0-23", "source": "IFEI_RPM_L", "colour": "amber" },
      ],
    },
  ],
}
```

**Named by module, not by profile.** Profiles are named by hand and can
share a module: `f-14.json` and `f-14bu.json` both fly `F-14`, and
`fc3.json` and `no-aircraft.json` both ride on `FC3`. A page works in every
profile on its module, so a file per profile would hold it twice or send one
profile to another's file. The module key needs no table to find and cannot
drift from what the pages read.

**The id is not the name.** Profiles point at the id, so renaming a page
reaches every profile using it with nothing rewritten. Ids are unique across
the whole library. The name is for people, and is unique within its module,
ignoring case and surrounding space, as a profile name is: the editor only
ever shows one module's pages, so "Flight" on the F-16 and "Flight" on the
A-10C cannot be mistaken for each other.

**A file that will not parse** takes that module's pages out, and only
those: the file is named in the log and the editor, and every slot on the
module loads empty with a caution. Pages are written through a temporary file
and a rename, as profiles are, so only a hand edit can do it.

**Compatible means the same module.** A field names its signal by id, and an
id means something only in the catalogue of one module. The F/A-18C and the
F/A-18E both fly `FA-18C_hornet`, so a page made in one works in the other; an
F-14 page reads nothing in a Hornet. A slot is offered only pages on the
profile's module, and `validate` refuses any other. The editor names the
aircraft a page suits from its module, and nothing more is stored.

**A page is checked where it is used, as well as when it is saved.** Cells,
overlaps, seats and colours do not depend on where the page goes, and are
checked on save. The font does: an aircraft with a CDU of its own draws with
its native font and one without draws with the profile's `font`, so one page
can be fine in one profile and leave a blank cell in another. That check runs
for each profile that points at the page, when the profile is loaded, and the
editor shows it on the slot.

### Slots in a profile

```jsonc
"schema_version": 2,
"screens": {
  "MCDU_Captain": {
    "start": 1, // the slot shown when a mission starts, counting from 1
    "slots": [
      { "page": "k3f9x2", "key": null },
      { "page": "p81qzt", "key": null },
      { "page": null, "key": null }, // blank
      null, null, null,               // disabled
    ],
  },
},
```

- **Six slots per MCDU, one for each left line select key.** `slots` always
  holds six entries, so slot 3 is the same slot whatever is filled around it,
  and slot n is LSK nL. The editor labels each slot with its key.
- **A slot shows a page, shows a blank screen, or is disabled.** A page is
  `{ "page": id }`. Blank is `{ "page": null }`: the screen dark on purpose.
  Disabled is `null`. The difference is for swapping, below: the key of a
  disabled slot does nothing and leaves the page shown, and the key of a
  blank slot takes the screen dark.
- **`start` names a slot in use**, a blank one included, which starts the
  screen dark. With every slot disabled the screen is blank, as an MCDU with
  no fields always was, and `start` is left out.
- **The same page may sit in two slots.** Pointless, but not wrong.
- **`key` is reserved for swapping**, below, and must be null until then. It
  will say which line select key brings the slot up. It is there now so that
  giving it a meaning changes no profile's shape.
- **No loose MCDU fields.** A field in `readouts` on the MCDU is refused in
  version 2: everything on that screen comes from a page, so nothing on it
  has two owners.
- **Resolved when the engine takes the profile**, the way `follows` is: the
  start page's fields become ordinary display fields on the device, so
  painting, sweeping and resolving never learn about pages. Swapping will be
  resolving again with another slot and repainting one screen.

**Per device, as everything else is.** An MCDU that follows the Captain shows
what the Captain shows, the page included. One with slots of its own is
independent. A follower's own `screens` entry is kept and ignored, like its
other rows.

**A slot whose page has gone** (removed by hand, or not brought in by an
import) loads as empty with a caution, rather than refusing the profile.
If it was the start slot, the first filled slot starts instead.

**A page file that will not load** is named in the log and on the screen's
slots in the editor, and its pages cannot be edited until it is fixed by
hand, since saving would write over it.

### The library

- **Where it lives.** Shipped pages are read-only in `data/default-pages`,
  and the library in use is `pages` in the data folder, one file per module in
  each. Install copies a module's file whole when the library has none, and
  an update reconciles the rest a page at a time, below. In development both
  are the tracked folder, as for profiles.
- **Shipped ids are for good.** An id is never reused for a different page. A
  page that changes what it is for ships under a new id.
- **The screen's section lists the six slots and nothing else** until asked:
  each slot's menu reads Disabled, Blank, then the module's pages by name,
  and a tick marks the start slot. **Edit page** opens the page picked beside
  it in the MCDU field editor that already existed, and **New page** opens an
  empty one.
- **A page is saved on its own.** **Save page** checks it, drawn in the open
  profile's font, and writes its module's page file, apart from the profile's
  Save, which writes only the slots. A page is the library's, not the
  profile's, so one Save for both would tie an edit shared by every profile to
  the one that happened to be open. **Save as new page** writes a copy under
  a new id and leaves the page it came from as saved. Closing with changes
  asks first.
- **Editing a page changes it everywhere it is used.** That is what a library
  is for, so the editor says so: the open page lists the profiles and slots
  showing it.
- **Deleting a page** lists what uses it and, once confirmed, disables those
  slots in every profile on the module. Disabled rather than blank, so a page
  deleted never takes a screen dark that nobody chose to. A start slot that
  goes moves to the first slot still in use.

### Updates: pages and profiles apart

An update reconciles pages and profiles as two separate checks. Neither reads
the other's files, and each leaves the other valid whatever it decides, since
a slot only names an id and a page knows nothing of the slots using it. So
they need no order and no knowledge of each other. Both follow "Correcting a
display field an update changed", each against a snapshot of its own.

**Pages**, against `data/default-pages-previous`, the pages as the last
release shipped them:

- **New and deleted.** A shipped page the snapshot lacks is new in this
  release and is copied in. One the snapshot has but the library does not
  was deleted by the user, and stays deleted. This is the same pair of rows
  that tells a new field from a deleted one, and it is why pages are not
  seeded by "copy every id not already there", which would put a deleted page
  back on every update.
- **A field at a time**, keyed on cells, with the five cases in the table.
  The name is one more field: it follows the shipped name only while it still
  matches the snapshot's.
- **A new shipped page whose name a user page already has** gets a number
  added, as on import.
- **A page the release no longer ships** goes only while it is still exactly
  as the snapshot has it, name and fields both. One the user changed stays.

**Profiles**, against `data/defaults-previous` as now. A device's slots are
reconciled with the profile's display fields, keyed on device and slot
number, with `start` as one more field. A slot still as shipped takes the new
one, and a slot the user changed is theirs. Deleting a page empties its slots,
which is a change, so an update never puts back a slot the user emptied that
way.

**What that leaves.** A release that adds a page and points a slot at it
arrives in halves when the user owns one half: the page is copied in but not
slotted, because they changed that slot, or slotted where they edited the
page. Each half is valid alone, and the page is in the library to pick.

Each check keeps its own `.updated`, in its own folder, so each runs once per
version.

**In a release**, `tools/release.cmd` treats the pages exactly as it treats
the defaults, at the same three points, with `tools/snapshot.py` doing both
pairs of folders:

- **Step 0** lists the shipped pages that moved since the last tag beside the
  profiles, since a fix to a page reaches somebody who edited it only if
  CHANGELOG.md names it.
- **Step 3**, before the tag, checks that `data/default-pages-previous` still
  holds the pages the last release shipped, and refuses to tag if not.
- **Step 7**, once the tag is pushed, refreshes `data/default-pages-previous`
  from `data/default-pages` for the next release, left unstaged with the
  defaults' snapshot, so one snapshot branch and pull request carries both.
  Either refresh failing warns without undoing the release, as now.

### Sharing pages

- **Export writes the profile and its pages as one file**: every page its
  slots use, and any others on the same module ticked in the dialog. The
  dialog appears only when the module has pages no slot shows.

  ```jsonc
  { "schema_version": 2, "profile": { "name": "...", "screens": {} }, "pages": [] }
  ```

- **Import shows the pages beside the lamps and lines**, each ticked on its
  own. A slot pointing at a page left unticked comes in disabled. A profile
  file on its own, without pages, imports too.
- **A page already here by id** is left alone if it draws the same fields,
  whatever it is called here, and otherwise comes in under a new id, with the
  imported profile's slots following it. So does one whose id a page on
  another module has. **A name already taken** gets a number added, which the
  preview shows and can be changed there.
- **Merge from... offers slots on the MCDU** instead of lines: slot n of the
  source replaces slot n of the target, and brings its page into the library
  if it is not there. The UFC and the DED still merge a line at a time.
- **Version 1 files are refused**, on import and in the active folder, with a
  message saying they were made before pages. There is no migration: the
  testers reset their app data for this release.

### Swapping, later

Not part of this change, and written down so the schema holds still for it.

- **Hold Alt, Ctrl or Shift and press a left line select key**, LSK 1L to 6L
  for slots 1 to 6 unless `key` says otherwise. DCS already treats those keys
  as modifiers, so a modifier and a line select key is a binding DCS has only
  if the user made one, and needing the keyboard and the panel together makes
  it hard to do by accident.
- **The modifier is an app-wide setting**, not part of a profile, since which
  one is free depends on how DCS is bound on that PC, not on the aircraft.
- **Needs first:** a SimAppPro capture of the MCDU's input report for the key
  bits, and a keyboard reader in the daemon.
- **A disabled slot's key is ignored**, so the page shown stays; a blank
  slot's takes the screen dark.
- **The page in use is not saved.** Every aircraft load starts on `start`.
- **One swap per press**, however long it is held.

## Open questions

1. **Profile inheritance.** Should a profile be able to extend a base, so a
   generic "gear and flaps" profile can be specialised per module? Powerful for
   sharing, but it complicates the editor and conflict resolution. Leaning no for
   v1.
