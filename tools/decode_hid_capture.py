#!/usr/bin/env python3
"""Decode a USBPcap capture and annotate WinCtrl HID frames.

Usage:  python tools/decode_hid_capture.py capture.pcapng [--pid 0xbf05]

Reads pcapng (USBPcap link type 249) with no third-party dependencies and
prints every USB transfer payload, flagging frames that carry a known
WinCtrl command byte. Direction is inferred from the endpoint's high bit.
"""
import struct, sys

# Recovered from WWTHID.dll's name->code registration table.
COMMANDS = {
    0x01: "ONLINE_HEARTBEAT",      0x02: "REQUEST_DEVICE_HW",
    0x03: "REQUEST_DEVICE_FW",     0x04: "REQUEST_DEVICE_SN",
    0x05: "DEVICE_RESTART",        0x06: "READ_CFG_DATA",
    0x07: "WRITE_CFG_DATA",        0x18: "LOOP_BACK",
    0x20: "REQUEST_DEVICE_MODE",   0x21: "START_UPDATE",
    0x22: "UPDATE_DATA",           0x23: "UPDATE_DATA_LEN",
    0x24: "UPDATE_DATA_CRC",       0x25: "QUIT_UPDATA_MODE",
    0x40: "READ_UPDATE_OFFSET",    0x41: "ENTER_UPDATA_MODE",
    0x42: "SET_HIDE_MODE",         0x43: "REQUEST_HIDE_MODE",
    0x44: "SET_USE_COUNTS",        0x45: "REQUEST_USE_COUNTS",
    0x46: "REQUEST_AXIS_RAW_DATA", 0x47: "REQUEST_AXIS_DATA",
    0x48: "CALIBRATION_CMD_START", 0x49: "CALIBRATION_CMD_FINISH",
    0x4A: "SET_LEDX",              0x4B: "REQUEST_AXIS_CLIB_STATUS",
    0x4C: "SET_LEDX_WITH_DURATION",0x55: "SET_LCDS",
    0x56: "READ_PARAM_DATA",       0x57: "WRITE_PARAM_DATA",
}
# PTO2 LED indices, from SimAppPro's www/js/DeviceConfig.js
PTO2_LEDS = {
    0: "Backlight", 1: "Landing_gear_lights", 2: "SL", 4: "Master_Caution",
    5: "JETT", 6: "CTR", 7: "LI", 8: "LO", 9: "RO", 10: "RI", 11: "FLAPS",
    12: "NOSE", 13: "FULL", 14: "RIGHT", 15: "LEFT", 16: "HALF", 17: "HOOK",
}


def blocks(data):
    """Yield (block_type, body) from a pcapng stream."""
    off, endian = 0, "<"
    while off + 12 <= len(data):
        btype, blen = struct.unpack_from(endian + "II", data, off)
        if btype == 0x0A0D0D0A:  # section header: check byte order magic
            if struct.unpack_from("<I", data, off + 8)[0] != 0x1A2B3C4D:
                endian = ">"
                blen = struct.unpack_from(">I", data, off + 4)[0]
        if blen < 12 or off + blen > len(data):
            break
        yield btype, data[off + 8: off + blen - 4], endian
        off += blen


def usbpcap_payload(body, endian):
    """Split a USBPcap packet into (endpoint, transfer_type, data)."""
    hlen = struct.unpack_from(endian + "H", body, 0)[0]
    if hlen < 27 or hlen > len(body):
        return None
    endpoint, transfer = body[24], body[25]
    dlen = struct.unpack_from(endian + "I", body, 26)[0]
    return endpoint, transfer, body[hlen:hlen + dlen]


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 1
    raw = open(sys.argv[1], "rb").read()
    shown = 0
    for btype, body, endian in blocks(raw):
        if btype != 6:  # enhanced packet block
            continue
        # EPB: interface_id(4) ts_hi(4) ts_lo(4) cap_len(4) orig_len(4) then data
        cap_len = struct.unpack_from(endian + "I", body, 12)[0]
        parsed = usbpcap_payload(body[20:20 + cap_len], endian)
        if not parsed:
            continue
        endpoint, transfer, data = parsed
        if not data:
            continue
        direction = "IN " if endpoint & 0x80 else "OUT"
        hexs = " ".join(f"{b:02x}" for b in data)
        note = ""
        for i, b in enumerate(data[:4]):
            if b in COMMANDS:
                note = f"   <- {COMMANDS[b]} at byte {i}"
                if b in (0x4A, 0x4C) and len(data) > i + 2:
                    led, val = data[i + 1], data[i + 2]
                    note += f" | led {led}={PTO2_LEDS.get(led, '?')} value {val}"
                break
        print(f"{direction} ep=0x{endpoint:02x} len={len(data):2d}  {hexs}{note}")
        shown += 1
    if not shown:
        print("No USB payloads found. Confirm the capture used the USBPcap interface.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
