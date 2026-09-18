#!/usr/bin/env python3
"""WinCtrl HID tool - part discovery and LED control.

  python tools/hid_probe.py list
  python tools/hid_probe.py listen [--pid 0xbf05] [--seconds 10]
  python tools/hid_probe.py parts  [--pid 0xbf05]
  python tools/hid_probe.py led    [--pid 0xbf05] [--part 0xbf05] --index 1 --value 255
  python tools/hid_probe.py blink  [--pid 0xbf05] [--part 0xbf05] --index 1
  python tools/hid_probe.py lcd    [--pid 0xbede] [--part 0xbed0] --text HELLO --at 10
  python tools/hid_probe.py lcd    --clear
  python tools/hid_probe.py cfg    [--pid 0xbf06] [--part 0xbf06] --offset 0xc8

Wire protocol (14-byte reports, see docs/PROTOCOL.md):

    byte  0     0x02   report id
    bytes 1-4   uint32 target part id, little-endian; 0x00000001 broadcasts
    byte  5     len    significant bytes of data (1..8)
    bytes 6-13  data   payload; data[0] is the command

Replies carry the responding part's id with 0x1000 added.
"""
import argparse
import ctypes as C
import os
import sys
import time

setup = C.WinDLL("setupapi")
hid = C.WinDLL("hid")
k32 = C.WinDLL("kernel32")

GENERIC_READ, GENERIC_WRITE = 0x80000000, 0x40000000
FILE_SHARE_RW, OPEN_EXISTING, FILE_FLAG_OVERLAPPED = 3, 3, 0x40000000
DIGCF_PRESENT, DIGCF_DEVICEINTERFACE = 0x02, 0x10
WAIT_OBJECT_0, ERROR_IO_PENDING = 0x0, 997
INVALID = C.c_void_p(-1).value

VENDOR_ID = 0x4098
REPORT_ID = 0x02
BROADCAST = 0x00000001
PART_REPLY_BIAS = 0x1000

# Command codes as they appear on the wire (data[0]).
ONLINE_HEARTBEAT = 0x00
REQUEST_DEVICE_HW = 0x01
REQUEST_DEVICE_FW = 0x02
REQUEST_DEVICE_SN = 0x03
DEVICE_RESTART = 0x04
READ_CFG_DATA = 0x05
WRITE_CFG_DATA = 0x06
REQUEST_DEVICE_MODE = 0x18
SET_LEDX = 0x49
SET_LEDX_WITH_DURATION = 0x4B
SET_LCDS = 0x4C

# Commands that alter persistent state or interrupt the device. This tool
# refuses to place any of them in an outgoing frame.
FORBIDDEN = {
    0x04: "DEVICE_RESTART",
    0x06: "WRITE_CFG_DATA",
    0x20: "START_UPDATE",
    0x21: "UPDATE_DATA",
    0x22: "UPDATE_DATA_LEN",
    0x23: "UPDATE_DATA_CRC",
    0x25: "READ_UPDATE_OFFSET",
    0x40: "ENTER_UPDATA_MODE",
    0x43: "SET_USE_COUNTS",
    0x47: "CALIBRATION_CMD_START",
    0x48: "CALIBRATION_CMD_FINISH",
    0x56: "WRITE_PARAM_DATA",
}


class GUID(C.Structure):
    _fields_ = [("d1", C.c_ulong), ("d2", C.c_ushort), ("d3", C.c_ushort), ("d4", C.c_ubyte * 8)]


class SP_DEVICE_INTERFACE_DATA(C.Structure):
    _fields_ = [("cbSize", C.c_ulong), ("guid", GUID), ("Flags", C.c_ulong),
                ("Reserved", C.POINTER(C.c_ulonglong))]


class SP_DEVICE_INTERFACE_DETAIL_DATA_W(C.Structure):
    _fields_ = [("cbSize", C.c_ulong), ("DevicePath", C.c_wchar * 1024)]


class HIDD_ATTRIBUTES(C.Structure):
    _fields_ = [("Size", C.c_ulong), ("VendorID", C.c_ushort),
                ("ProductID", C.c_ushort), ("VersionNumber", C.c_ushort)]


