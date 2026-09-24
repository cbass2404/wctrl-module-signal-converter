# WinCtrl HID protocol

How to drive WinCtrl (WinWing / WinUSA) panel LEDs directly, with no SimAppPro
process running and no elevation.

Confirmed 2026-09-16 against a CarrierAce PTO 2 by capturing SimAppPro's own
traffic (`"HIDLog": true`) and by successful round-trips from `tools/hid_probe.py`.
Builds on the protocol first reverse-engineered in `wctrl-auto-ab-detent-ratio`,
whose `Scripts/wctrl-auto-ab-detent-ratio/lib/WinctrlHid.ps1` is the reference
implementation.

## Transport

Each device exposes one HID collection, usage page `0x0001`, usage `0x04`, with a
vendor report pair:

| Report | Direction     | Usage page          | Payload                                 |
| ------ | ------------- | ------------------- | --------------------------------------- |
| `0x01` | device → host | 0x0001              | Joystick state, streams ~100/s. Ignore. |
| `0x02` | device → host | 0xff00 / usage 0x01 | 13 bytes, vendor channel                |
| `0x02` | host → device | 0xff00 / usage 0x02 | 13 bytes, vendor channel                |

On the 14-byte panels `0x02` is the only declared output report ID; Windows
rejects any other in the HID stack before it reaches the device. Panels with a
pixel screen (ViperAce ICP, MCDU) declare 64-byte reports and add a second
channel, `0xf0`, described under "Driving a pixel display". Handles open `GENERIC_READ | GENERIC_WRITE`
with `FILE_SHARE_READ | FILE_SHARE_WRITE` even while SimAppPro is running HID
access is shared and input reports reach every open handle. **No admin rights
required.**

The vendor channel is solicited: with SimAppPro closed the device sends nothing on
report `0x02` until addressed.

**One PID is not always one collection.** A CarrierAce MFD in "1 Split 3" mode
enumerates as three collections under one PID, `col01` to `col03`, all usage
`0x0001`/`0x04`. Only `col01` declares report `0x02` and an output report; the
other two are joysticks on reports `0x03` and `0x04` with no output, and a write
to them fails. Windows' listing order is not a promise, so `Device::open` takes
the collection whose report descriptor declares an Output item, and falls back
to the first when none can be read. Every other panel here is a single
collection with an output report, so the rule picks what it always did.

## Frame layout

14 bytes, matching `OutputReportByteLength`. Verified both by decoding
`sendHid()` in `WWTHID.dll` (`.text` rva `0x4ce70`) and against captured traffic.

```text
byte  0      0x02    report id (SimAppPro logs this as "channel:2")
bytes 1..4   uint32  target part id, little-endian; 0x00000001 broadcasts
byte  5      len     significant data bytes, 1..8
bytes 6..13  data    payload; data[0] is the command
```

Replies use the same layout and carry the responding part's id with `0x1000`
added (`0xBF05` → `0xCF05`). SimAppPro's log masks that bias off before printing.

## Commands

Wire values, i.e. `data[0]`.

| Code          | Name                                                                                   | Notes                                            |
| ------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------ |
| `0x00`        | ONLINE_HEARTBEAT                                                                       | SimAppPro polls this every 1-2s per device       |
| `0x01`        | REQUEST_DEVICE_HW                                                                      | read                                             |
| `0x02`        | REQUEST_DEVICE_FW                                                                      | read                                             |
| `0x03`        | REQUEST_DEVICE_SN                                                                      | read                                             |
| `0x04`        | **DEVICE_RESTART**                                                                     | reboots the panel                                |
| `0x05`        | READ_CFG_DATA                                                                          | read; `05 oo oo oo`, 24-bit LE offset, len 4     |
| `0x06`        | **WRITE_CFG_DATA**                                                                     | **persistent**; `06 oo oo oo dd dd dd dd`, len 8 |
| `0x07`        | LOOP_BACK                                                                              | diagnostic                                       |
| `0x18`        | REQUEST_DEVICE_MODE                                                                    | read                                             |
| `0x20`-`0x25` | **START_UPDATE / UPDATE_DATA / \_LEN / \_CRC / QUIT_UPDATA_MODE / READ_UPDATE_OFFSET** | **firmware**                                     |
| `0x40`        | **ENTER_UPDATA_MODE**                                                                  | **bootloader**                                   |
| `0x41`        | SET_HIDE_MODE                                                                          | volatile                                         |
| `0x42`        | REQUEST_HIDE_MODE                                                                      | read                                             |
| `0x43`        | **SET_USE_COUNTS**                                                                     | likely persistent                                |
| `0x44`        | REQUEST_USE_COUNTS                                                                     | read                                             |
| `0x45`        | REQUEST_AXIS_RAW_DATA                                                                  | read                                             |
| `0x46`        | REQUEST_AXIS_DATA                                                                      | read                                             |
| `0x47`        | **CALIBRATION_CMD_START**                                                              | alters calibration                               |
| `0x48`        | **CALIBRATION_CMD_FINISH**                                                             | alters calibration                               |
| `0x49`        | SET_LEDX                                                                               | volatile what we use                             |
| `0x4a`        | REQUEST_AXIS_CLIB_STATUS                                                               | read                                             |
| `0x4b`        | SET_LEDX_WITH_DURATION                                                                 | volatile                                         |
| `0x4c`        | SET_LCDS                                                                               | volatile                                         |
| `0x55`        | READ_PARAM_DATA                                                                        | read                                             |
| `0x56`        | **WRITE_PARAM_DATA**                                                                   | **persistent**                                   |

