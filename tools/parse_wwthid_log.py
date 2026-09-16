#!/usr/bin/env python3
"""Decode SimAppPro's WWTHID.log into wire frames.

  python tools/parse_wwthid_log.py [log] [--cmd SET_LEDX] [--device PTO]

Enable the log with "HIDLog": true in %APPDATA%\\SimAppPro\\config.json
(read at startup only), then restart SimAppPro.

Lines look like:
  HidData:<device>,<COMMAND>,<send|accept>,channel:2,id: 05 bf 00 00,len:1,data: 00 ...

`InputData` lines are raw joystick polls and are ignored. Note WWTHID masks the
0x1000 reply bias off before logging, so `id` is the true part id either way.
"""
import argparse
import os
import re
from collections import Counter

DEFAULT_LOG = os.path.expandvars(r"%APPDATA%\WWTHID\SimAppPro\WWTHID.log")

LINE = re.compile(
    r"HidData:(?P<device>[^,]+),"
    r"(?P<cmd>[A-Z0-9_]+),"
    r"(?P<dir>send|accept),"
    r"channel:(?P<channel>\d+),"
    r"id:\s*(?P<id>(?:[0-9a-f]{2} ?){4}),"
    r"len:(?P<len>\d+),"
    r"data:\s*(?P<data>(?:[0-9a-f]{2} ?)+)"
)

# PTO2 LED indices as *claimed* by SimAppPro's DeviceConfig.js. Verified so far:
# 0, 1 and 4 only. Index 17 acks but lights nothing, and index 2 did not behave
# as a master brightness, so these are shown as unconfirmed labels rather than
# facts - the point of a capture is to replace them with what SimAppPro sends.
PTO2_LEDS_CLAIMED = {
    0: "Backlight", 1: "Landing_gear_lights", 2: "SL", 4: "Master_Caution",
    5: "JETT", 6: "CTR", 7: "LI", 8: "LO", 9: "RO", 10: "RI", 11: "FLAPS",
    12: "NOSE", 13: "FULL", 14: "RIGHT", 15: "LEFT", 16: "HALF", 17: "HOOK",
}
VERIFIED = {0, 1, 4}


def parse_hex(text):
    return [int(x, 16) for x in text.split()]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("log", nargs="?", default=DEFAULT_LOG)
    ap.add_argument("--cmd", help="only this command, e.g. SET_LEDX")
    ap.add_argument("--device", help="substring match on the device name")
    ap.add_argument("--dir", choices=("send", "accept"))
    ap.add_argument("--frames", action="store_true",
                    help="print the reconstructed 14-byte output report")
    args = ap.parse_args()

    counts, shown = Counter(), 0
    with open(args.log, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = LINE.search(line)
            if not m:
                continue
            counts[(m["cmd"], m["dir"])] += 1
            if args.cmd and m["cmd"] != args.cmd:
                continue
            if args.device and args.device.lower() not in m["device"].lower():
                continue
            if args.dir and m["dir"] != args.dir:
                continue

            ident = parse_hex(m["id"])
            part = int.from_bytes(bytes(ident), "little")
            length = int(m["len"])
            data = parse_hex(m["data"])[:length]

            note = ""
            if m["cmd"].startswith("SET_LEDX") and len(data) >= 3:
                index, value = data[1], data[2]
                claimed = PTO2_LEDS_CLAIMED.get(index)
                if claimed and index in VERIFIED:
                    note = "   index %d (%s) = %d" % (index, claimed, value)
                elif claimed:
                    note = "   index %d (claimed %s?) = %d" % (index, claimed, value)
                else:
                    note = "   index %d = %d" % (index, value)
            elif m["cmd"].endswith("CFG_DATA") and len(data) >= 4:
                # 24-bit little-endian config offset, then up to 4 data bytes.
                offset = data[1] | (data[2] << 8) | (data[3] << 16)
                rest = " ".join("%02x" % b for b in data[4:])
                note = "   offset 0x%03x%s" % (offset, "  <- " + rest if rest else "")

            print("%-7s part 0x%04x  %-22s len=%d  data=%s%s"
                  % (m["dir"], part, m["cmd"], length,
                     " ".join("%02x" % b for b in data), note))

            if args.frames:
                frame = bytearray(14)
                frame[0] = int(m["channel"])
                frame[1:5] = bytes(ident)
                frame[5] = length
                frame[6:6 + len(data)] = bytes(data)
                print("          frame: %s" % " ".join("%02x" % b for b in frame))
            shown += 1

    if not shown:
        print("No matching HidData lines.")
    print("\n-- all commands seen --")
    for (cmd, direction), n in sorted(counts.items(), key=lambda kv: -kv[1]):
        print("  %-24s %-7s %d" % (cmd, direction, n))


if __name__ == "__main__":
    main()