class HIDP_CAPS(C.Structure):
    _fields_ = [("Usage", C.c_ushort), ("UsagePage", C.c_ushort),
                ("InputReportByteLength", C.c_ushort), ("OutputReportByteLength", C.c_ushort),
                ("FeatureReportByteLength", C.c_ushort), ("Reserved", C.c_ushort * 17),
                ("NumberLinkCollectionNodes", C.c_ushort), ("NumberInputButtonCaps", C.c_ushort),
                ("NumberInputValueCaps", C.c_ushort), ("NumberInputDataIndices", C.c_ushort),
                ("NumberOutputButtonCaps", C.c_ushort), ("NumberOutputValueCaps", C.c_ushort),
                ("NumberOutputDataIndices", C.c_ushort), ("NumberFeatureButtonCaps", C.c_ushort),
                ("NumberFeatureValueCaps", C.c_ushort), ("NumberFeatureDataIndices", C.c_ushort)]


class OVERLAPPED(C.Structure):
    _fields_ = [("Internal", C.POINTER(C.c_ulong)), ("InternalHigh", C.POINTER(C.c_ulong)),
                ("Off", C.c_ulong), ("OffHigh", C.c_ulong), ("hEvent", C.c_void_p)]


setup.SetupDiGetClassDevsW.restype = C.c_void_p
k32.CreateFileW.restype = C.c_void_p
k32.CreateEventW.restype = C.c_void_p