**Never transmit the bolded codes.** `0x40` (bootloader) is adjacent to `0x41`
(`SET_HIDE_MODE`), and `0x48` (calibration) to `0x49` (LED), so blind opcode
sweeps are not acceptable on this hardware.

> These codes were originally mis-derived from the name→code registration table in
> `WWTHID.dll`. The emitted constant belongs to the string _preceding_ it, not the
> one following; pairing them the wrong way shifts every entry by one position.
> The table above is the corrected, capture-verified mapping.

### Why LED writes cause no flash wear

`SET_LEDX` is volatile a USB interrupt OUT transfer whose payload the firmware
turns into a PWM/GPIO register write in RAM. Persistence is a separate command
family, and the `WRITE_PARAM_DATA` / `SAVE_PARAM_DATA` pair is the usual
RAM-then-commit split; `SET_LEDX` has no save counterpart, and
`SET_LEDX_WITH_DURATION` implies a RAM-resident expiry timer. SimAppPro streams
these continuously during normal play.

## Setting an LED

```text
02 | 05 bf 00 00 | 03 | 49 <index> <value> 00 00 00 00 00
```

`len` is 3 and the payload is `[0x49, index, value]`. The device echoes the same
command back as an ack, so a write can be confirmed.

Captured example typing "137" into SimAppPro's Landing Gear slider sends one
frame per keystroke:

```text
02 05 bf 00 00 03 49 01 01    Landing_gear_lights = 1
02 05 bf 00 00 03 49 01 0d    = 13
02 05 bf 00 00 03 49 01 89    = 137
```

### Persisted brightness

SimAppPro saves a panel's dimmers to device flash. The offset differs per part
and the layout is one byte per dimmer, in index order everywhere except the
rudder pedals, which store theirs in slider order:

| Part     | Offset  | Byte 0               | Byte 1             | Byte 2         |
| -------- | ------- | -------------------- | ------------------ | -------------- |
| `0xbf05` | `0x114` | `Backlight`          | `SL`               |                |
| `0xbed0` | `0x0d8` | `INST_PNL_Backlight` | `LCDBacklight`     |                |
| `0xbe0e` | `0x0c8` | `INST_PNL_Backlight` |                    |                |
| `0xbe0d` | `0x0c8` | `INST_PNL_Backlight` |                    |                |
| `0xbef0` | `0x0e8` | `Backlight_L`        | `Backlight_R`      | `Logo`         |
| `0xbb32` | `0x0d0` | `Backlight`          | `Screen Backlight` | `Marker Light` |

```text
WRITE_CFG_DATA  offset 0x114  <- 00 de 00 00     after PTO2 SL = 222
WRITE_CFG_DATA  offset 0x114  <- 6f de 00 00     after PTO2 Backlight = 111
WRITE_CFG_DATA  offset 0x0d8  <- 0f 7b ff ff     after UFC LCDBacklight = 123
WRITE_CFG_DATA  offset 0x0d8  <- d3 7b ff ff     after UFC panel = 211
WRITE_CFG_DATA  offset 0x0c8  <- fb ff ff ff     after HUD panel = 251
WRITE_CFG_DATA  offset 0x0c8  <- 89 ff ff ff     after MFD panel = 137
WRITE_CFG_DATA  offset 0x0e8  <- 0b de 65 ff     after pedals L 11, R 222, logo 101
WRITE_CFG_DATA  offset 0x0d0  <- 87 87 87 ff     after MCDU all three = 135
```

**Never write these.** `WRITE_CFG_DATA` is persistent and is on the forbidden
list, so our brightness changes stay volatile which is correct: we should not
wear flash or silently change a user's saved panel settings. `READ_CFG_DATA` at
the same offsets is a safe read, and is how the daemon can warn that a stored
brightness of zero would leave correctly-bound output invisible.

The UFC sets that trap twice. A dark `INST_PNL_Backlight` hides the legends, and
a dark `LCDBacklight` hides the entire segment display while every cell is being
driven correctly.

### Value ranges differ per LED

`Backlight` (0), `Landing_gear_lights` (1), `SL` (2) and `FLAG` (3) are dimmers:
SimAppPro drives them with sliders and they were observed taking values across
the whole `0-255` span.

The fourteen indicators at indices 4-17 take **`0` or `1` only**, verified on
hardware 2026-09-16. `1` lights, `0` clears, and `255` is out of range: it acks
and lights nothing, so it is not truthy. `data/devices.json` records them as
`kind: indicator` with `max: 1`. Index 17 (HOOK) is a physically dim lamp,
visibly weaker than its neighbours at full brightness, and not a protocol
difference.

The lesson stays: never default a range to 255. That assumption sent us down a
blind alley where every write acked and nothing visibly happened.

SimAppPro's own DCS path computes brightness as `value * 255` from the DCS
argument (`DCSAPULight.js`), which is consistent with the dimmers but tells us
nothing about the indicators, since its LED bindings for them are on/off.

## Part discovery

Send any command to part id `0x00000001` and every sub-part answers with its own
id. Observed on this machine:

