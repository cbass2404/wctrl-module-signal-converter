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

### Persisted brightness, config offset 0x114

SimAppPro saves the PTO2's two dimmers to device flash as one little-endian word
at config offset `0x114` byte 0 is `Backlight`, byte 1 is `SL`:

```text
WRITE_CFG_DATA  offset 0x114  <- 00 de 00 00     after SL = 222
WRITE_CFG_DATA  offset 0x114  <- 6f de 00 00     after Backlight = 111
```

**Never write this.** `WRITE_CFG_DATA` is persistent and is on the forbidden
list, so our brightness changes stay volatile which is correct: we should not
wear flash or silently change a user's saved panel settings. `READ_CFG_DATA` at
the same offset is a safe read, and is how the daemon can warn that a stored
brightness of zero would leave correctly-bound lamps invisible.

### Value ranges differ per LED and are not yet settled

`Backlight` (0), `Landing_gear_lights` (1) and `SL` (2) are dimmers: SimAppPro
drives them with sliders and they were observed taking values across the whole
`0-255` span.

The fourteen indicators at indices 4-17 are **not** understood yet. What is known:

- SimAppPro only ever sends them `0` or `1`.
- Writing `1` to index 17 lit it very faintly; writing `255` appeared to leave it
  dark; writing `0` extinguished the faint light.

That ordering argues against "boolean lamp" if the value were a flag, `255`
would be truthy and light it. The likelier reading is that the value is a
brightness with a ceiling well below `255`, and that out-of-range writes are
rejected. Whether index 17 is also simply a dimmer lamp than its neighbours is
being checked separately.

Until measured, `data/devices.json` records these as `kind: indicator` with **no
`max`**. Do not default it to 255; that assumption already sent us down a blind
alley where every write acked and nothing visibly happened.

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
| `0xbe0e`, `0xbed0` | CarrierAce UFC / HUD             |
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
every indicator lit changed nothing visible. What it drives is still
unidentified, and it is marked unverified in `data/devices.json`.

This cost a full debugging session. The engine resolved the A-10C flap lamps
correctly, every write acked, and the lamps were invisible because `FLAG` sat
near 0 where SimAppPro had left it. A dimmer nothing writes is invisible state.

### Orion Throttle Base II part `0xbe60`

| Index | LED       |
| ----- | --------- |
| 0     | Backlight |
| 1     | A/A       |
| 2     | A/G       |

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

## LED state is latched no host watchdog

Verified 2026-09-16: an LED set to 255 stayed lit indefinitely with SimAppPro
closed, our process exited and no handle open, so nothing was sending
`ONLINE_HEARTBEAT` at all.

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

## Tools

- `tools/hid_probe.py` `list`, `listen` (read-only), `parts`, `led`, `blink`.
  Frame assembly refuses any command in the forbidden set.
- `tools/parse_wwthid_log.py` decode `WWTHID.log` into frames.
- `tools/decode_hid_capture.py` decode a USBPcap capture, if ever needed.