def enumerate_devices(vid=VENDOR_ID):
    guid = GUID()
    hid.HidD_GetHidGuid(C.byref(guid))
    dev_info = setup.SetupDiGetClassDevsW(C.byref(guid), None, None,
                                          DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
    index = 0
    while True:
        did = SP_DEVICE_INTERFACE_DATA()
        did.cbSize = C.sizeof(did)
        if not setup.SetupDiEnumDeviceInterfaces(C.c_void_p(dev_info), None, C.byref(guid),
                                                 index, C.byref(did)):
            break
        index += 1
        need = C.c_ulong()
        setup.SetupDiGetDeviceInterfaceDetailW(C.c_void_p(dev_info), C.byref(did), None, 0,
                                               C.byref(need), None)
        detail = SP_DEVICE_INTERFACE_DETAIL_DATA_W()
        detail.cbSize = 8
        if not setup.SetupDiGetDeviceInterfaceDetailW(C.c_void_p(dev_info), C.byref(did),
                                                      C.byref(detail), need, C.byref(need), None):
            continue
        handle = k32.CreateFileW(detail.DevicePath, 0, FILE_SHARE_RW, None, OPEN_EXISTING, 0, None)
        if handle in (None, 0, INVALID):
            continue
        attrs = HIDD_ATTRIBUTES()
        attrs.Size = C.sizeof(attrs)
        caps = HIDP_CAPS()
        if hid.HidD_GetAttributes(C.c_void_p(handle), C.byref(attrs)) and attrs.VendorID == vid:
            preparsed = C.c_void_p()
            if hid.HidD_GetPreparsedData(C.c_void_p(handle), C.byref(preparsed)):
                hid.HidP_GetCaps(preparsed, C.byref(caps))
                hid.HidD_FreePreparsedData(preparsed)
            yield attrs.ProductID, detail.DevicePath, caps
        k32.CloseHandle(C.c_void_p(handle))
    setup.SetupDiDestroyDeviceInfoList(C.c_void_p(dev_info))


def find(pid):
    for found, path, caps in enumerate_devices():
        if found == pid:
            return path, caps
    sys.exit("PID 0x%04x not found. Run `list` to see what is connected." % pid)


def open_rw(path):
    handle = k32.CreateFileW(path, GENERIC_READ | GENERIC_WRITE, FILE_SHARE_RW,
                             None, OPEN_EXISTING, FILE_FLAG_OVERLAPPED, None)
    if handle in (None, 0, INVALID):
        sys.exit("Could not open device read/write.")
    hid.HidD_SetNumInputBuffers(C.c_void_p(handle), 64)
    return handle


def read_report(handle, size, timeout_ms):
    buf = (C.c_ubyte * size)()
    ov = OVERLAPPED()
    ov.hEvent = k32.CreateEventW(None, True, False, None)
    got = C.c_ulong()
    ok = k32.ReadFile(C.c_void_p(handle), buf, size, C.byref(got), C.byref(ov))
    try:
        if not ok and k32.GetLastError() != ERROR_IO_PENDING:
            return None
        if k32.WaitForSingleObject(C.c_void_p(ov.hEvent), timeout_ms) != WAIT_OBJECT_0:
            k32.CancelIo(C.c_void_p(handle))
            return None
        k32.GetOverlappedResult(C.c_void_p(handle), C.byref(ov), C.byref(got), False)
        return bytes(buf[:got.value])
    finally:
        k32.CloseHandle(C.c_void_p(ov.hEvent))


def write_report(handle, frame, timeout_ms=300):
    buf = (C.c_ubyte * len(frame)).from_buffer_copy(bytes(frame))
    written = C.c_ulong()
    ov = OVERLAPPED()
    ov.hEvent = k32.CreateEventW(None, True, False, None)
    try:
        ok = k32.WriteFile(C.c_void_p(handle), buf, len(frame), C.byref(written), C.byref(ov))
        err = k32.GetLastError()
        if not ok and err != ERROR_IO_PENDING:
            return False, "WriteFile failed, GetLastError=%d" % err
        if k32.WaitForSingleObject(C.c_void_p(ov.hEvent), timeout_ms) != WAIT_OBJECT_0:
            k32.CancelIo(C.c_void_p(handle))
            return False, "write timed out"
        return True, "%d bytes" % written.value
    finally:
        k32.CloseHandle(C.c_void_p(ov.hEvent))


def build(part_id, data, report_len=14):
    """Assemble one command frame."""
    if not 1 <= len(data) <= 8:
        raise ValueError("data must be 1..8 bytes, got %d" % len(data))
    if data[0] in FORBIDDEN:
        raise SystemExit("guard: refusing to send %s (0x%02x)"
                         % (FORBIDDEN[data[0]], data[0]))
    frame = bytearray(report_len)
    frame[0] = REPORT_ID
    frame[1:5] = int(part_id).to_bytes(4, "little")
    frame[5] = len(data)
    frame[6:6 + len(data)] = bytes(data)
    return bytes(frame)


def drain(handle, size, seconds=0.3):
    """Discard queued reports; time-bounded because report 0x01 streams."""
    end = time.time() + seconds
    while time.time() < end:
        if not read_report(handle, size, 50):
            break


def replies(handle, size, seconds):
    """Collect vendor-channel (0x02) reports for `seconds`."""
    out, end = [], time.time() + seconds
    while time.time() < end:
        r = read_report(handle, size, 120)
        if r and r[0] == REPORT_ID:
            out.append(r)
    return out


def decode(report):
    part = int.from_bytes(report[1:5], "little")
    length = report[5]
    return part, part - PART_REPLY_BIAS, length, report[6:6 + max(length, 0)]


def printable(data):
    return "".join(chr(b) if 32 <= b < 127 else "." for b in data)


def cmd_list(args):
    for pid, path, caps in enumerate_devices():
        print("PID=0x%04x  in=%3d out=%3d  usage=%#06x/%#04x"
              % (pid, caps.InputReportByteLength, caps.OutputReportByteLength,
                 caps.UsagePage, caps.Usage))
        print("    %s" % path)


def cmd_listen(args):
    path, caps = find(args.pid)
    size = caps.InputReportByteLength
    print("Listening to 0x%04x for %ds. Nothing is transmitted.\n" % (args.pid, args.seconds))
    handle = open_rw(path)
    seen, count = {}, 0
    end = time.time() + args.seconds
    try:
        while time.time() < end:
            r = read_report(handle, size, 500)
            if not r:
                continue
            count += 1
            if r not in seen:
                seen[r] = 0
                print("  new  %s  |%s|" % (" ".join("%02x" % b for b in r), printable(r)))
            seen[r] += 1
    finally:
        k32.CloseHandle(C.c_void_p(handle))
    print("\n%d reports, %d distinct." % (count, len(seen)))


def cmd_parts(args):
    """Broadcast a heartbeat; every sub-part answers with its own id."""
    path, caps = find(args.pid)
    inn, outn = caps.InputReportByteLength, caps.OutputReportByteLength
    frame = build(BROADCAST, [ONLINE_HEARTBEAT], outn)
    print("Broadcasting ONLINE_HEARTBEAT on 0x%04x" % args.pid)
    print("  %s\n" % " ".join("%02x" % b for b in frame))
    handle = open_rw(path)
    try:
        drain(handle, inn)
        ok, detail = write_report(handle, frame)
        if not ok:
            sys.exit("write failed: %s" % detail)
        found = {}
        for r in replies(handle, inn, args.seconds):
            raw, part, length, data = decode(r)
            found.setdefault(part, (raw, length, data))
        if not found:
            print("No replies.")
            return
        for part in sorted(found):
            raw, length, data = found[part]
            print("  part 0x%04x  (reply id 0x%04x)  len=%d  data=%s"
                  % (part, raw, length, " ".join("%02x" % b for b in data)))
    finally:
        k32.CloseHandle(C.c_void_p(handle))


def cmd_cfg(args):
    """Read four bytes of a part's saved configuration. Read only: the write
    command is on the forbidden list and cannot be built here."""
    path, caps = find(args.pid)
    inn, outn = caps.InputReportByteLength, caps.OutputReportByteLength
    off = args.offset
    frame = build(args.part, [READ_CFG_DATA, off & 0xff, off >> 8 & 0xff, off >> 16 & 0xff], outn)
    print("READ_CFG_DATA part 0x%04x offset 0x%03x" % (args.part, off))
    print("  " + " ".join("%02x" % b for b in frame))
    handle = open_rw(path)
    try:
        drain(handle, inn)
        ok, detail = write_report(handle, frame)
        if not ok:
            sys.exit("write failed: %s" % detail)
        seen = False
        for r in replies(handle, inn, args.seconds):
            raw, part, length, data = decode(r)
            if data and data[0] == READ_CFG_DATA:
                seen = True
                print("  part 0x%04x  len=%d  data=%s"
                      % (part, length, " ".join("%02x" % b for b in data)))
        if not seen:
            print("No READ_CFG_DATA reply.")
    finally:
        k32.CloseHandle(C.c_void_p(handle))


def cmd_led(args):
    path, caps = find(args.pid)
    inn, outn = caps.InputReportByteLength, caps.OutputReportByteLength
    frame = build(args.part, [SET_LEDX, args.index, args.value], outn)
    print("SET_LEDX part 0x%04x index %d value %d" % (args.part, args.index, args.value))
    print("  %s" % " ".join("%02x" % b for b in frame))
    handle = open_rw(path)
    try:
        ok, detail = write_report(handle, frame)
        print("  write: %s (%s)" % ("OK" if ok else "FAILED", detail))
        for r in replies(handle, inn, 0.4):
            raw, part, length, data = decode(r)
            print("  reply part 0x%04x len=%d data=%s"
                  % (part, length, " ".join("%02x" % b for b in data)))
    finally:
        k32.CloseHandle(C.c_void_p(handle))


def load_display(path):
    """Read a segment map written by tools/gen, e.g. data/displays/ufc1.json."""
    import json
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)["displays"][0]