| Part               | Device                           |
| ------------------ | -------------------------------- |
| `0xbf05`           | CarrierAce PTO 2                 |
| `0xbe60`           | Orion Throttle Base II           |
| `0xbf01`, `0xbf02` | F15EX handles L / R              |
| `0xbef0`           | Orion Combat Rudder Pedals Metal |
| `0xbed0`           | CarrierAce UFC, the glass and its backlights |
| `0xbe0e`           | CarrierAce HUD control panel     |
| `0xbb32`           | MCDU, under every name           |

A part id is not the USB product id: the Orion II enumerates under PID `0xbd64`
as a composite device but answers as part `0xbe60`, with its handles as separate
parts.

## LED indices

From `www/js/DeviceConfig.js` inside `app.asar`. These are the `index` byte.

**That table is not complete.** It omits index 3 entirely, and index 3 is a
dimmer that governs half the PTO2's lamps. Treat the vendor's list as a starting
point to verify against hardware, never as the inventory.

### PTO2 `TAKEOFF_PLANEL_2`, part `0xbf05`

| Index | LED                 |
| ----- | ------------------- |
| 0     | Backlight           |
| 1     | Landing_gear_lights |
| 2     | SL                  |
| 3     | FLAG                |
| 4     | Master_Caution      |
| 5     | JETT                |
| 6     | CTR                 |
| 7     | LI                  |
| 8     | LO                  |
| 9     | RO                  |
| 10    | RI                  |
| 11    | FLAPS               |
| 12    | NOSE                |
| 13    | FULL                |
| 14    | RIGHT               |
| 15    | LEFT                |
| 16    | HALF                |
| 17    | HOOK                |

**Three independent brightness groups, measured 2026-09-16** by holding all 14
indicators at 1 and moving one dimmer at a time:

| Dimmer          | Governs                                          | Behaviour     |
| --------------- | ------------------------------------------------ | ------------- |
| `SL` (2)        | all indicators, 4 to 17                          | hard gate     |
| `FLAG` (3)      | NOSE, LEFT, RIGHT, FLAPS, HALF, FULL, HOOK       | brightness    |
| `Backlight` (0) | panel labels only, no indicator                  | brightness    |

At `FLAG` 0 those seven lamps are invisible while CAUTION, JETT, CTR, LI, LO, RO
and RI stay lit; raising `FLAG` brings all seven back with no rewrite of the
lamps, the same latch-beneath-the-governor behaviour `SL` shows.

Index 1, `Landing_gear_lights`, is the brightness of the landing gear handle's
own light, identified by Cory 2026-09-22. Setting it to 0 with every indicator
lit changed nothing visible because the handle was not lit at the time. It is
verified as a dimmer taking 0-255.

This cost a full debugging session. The engine resolved the A-10C flap lamps
correctly, every write acked, and the lamps were invisible because `FLAG` sat
near 0 where SimAppPro had left it. A dimmer nothing writes is invisible state.

### Orion Throttle Base II part `0xbe60`

| Index | LED       |
| ----- | --------- |
| 0     | Backlight |
| 1     | A/A       |
| 2     | A/G       |

### CarrierAce UFC part `0xbed0`, HUD part `0xbe0e`

| Part     | Index | LED                  |
| -------- | ----- | -------------------- |
| `0xbed0` | 0     | INST_PNL_Backlight   |
| `0xbed0` | 1     | LCDBacklight         |
| `0xbe0e` | 1     | INST_PNL_Backlight   |

All three are dimmers, seen taking values across the whole `0-255` span while
the sliders moved. **There are no indicators on either panel.** Every light is
a backlight or part of the segment display.

`0xbe0e` really does start at index 1, with no index 0. That reads like a
transcription slip and is not: nothing addressed index 0 in a capture, and the
vendor table declares only the one entry.

The UFC also carries a segment display, which is a different command entirely.

### CarrierAce MFD part `0xbe0d`

| Index | LED                |
| ----- | ------------------ |
| 0     | INST_PNL_Backlight |

One dimmer, the bezel legends, seen taking every value from `0x00` to `0xff` as
the slider moved and 1, 13 and 137 when typed. Captured 2026-09-18.

**One MFD has three identities, and each is its own PID.** SimAppPro renames it
so a user with several keeps their bindings apart, and the rename changes the
USB product id. Part, serial and lamp stay the same:

| Name | PID      | Config `0xd8` |
| ---- | -------- | ------------- |
| C    | `0xbee0` | `0`           |
| L    | `0xbee1` | `1`           |
| R    | `0xbee2` | `2`           |

```text
WRITE_CFG_DATA  offset 0x0d8  <- 00 00 00 00     rename to C
WRITE_CFG_DATA  offset 0x0d8  <- 01 00 00 00     rename to L
WRITE_CFG_DATA  offset 0x0d8  <- 02 00 00 00     rename to R
```

The device drops off USB and comes back under the new PID about a second
later. All three renames captured 2026-09-18.

**"1 Split 3" mode** is config `0xd0`: `2` on, `0` off. PID and name are kept,
and the device re-enumerates as three collections (see Transport). The side
switch picks which of them the buttons report on. The switch itself is a
button: input byte 5 reads `0x10` at the bottom, `0x20` in the middle and
`0x40` at the top, in either mode. Flipping it sends no config write, causes no
re-enumeration and carries no lamp traffic, so a split MFD still has one
backlight, on `col01`.

The name and the mode are independent. Split was checked under R and again
under L: the same three collections and descriptors, `col01` alone writable,
under whichever PID the name gives. Read back from the device under L, with
`READ_CFG_DATA`: `0xd0` = `2`, `0xd8` = `1`, `0xc8` = `0x89`.

