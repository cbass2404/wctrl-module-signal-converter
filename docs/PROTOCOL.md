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

`0x02` is the only declared output report ID; Windows rejects any other in the HID
stack before it reaches the device. Handles open `GENERIC_READ | GENERIC_WRITE`
with `FILE_SHARE_READ | FILE_SHARE_WRITE` even while SimAppPro is running HID
access is shared and input reports reach every open handle. **No admin rights
required.**

The vendor channel is solicited: with SimAppPro closed the device sends nothing on
report `0x02` until addressed.

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
and the layout is one byte per dimmer, in index order:

| Part     | Offset  | Byte 0               | Byte 1         |
| -------- | ------- | -------------------- | -------------- |
| `0xbf05` | `0x114` | `Backlight`          | `SL`           |
| `0xbed0` | `0x0d8` | `INST_PNL_Backlight` | `LCDBacklight` |
| `0xbe0e` | `0x0c8` | `INST_PNL_Backlight` |                |

```text
WRITE_CFG_DATA  offset 0x114  <- 00 de 00 00     after PTO2 SL = 222
WRITE_CFG_DATA  offset 0x114  <- 6f de 00 00     after PTO2 Backlight = 111
WRITE_CFG_DATA  offset 0x0d8  <- 0f 7b ff ff     after UFC LCDBacklight = 123
WRITE_CFG_DATA  offset 0x0d8  <- d3 7b ff ff     after UFC panel = 211
WRITE_CFG_DATA  offset 0x0c8  <- fb ff ff ff     after HUD panel = 251
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
| `0xbb32`           | MCDU Captain                     |

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

Index 1 carries the vendor's name `Landing_gear_lights`, but setting it to 0 with
every indicator lit changed nothing visible. It is verified as a dimmer taking
0-255, but what it drives is still unidentified, so the name is not evidence.

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
3. The MCDU Captain (`0xbb36`) enumerates with 64-byte reports in both
   directions, unlike every other panel here at 14. Its screen is presumably
   not driven by `SET_LCDS` in this form.

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