def draw(display, buf, at, text, whole=False):
    """Light `text` starting at cell `at`, one character per cell.

    Multi-character glyphs exist (a two-character field can occupy one cell),
    but this walks one character per cell, which is what a legibility test
    wants. Pass `whole` to look the entire text up as one glyph instead, which
    is how the daemon draws a field on a single cell. Unknown characters are
    left blank rather than guessed at.
    """
    cells, glyphs = display["cells"], display["glyphs"]
    spellings = display.get("spellings", {})
    pieces = [text] if whole else list(text)
    for offset, piece in enumerate(pieces):
        index = at + offset
        if index >= len(cells):
            print("  cell %d is past the end of the display, stopping" % index)
            break
        cell = cells[index]
        table = glyphs[cell["shape"]]
        # Same order the daemon uses: the value as given wins, and a spelling
        # only rescues one the table does not have.
        lit = table.get(piece)
        if lit is None and piece in spellings:
            lit = table.get(spellings[piece])
            if lit is not None:
                print("  %r is spelled %r on this display" % (piece, spellings[piece]))
        if lit is None:
            print("  no %s glyph for %r, leaving cell %d blank"
                  % (cell["shape"], piece, index))
            lit = []
        for slot, bit in enumerate(cell["segments"]):
            if slot in lit:
                buf[bit // 8] |= 1 << (bit % 8)
            else:
                buf[bit // 8] &= ~(1 << (bit % 8)) & 0xFF


def cmd_lcd(args):
    display = load_display(args.map)
    nbytes, gsize = display["buffer_bytes"], display["group_bytes"]
    buf = bytearray(nbytes)
    if not args.clear:
        draw(display, buf, args.at, args.text, args.whole)

    path, caps = find(args.pid)
    inn, outn = caps.InputReportByteLength, caps.OutputReportByteLength
    groups = nbytes // gsize
    what = "clearing" if args.clear else ("drawing %r at cell %d" % (args.text, args.at))
    print("SET_LCDS part 0x%04x: %s, %d groups" % (args.part, what, groups))

    # Every group is sent, not just the changed ones. This tool has no idea what
    # is currently on the glass, so a full write is the only way to leave it in
    # a known state.
    handle = open_rw(path)
    sent = 0
    try:
        for g in range(groups):
            chunk = buf[g * gsize:(g + 1) * gsize]
            frame = build(args.part, [SET_LCDS, g] + list(chunk), outn)
            ok, detail = write_report(handle, frame)
            if not ok:
                print("  group %d FAILED (%s)" % (g, detail))
                break
            sent += 1
            time.sleep(0.002)
        print("  %d/%d groups written" % (sent, groups))
        acked = replies(handle, inn, 0.3)
        print("  %d replies" % len(acked))
    finally:
        k32.CloseHandle(C.c_void_p(handle))


def cmd_blink(args):
    path, caps = find(args.pid)
    outn = caps.OutputReportByteLength
    on = build(args.part, [SET_LEDX, args.index, args.value], outn)
    off = build(args.part, [SET_LEDX, args.index, 0], outn)
    print("Blinking part 0x%04x index %d, %d times." % (args.part, args.index, args.count))
    for n in range(args.countdown, 0, -1):
        print("  starting in %d..." % n)
        time.sleep(1.0)
    handle = open_rw(path)
    try:
        for i in range(args.count):
            write_report(handle, on)
            time.sleep(0.5)
            write_report(handle, off)
            time.sleep(0.4)
            print("  blink %d/%d" % (i + 1, args.count))
    finally:
        k32.CloseHandle(C.c_void_p(handle))


parser = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
sub = parser.add_subparsers(dest="cmd", required=True)
sub.add_parser("list").set_defaults(func=cmd_list)

p = sub.add_parser("listen")
p.set_defaults(func=cmd_listen)
p.add_argument("--pid", type=lambda s: int(s, 0), default=0xBF05)
p.add_argument("--seconds", type=int, default=10)

p = sub.add_parser("parts")
p.set_defaults(func=cmd_parts)
p.add_argument("--pid", type=lambda s: int(s, 0), default=0xBF05)
p.add_argument("--seconds", type=float, default=1.5)

p = sub.add_parser("cfg")
p.set_defaults(func=cmd_cfg)
p.add_argument("--pid", type=lambda s: int(s, 0), default=0xBF06)
p.add_argument("--part", type=lambda s: int(s, 0), default=0xBF06)
p.add_argument("--offset", type=lambda s: int(s, 0), required=True)
p.add_argument("--seconds", type=float, default=0.5)

p = sub.add_parser("led")
p.set_defaults(func=cmd_led)
p.add_argument("--pid", type=lambda s: int(s, 0), default=0xBF05)
p.add_argument("--part", type=lambda s: int(s, 0), default=0xBF05)
p.add_argument("--index", type=int, default=1)
p.add_argument("--value", type=int, default=255)

p = sub.add_parser("blink")
p.set_defaults(func=cmd_blink)
p.add_argument("--pid", type=lambda s: int(s, 0), default=0xBF05)
p.add_argument("--part", type=lambda s: int(s, 0), default=0xBF05)
p.add_argument("--index", type=int, default=1)
p.add_argument("--value", type=int, default=255)
p.add_argument("--count", type=int, default=5)
p.add_argument("--countdown", type=int, default=6)

p = sub.add_parser("lcd")
p.set_defaults(func=cmd_lcd)
p.add_argument("--pid", type=lambda s: int(s, 0), default=0xBEDE)
p.add_argument("--part", type=lambda s: int(s, 0), default=0xBED0)
p.add_argument("--map", default=os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "data", "displays", "ufc1.json"))
p.add_argument("--text", default="")
p.add_argument("--at", type=int, default=0, help="first cell to draw into")
p.add_argument("--clear", action="store_true", help="blank the whole display")
p.add_argument("--whole", action="store_true",
               help="look --text up as one glyph on one cell, the way a field is drawn")

args = parser.parse_args()
args.func(args)