Both offsets are `WRITE_CFG_DATA`, so neither can be sent from here, and a
rename is the user's business in SimAppPro.

### Orion Combat Rudder Pedals part `0xbef0`

| Index | LED         | Lights                     |
| ----- | ----------- | -------------------------- |
| 0     | Backlight_L | the left pedal sensor      |
| 1     | Backlight_R | the right pedal sensor     |
| 2     | *(none)*    | writes both 0 and 1        |
| 3     | Logo        | the WinWing logo           |

All three lamps are dimmers, captured 2026-09-18 from SimAppPro's three sliders
taking 1 and 11, 2, 22 and 222, and 1, 10 and 101. The vendor table lists only
`Backlight1` 0 and `Backlight2` 1. **The logo is on index 3 and is missing from
it**, and the saved-brightness record at `0xe8` puts the logo's byte third, in
slider order rather than index order.

**Index 2 is a shortcut, not a lamp and not a gate.** A write to it goes through
to both pedal lights, and whichever write came last wins. Worked out on
hardware with `dcs-signal led`, with 1 and 2 starting at 0:

1. 1 = 255 lit the right light alone, so 2 at 0 does not hold it off.
2. 2 = 255 lit both, with 1 still at 0, so 2 drives them directly.
3. 1 = 0 put the right light out with 2 still at 255, so 2 leaves nothing
   latched beneath it.
4. 2 = 0 put both out.

Index 0 alone, from all dark, lit only the left light, which makes index 2 a
fourth control SimAppPro does not show: 0 and 1 together. SimAppPro never
writes it. A profile that wants the lights together uses `same_as` instead,
which every shipped default does, and a user can still split them. It is left out of the inventory on purpose: a sweep
that owned it would overwrite L and R with whatever it wrote, depending on
order.

### MCDU Captain part `0xbb32`

One part, `0xbb32`, under any of three names. Labels are SimAppPro's.

**One MCDU has three identities, and each is its own PID**, as the MFD's do.
SimAppPro renames it, and the panel drops off USB and comes back about three
seconds later under the new name:

| Name     | USB PID  | Config `0xcc`, byte 1 |
| -------- | -------- | --------------------- |
| CAPTAIN  | `0xbb36` | 0                     |
| OBSERVER | `0xbb3a` | 1                     |
| CO-PILOT | `0xbb3e` | 2                     |

All three captured 2026-09-18 with the same serial and part under each, and
CAPTAIN rechecked after the round trip. Byte 0 of the record was `01` every
time; before the first rename byte 1 read `ff`, and the panel was CAPTAIN.
Windows also remembers this serial under PID `0xbb32`, the part id, from before
today. Which name or mode that was is unknown.

| Index | LED              | Kind   |
| ----- | ---------------- | ------ |
| 0     | Backlight        | dimmer |
| 1     | Screen Backlight | dimmer |
| 2     | Marker Light     | dimmer |
| 8     | Fail             | on/off |
| 9     | FM               | on/off |
| 10    | MCDU             | on/off |
| 11    | MENU             | on/off |
| 12    | FM1              | on/off |
| 13    | IND              | on/off |
| 14    | RDY              | on/off |
| 15    | STATUS           | on/off |
| 16    | FM2              | on/off |

Captured 2026-09-18. Each dimmer was swept 0 to 255 to 0, then typed, and the
nine indicators were switched on and back off in the order above, each only
ever written 1 or 0. Indices 3 to 7 were never written.

### PFP-3N part `0xbb31`, PFP-7 part `0xbb33`, PFP-4 part `0xbb34`

**Not captured here.** Nobody on this project has a PFP, so everything below
is WwDevicesDotnet's (commit `2bf28fa`: `SupportedDevices.cs`,
`Winctrl/README.md`, `Winctrl/Pfp*/`). Its PFP support has been confirmed
working by other owners, which is why it was taken on; every lamp and key is
still marked unverified in `devices.json` until a panel is seen here.

Each PFP has three names, each its own PID, as the MCDU does:

| Model  | Captain  | Observer | Co-Pilot |
| ------ | -------- | -------- | -------- |
| PFP-3N | `0xbb35` | `0xbb39` | `0xbb3d` |
| PFP-7  | `0xbb37` | `0xbb3b` | `0xbb3f` |
| PFP-4  | `0xbb38` | `0xbb3c` | `0xbb40` |

The library pairs the PFP-4's PIDs with no seat, so its seats are taken from
the pattern every other CDU follows: Observer +4, Co-Pilot +8.

**The part id is the library's command prefix.** WwDevicesDotnet calls the
two bytes after the report id a command prefix: `31 bb` on the PFP-3N, `32 bb`
on the MCDU, `33 bb` on the PFP-7 and `34 bb` on the PFP-4. On the MCDU that is
part `0xbb32` written low byte first, which our own captures confirm, so the
PFPs are parts `0xbb31`, `0xbb33` and `0xbb34`.

**The screen is the MCDU's**: the same 24x14 grid on report `0xf2` and the
same font upload, so every PFP part declares display `MCDU` and shares its
pages. The library measured the PFP's visible area as 86mm tall against the
MCDU's 80mm, which fits 32px glyphs where the MCDU takes 31. The MCDU's 31px
fonts are used on both for now, so on a PFP the rows drift up to about 12px
above their line select keys toward the bottom.

| Index | LED              | Kind   |
| ----- | ---------------- | ------ |
| 0     | Backlight        | dimmer |
| 1     | Screen Backlight | dimmer |
| 2     | Marker Light     | dimmer |
| 3     | DSPY             | on/off |
| 4     | FAIL             | on/off |
| 5     | MSG              | on/off |
| 6     | OFST             | on/off |
| 7     | EXEC             | on/off |

The same on all three models. Indices 0 to 2 are the dimmers the library
drives on every WinWing CDU, and 3 to 7 are the ones the MCDU never took.

**The keys differ by model.** The twelve line select keys are the MCDU's
(bytes 1 and 2), and the rest are from each model's `KeyboardMap.cs`: the
PFP-7 is the PFP-4 with ALTN where the PFP-4 has ATC, and the PFP-3N moves
about ten keys and adds CLB, CRZ, DES and N1 LIMIT. A key at byte `b`, flag
bit `n` is button `(b - 1) * 8 + n + 1`, which is how the MCDU's captured LSKs
number.

## Driving a segment display

`SET_LCDS` (`0x4c`) writes four bytes of a device-side bitmap. It is volatile,
like `SET_LEDX`, and the payload is:

```text
02 | d0 be 00 00 | 06 | 4c <group> <b0> <b1> <b2> <b3> 00
```

`len` is 6. `group` selects which four bytes of the buffer to replace, so the
byte offset is `group * 4`. The UFC's buffer is 96 bytes, giving groups `0x00`
to `0x17`.

**`SET_LCDS` is not acknowledged.** 24 frames written to the UFC drew 0
replies, using the same read window that had just taken an immediate echo from
a `SET_LEDX` write to the same device moments earlier. So the ack discipline
that applies to LEDs, where a dropped write would otherwise persist unnoticed,
has no equivalent here: a display write cannot be confirmed from the device.

The mitigation is that a display is redrawn from a shadow buffer rather than
from deltas, so a full repaint restores a display that has drifted, and a
resync path that writes every group costs 24 frames.

**The buffer is segments, not characters.** There is no text anywhere on the
wire. A character position is a set of bit indices scattered across the buffer,
and a glyph says which of that position's segments to light. The map for the
UFC, 36 cells and 105 glyphs, is `data/displays/ufc1.json`.

Two consequences that are not obvious:

- A cell can straddle two groups, so a single character change can take two
  frames, and the cell reads as a **different, wrong letter in between**. A
  capture is full of these. They are not errors.
- Writing a cell means read-modify-write of its group, because the other bits in
  that group belong to neighbouring cells. A host shadow of the whole buffer is
  not an optimisation here, it is required for correctness.

SimAppPro diffs its own shadow and sends only groups whose bytes changed, which
is the same write-on-change discipline the LEDs need and for the same reason.

### Glyphs are looked up by the whole field value

A field is not necessarily one character per cell. `UFC_COMM1_DISPLAY` is two
characters wide in DCS-BIOS and occupies **one** cell, and the glyph table has an
entry keyed by the two-character string. Those multi-character glyphs are not
the union of their parts:

```text
'0'   lights slots 0,5,6,7,9,13
' 0'  lights slots 1,2,3,4,9,13
```

and `` ` `` does not exist as a glyph at all, so `` `0 `` can only come from the
table. Verified live: DCS-BIOS reported `UFC_COMM1_DISPLAY` as `' 2'` and the
hardware was sent the `' 2'` glyph. Look up the whole value first; SimAppPro
only falls back to OR-ing two glyphs when the pair is absent.

### Where each segment sits, for the editor's preview

Nothing on the wire needs it, and nothing did until the editor started drawing
a field before it is flown. A buffer of bit indices says which slots a glyph
lights and nothing at all about where they are, so `art` in `ufc1.json` gives
each slot a stroke, and where each one sits was read out of the glyph table
rather than captured:

- The pair above splits the cell. `'0'` takes 0,5,6,7 and `' 0'` takes 1,2,3,4,
  so those are the left and right of a 16 segment box, with 9 and 13 the
  centre bar both halves share. `` `1 `` lights 2,3,6,7: one digit each side.
- `'Z'` lights 10 and 14, so they are the diagonals running top right to
  bottom left, and `'X'` adds 8 and 12 for the other pair.
- `'B'` takes 11 where `'F'` takes 15, which puts 11 on the right of the
  middle bar and 15 on the left.

So the preview is exactly right about which segments light, since the daemon's
own lookup answers that, and only as right about where they sit as that
reading of the table. Replace it if a photograph of the glass ever says
otherwise. The DED needs none of this: its slots are pixels of the cell, and
the cell is generated from the grid.

### Where the UFC's cells come from, for the Hornet

| Cells                             | Shape   | DCS-BIOS signal                  | Rule                |
| --------------------------------- | ------- | -------------------------------- | ------------------- |
| 0, 1                              | 16-seg  | `UFC_SCRATCHPAD_STRING_1/2_DISPLAY` | whole 2-char value |
| 2 to 8                            | 7-seg   | `UFC_SCRATCHPAD_NUMBER_DISPLAY`  | **last 7 of 8**     |
| 9, 14, 19, 24, 29                 | 1-seg   | `UFC_OPTION_CUEING_1..5`         | one char            |
| 10-13, 15-18, 20-23, 25-28, 30-33 | 16-seg  | `UFC_OPTION_DISPLAY_1..5`        | one char per cell   |
| 34, 35                            | 16-seg  | `UFC_COMM1/2_DISPLAY`            | whole 2-char value  |

The scratchpad rule was confirmed from both ends. DCS-BIOS reports eight
characters, the panel has seven cells, and keypresses enter at the right:
`'    .  1'`, `'    . 12'`, `'    .123'`.

This table is Hornet-specific and belongs in a profile, not in the firmware
knowledge above it. The cell and glyph map is a property of the device; which
signal feeds which cell is a property of the aircraft.

### DCS-BIOS is not the same as DCS's own indication

SimAppPro reads `list_indication(6)` directly. DCS-BIOS reads the same
indication but does not always report the same characters:

```text
raw DCS     UFC_ScratchPadString2Display = '_'
DCS-BIOS    UFC_SCRATCHPAD_STRING_2_DISPLAY = '--'
```

`'--'` is not in the glyph table, so a lookup falls through to the merge path
and draws one dash where the hardware should show an underscore. Mapping a
DCS-BIOS value onto a glyph key therefore needs an alias table, and since the
wording is a property of the module, it belongs with the rest of the per-module
mapping.

### String fields arrive in pieces

DCS-BIOS packs a string two bytes to a word, and the words of one field do not
all land in the same write. Captured between two keypresses:

```text
'    . 12'      <- real
'    .112'      <- never on screen, half of the update applied
'    .123'      <- real
```

So a display must be repainted on a settled state, not per write. Batching by
datagram is the natural place to do it.

## Driving a pixel display

The ViperAce ICP (PID `0xbf06`, one part, `0xbf06`) carries the F-16 DED as a
monochrome pixel screen. It is not driven by `SET_LCDS` or by report `0x02` at
all. Decoded 2026-09-18 from a SimAppPro capture of a live F-16 and then
**confirmed on hardware** by drawing with frames we built ourselves: a solid
bar, a multi-report write, a full clear, and "UHF" in captured glyphs, legible
and the right way round. **Flown the same day:** the daemon drove the DED from
DCS-BIOS in a live F-16 through every page tried, inverse fields included.

### Wire format

Every report is 64 bytes on report `0xf0`:

```text
byte  0      0xf0    report id
byte  1      0x00    host to device
byte  2      seq     host counter, wraps at 256
byte  3      n       bytes of logical frame in this report, 1..60
bytes 4..    chunk   the next n bytes of the logical frame, zero padded to 64
```

A logical frame longer than 60 bytes is split into consecutive 60-byte chunks,
each in its own report with its own header. Nothing else marks a continuation.

**`WWTHID.log` hides this header.** Its `Hiddata:` lines print `f0` followed
directly by the logical frame, so a frame replayed from the log as printed is
acknowledged by the device and then ignored. That cost one round of testing.

The logical frame:

```text
bytes 0..3    uint32  part id, 06 bf 00 00
bytes 4..7    uint32  function id: 0x102 write, 0x103 commit, 0x104 self-test
bytes 8..11   uint32  host milliseconds clock
byte  12      respond: 1 asks the device to answer
bytes 13..16  uint32  payload length
bytes 17..    payload
```

This was first read as a one-byte `cmd` followed by a constant `01 00 00`.
WwDevicesDotnet's notes on the same channel (see "Driving a text grid") show
it is a 32-bit function id, and the names are SimAppPro's: `0x103` is
`refreshLCD`. Several of these commands can share one stream and straddle
reports; the device reassembles them.

The clock is not checked for continuity: frames stamped from our own clock,
starting near zero, were accepted straight after SimAppPro's.

The device answers **each report** with `f0 01 <nn> 00`, where `nn` is the
device's own counter and carries on across processes. So a 96-byte frame draws
two acks.

### Pixel display commands

| cmd    | Payload                   | Effect                                              |
| ------ | ------------------------- | --------------------------------------------------- |
| `0x02` | `u32 address`, then bytes | write into the framebuffer, not yet shown           |
| `0x03` | `00`                      | commit: show what has been written                  |
| `0x04` | one byte, 1..6            | factory self-test pattern, see below                |

Self-test values, matching SimAppPro's buttons in order: 1 ALL LCD ON, 2 ALL LCD
OFF, 3 to 6 HALF LCD ON 1 to 4. These are stored in the firmware and need no
framebuffer writes.

**The MCDU uses the same frame.** SimAppPro's screen buttons for the MCDU
Captain send exactly this logical frame, part id `32 bb 00 00`, cmd `0x04`,
length 1, one byte of payload:

```text
32 bb 00 00 | 04 | 01 00 00 | ea 0f 01 00 | 00 | 01 00 00 00 | 0d
```

Its self-test values are 13 to 20 rather than 1 to 6, and its screen is color:

| Value | Screen                                  |
| ----- | --------------------------------------- |
| 13    | white                                   |
| 14    | black                                   |
| 15    | red                                     |
| 16    | green                                   |
| 17    | blue                                    |
| 18    | lime                                    |
| 19    | purple                                  |
| 20    | black with the WinWing logo, as at boot |

Captured 2026-09-18, with the colors as seen on the panel. Each report drew the
usual `f0 01 <nn>` ack. Whether `0x02` and `0x03` draw on the MCDU as they do on
the ICP, and its framebuffer's size and pixel format, are not known yet:
SimAppPro's device page sends only self-tests, so that needs a capture of a
live module.

### The framebuffer

1 bit per pixel, **200 pixels wide**, 25 bytes per row, rows top to bottom.
64 rows, 1600 bytes: SimAppPro's highest write ends at byte 1597, in row 63,
and it inks row 63 with the bottom of an inverse box on line 5.

- **The write address is in pixels, not bytes**: `y * 200 + x`, with `x` a
  multiple of 8. Row 2, column 8 is address 408.
- **The least significant bit is the leftmost pixel.** Read MSB first, every
  glyph comes out mirrored.
- Written bytes run on across row ends, so one write can cover several rows.
- The largest single write SimAppPro sent carried 270 data bytes. Ours stayed
  at 225. Larger may work and has not been tried.

### Layout of the DED

SimAppPro draws a character cell at `x * 8, y * 13 + 1` (`F16_ICP.js`): 24
columns of 8 pixels and 5 lines on a 13-pixel pitch, glyph ink starting at rows
2, 15, 28, 41 and 54. That is DCS-BIOS's `DED_L1` to `DED_L5` grid, 24
characters each, one to one. Five 13-row lines would be 65 rows, so line 5's
cell has no bottom row; it is margin and never inked.

Every captured glyph sits in columns 1 to 6 and rows 2 to 10 of its cell, with
2-pixel strokes.

Inverse is not a device feature. SimAppPro draws a cell flagged `i` in
`DED_Ln_FORMAT` by filling rows 1 to 11 of the cell and knocking the glyph out,
and the device shows those bytes like any others. Drawing `DED_L3` from the TCN
page that way reproduces SimAppPro's frame byte for byte.

The font is host side. `WWTHID_JSAPI.node` loads `config/ICP/ICP_font_0..2.png`
and indexes them through `textfont_config_new.json`, 66 characters.

### SimAppPro does not use DCS-BIOS for this

It calls `list_indication(6)` through its own export script and places each
named element using `config/ICP/ded_dcs.xlsx`. DCS-BIOS reads the same
indication and has already laid it out as five 24-character lines plus a format
string per line, so we need neither the spreadsheet nor the element names.

SimAppPro repaints about once a second and sends only the byte ranges that
changed. The CNI clock ticking over is one 1-byte write and a commit.

### It latches

The screen kept "UHF" after the writing process exited, so it needs the same
blank-on-end discipline as the lamps and the UFC.

### A page change flashes

A large update shows a brief full-bright flash on the glass between the old
page and the new one. SimAppPro does exactly the same on the same panel
(observed 2026-09-18), and its page changes are a burst of about 21 to 25
reports in 1 to 2 ms without waiting for acknowledgements, much as ours are. So
this is the device, not the host, and not worth chasing.

### The screen backlight is separate

With the ICP's screen backlight at zero a correct frame shows nothing at all.
Seen during the test above, where the bar only appeared once the backlight was
turned up. That dimmer is ordinary lamp state, and a profile has to own it, or
a working DED looks dead.

## Driving a text grid

The MCDU's screen is driven as a grid of characters the panel draws itself,
from a font it has been sent. None of this comes from a SimAppPro capture:
SimAppPro never drives the MCDU from DCS. It is ported from WwDevicesDotnet
(BSD-3-Clause, Andrew Whewell and Laurent André,
<https://github.com/landre-cerp/WwDevicesDotnet>, `Winctrl/README.md`), which
worked it out from SimAppPro's font upload, and it was **confirmed on our own
panel 2026-09-18** with `dcs-signal mcdu-test`: the corners of the 24x14 grid, ten
colours, the large font and an inverse cell all came out as sent.

**Setup, on the pixel channel `0xf0`, as structured commands:**

| Function               | Id      | Payload                                 |
| ---------------------- | ------- | --------------------------------------- |
| clearFeatureInfo       | `0x11e` | none                                    |
| setScreenInfo          | `0x118` | `x u16, y u16, rows u16, columns u16`   |
| setFeatureInfo         | `0x119` | `feature u16, value u32, format id u64` |
| setCompositeIndexBytes | `0x11a` | `02`                                    |
| buildFormatTable       | `0x11c` | none                                    |

The feature declarations say what a cell can look like: font slot 5 or 6,
eleven foreground colours, eleven background colours, and a first/last-cell
marker. The device indexes the product of them, and a cell names its look by
that index: `fg * 33 + bg * 3`, plus 363 for the small font, plus 1 on the
first cell of the screen and 2 on the last. Our declarations are byte for byte
the ones inside SimAppPro's own upload (`crates/wctrl-hid`, test
`the_format_table_is_the_one_simapppro_declares`).

**The screen, on report `0xf2`:** every cell in order, each its format index
**low byte first** (WwDevicesDotnet's README says big-endian; its code and our
panel say otherwise) followed by the character in UTF-8. The stream runs on
across 64-byte reports after the `f2` report id, and the last is zero padded.
A cell cannot be written alone: every write is the whole screen. A 24x14
screen of plain characters is 16 reports. Sent too fast, screens can garble,
so 40 ms follows each one.

**The font is not the panel's.** Glyphs live in RAM and are lost on a power
cycle, and until a font is sent the grid draws nothing. The upload is
SimAppPro's, replayed from WwDevicesDotnet's packet map with the glyph bytes
swapped in (`data/mcdu`, `crates/dsc-config/src/mcdu_font.rs`): 603 reports
of `0x106` downLoadFontHead for slots 5 and 6, `0x107` downLoadFontData,
`0x105` getLastErrorString after each chunk, its own format table, and one
`SET_LEDX` of the screen brightness. It sends nothing persistent. It resets
the grid to 24x14, so the grid is declared again after it.

**A font is an aircraft's, not a character set.** The A-10C font, from
WCtrlDcsBiosBridge (MIT), draws the A-10C CDU's own symbols in slots named for
other characters: `Δ` is an up-down arrow, `⬡` the cursor block, and `%` a
question mark. It has no lower case and no `#`. So which font a screen needs
is decided by the aircraft, recorded in `data/displays/mcdu.json` as
`native_fonts`, and a profile swaps DCS-BIOS's stand-in characters for the
slots it wants with `replace`.

**DCS-BIOS sends those stand-ins as single bytes above ASCII**, Latin-1, not
UTF-8: the A-10C module's `cdu_replace_map` makes its arrows `0xBB` and `0xAB`.
A display field is therefore read one byte per character.

## Capturing SimAppPro's own traffic

1. Set `"HIDLog": true` in `%APPDATA%\SimAppPro\config.json` (read at startup only).
2. Restart SimAppPro.
3. Reproduce the action.
4. Read `%APPDATA%\WWTHID\SimAppPro\WWTHID.log`.

The log writes an `InputData` line per device per poll (~120 KB/s), so truncate it
immediately before the action. `HidData` lines are the decoded vendor-channel
frames, already labelled with the command name:

```text
HidData:<device>,<COMMAND>,<send|accept>,channel:2,id: 05 bf 00 00,len:3,data: 49 01 89 ...
```

`tools/parse_wwthid_log.py` decodes these and reconstructs the 14-byte frames.

The 64-byte channel is logged separately, as `Hiddata: send` and
`Hiddata: accpet` (sic) lines, with the report header removed. See "Driving a
pixel display".

**The log wraps, so a long capture keeps only its tail.** On 2026-09-18 a
five-minute capture kept only 13:05:42 to 13:06:08, and a later one with the
MCDU connected kept only its last 49 seconds out of about 43 MB. When the file
fills, SimAppPro truncates it in place and carries on, and almost all of it is
`InputData`. Run `python tools/tail_wwthid.py <out>` for the whole capture: it
follows the log, survives the truncation, and keeps every non-`InputData` line.
It stops after an hour, or `--seconds N`.

The setting survived a SimAppPro update on 2026-09-16, but that is not guaranteed;
re-check it after any update.

## Panel state is latched no host watchdog

Verified 2026-09-16: an LED set to 255 stayed lit indefinitely with SimAppPro
closed, our process exited and no handle open, so nothing was sending
`ONLINE_HEARTBEAT` at all.

**The segment display latches the same way.** Verified 2026-09-17 on a
CarrierAce UFC: a pattern written with `SET_LCDS` was still on the glass
minutes later with the writing process exited and no handle open. Writing zeros
to all 24 groups blanks it.

So a display is not self-clearing state that lapses when we stop feeding it. It
is exactly as sticky as a lamp, and needs the same treatment: blanked on
mission end and on shutdown, or a user who quits mid-flight is left reading a
stale frequency forever.

The daemon therefore **writes only on change**. No keepalive traffic, no periodic
re-assertion, nothing at idle. SimAppPro's 1-2s heartbeat is its own presence
tracking, not a device requirement.

Two consequences:

- LEDs persist after the daemon exits, so it must clear every LED it owns on
  shutdown and on mission end, or the panel is left frozen mid-flight.
- A dropped USB write would otherwise persist unnoticed. The panel acks every
  command, so the writer should verify the ack rather than fire and forget.

## Blinking comes from the source, not from us

Where a DCS cockpit lamp blinks F/A-18 gear in transit or damaged, for example
the module's own argument is oscillating. SimAppPro simply mirrors it. Faithful
mirroring of the source signal reproduces the blink for free.

A flash primitive in the rule engine is therefore an _override_ for behaviour DCS
does not already express, not core vocabulary. It does mean the sample rate must
not alias the source: DCS-BIOS exports at 30 Hz, comfortably above a 2-4 Hz lamp.

## Open questions

1. Are LEDs on parts behind a throttle base addressed via the base's part id or
   their own?
2. What does `SET_LEDX_WITH_DURATION` (`0x4b`) take as arguments?
3. The MCDU Captain takes the ICP's pixel-display frame (see "Driving a pixel
   display"), but only self-tests have been seen. How it draws, and its
   framebuffer's size and pixel format, need a capture of a live module.
4. The ICP's constant header bytes (`01 00 00` after the command, `00` before
   the length) and whether a write can exceed 270 bytes.

## Tools

- `tools/hid_probe.py` `list`, `listen` (read-only), `parts`, `led`, `blink`,
  `lcd`. Frame assembly refuses any command in the forbidden set. `lcd` renders
  text through a segment map and can blank a display.
- `tools/parse_wwthid_log.py` decode `WWTHID.log` into frames.
- `tools/decode_hid_capture.py` decode a USBPcap capture, if ever needed.
- `tools/decode_ufc_lcd.py` replay captured `SET_LCDS` frames through the
  segment map and read the glass back as text. Round-trips against synthetic
  frames, so a garbled decode means the capture or the map is wrong rather than
  the tool.
