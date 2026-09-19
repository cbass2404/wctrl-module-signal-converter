#!/usr/bin/env python3
"""Measure what `wctrl run` costs in CPU and memory.

  python tools/bench_daemon.py                          # dry run, A-10C, all scenarios
  python tools/bench_daemon.py --aircraft F-16C_50 --module F-16C_50
  python tools/bench_daemon.py --scenario stress --seconds 60
  python tools/bench_daemon.py --live                   # drives the real panels

No DCS needed. The tool plays a synthetic DCS-BIOS export stream onto the
multicast group (239.255.50.10:5010), shaped like the real one: a frame every
1/--hz seconds carrying the words that moved, plus the whole module map
re-exported every 300 ms. Values are random within each output's mask and
max_value, taken from the generated catalogue, so the engine sees every kind of
signal it would in flight.

Scenarios:

    idle     no stream at all; the daemon waiting for DCS
    typical  30 Hz, 20 integer outputs and 1 text field moving per frame
    stress   60 Hz, every output in the module rewritten every frame

--dry-run is the default so a benchmark never lights panels across the room;
--live opens the devices and measures the HID writes too. The daemon's own
output goes to NUL either way, since printing it would be the thing measured.

CPU is reported as a percentage of one core, from GetProcessTimes deltas, and
memory from GetProcessMemoryInfo. The first --warmup seconds are dropped so the
module-load flood and startup do not skew the steady state.
"""
import argparse
import ctypes as C
import ctypes.wintypes as W
import json
import os
import random
import socket
import struct
import subprocess
import sys
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GROUP = ("239.255.50.10", 5010)
SYNC = b"\x55\x55\x55\x55"

SCENARIOS = {
    "idle": None,
    "typical": dict(hz=30, changes=20, strings=1, stress=False),
    "stress": dict(hz=60, changes=0, strings=0, stress=True),
}

# ---------------------------------------------------------------- stream


class Stream:
    """The module's address space and the frames that write into it."""

    def __init__(self, catalogue, aircraft):
        cat = json.load(open(catalogue))
        self.ints, self.strs = [], []
        for s in cat["signals"]:
            for o in s["outputs"]:
                if o["type"] == "integer":
                    self.ints.append(o)
                elif o["type"] == "string":
                    self.strs.append(o)
        self.mem = {}
        name = aircraft.encode().ljust(24, b"\0")
        for i in range(12):
            self.mem[2 * i] = name[2 * i] | name[2 * i + 1] << 8
        for o in self.ints:
            self.mem.setdefault(o["address"] & ~1, 0)
        for o in self.strs:
            for off in range(o["max_length"] + 1):
                self.mem.setdefault((o["address"] + off) & ~1, 0x2020)
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
        self.sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 1)
        self.sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)
        self.frames = 0
        self.datagrams = 0

    def set_int(self, o):
        v = random.randint(0, o["max_value"])
        w = self.mem[o["address"]]
        self.mem[o["address"]] = (w & ~o["mask"]) | ((v << o["shift"]) & o["mask"])
        return [o["address"]]

    def set_str(self, o):
        n = o["max_length"]
        text = bytes(random.choice(b"ABCDEFGHIJ0123456789 ") for _ in range(n))
        touched = []
        for i, b in enumerate(text):
            byte_addr = o["address"] + i
            addr = byte_addr & ~1
            w = self.mem[addr]
            if byte_addr % 2 == 0:
                self.mem[addr] = (w & 0xFF00) | b
            else:
                self.mem[addr] = (w & 0x00FF) | b << 8
            touched.append(addr)
        return touched

    def send(self, addrs):
        # Contiguous words go in one block, as DCS-BIOS itself packs them.
        # Datagrams stay under 4 KB; the listener's buffer is 8 KB.
        runs, run = [], []
        for ad in sorted(set(addrs)):
            if run and ad != run[-1] + 2:
                runs.append(run)
                run = []
            run.append(ad)
        if run:
            runs.append(run)
        pkt = SYNC
        for r in runs:
            block = struct.pack("<HH", r[0], 2 * len(r))
            block += b"".join(struct.pack("<H", self.mem[x]) for x in r)
            if len(pkt) + len(block) > 4000:
                self.sock.sendto(pkt, GROUP)
                self.datagrams += 1
                pkt = SYNC
            pkt += block
        self.sock.sendto(pkt, GROUP)
        self.datagrams += 1

    def run(self, hz, changes, strings, stress, stop):
        period = 1 / hz
        start = time.perf_counter()
        last_full = -1.0
        while not stop.is_set():
            now = time.perf_counter()
            touched = []
            if stress:
                for o in self.ints:
                    touched += self.set_int(o)
                for o in self.strs:
                    touched += self.set_str(o)
            else:
                for o in random.sample(self.ints, min(changes, len(self.ints))):
                    touched += self.set_int(o)
                for o in random.sample(self.strs, min(strings, len(self.strs))):
                    touched += self.set_str(o)
            if now - last_full >= 0.3:
                touched = list(self.mem)
                last_full = now
            self.send(touched)
            self.frames += 1
            lag = start + self.frames * period - time.perf_counter()
            if lag > 0:
                time.sleep(lag)


# ---------------------------------------------------------------- sampling

k32 = C.WinDLL("kernel32", use_last_error=True)
psapi = C.WinDLL("psapi", use_last_error=True)


class PMC(C.Structure):
    _fields_ = [
        ("cb", W.DWORD),
        ("PageFaultCount", W.DWORD),
        ("PeakWorkingSetSize", C.c_size_t),
        ("WorkingSetSize", C.c_size_t),
        ("QuotaPeakPagedPoolUsage", C.c_size_t),
        ("QuotaPagedPoolUsage", C.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", C.c_size_t),
        ("QuotaNonPagedPoolUsage", C.c_size_t),
        ("PagefileUsage", C.c_size_t),
        ("PeakPagefileUsage", C.c_size_t),
        ("PrivateUsage", C.c_size_t),
    ]


def cpu_seconds(handle):
    c, e, k, u = W.FILETIME(), W.FILETIME(), W.FILETIME(), W.FILETIME()
    if not k32.GetProcessTimes(handle, C.byref(c), C.byref(e), C.byref(k), C.byref(u)):
        raise C.WinError(C.get_last_error())
    ft = lambda f: (f.dwHighDateTime << 32 | f.dwLowDateTime) / 1e7
    return ft(k) + ft(u)


def memory(handle):
    m = PMC()
    m.cb = C.sizeof(PMC)
    if not psapi.GetProcessMemoryInfo(handle, C.byref(m), m.cb):
        raise C.WinError(C.get_last_error())
    return m


def measure(args, scenario):
    exe = os.path.join(ROOT, "target", "release", "wctrl.exe")
    cmd = [exe, "run", "--seconds", str(int(args.seconds + args.warmup) + 2)]
    if not args.live:
        cmd.append("--dry-run")
    proc = subprocess.Popen(
        cmd, cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    handle = int(proc._handle)

    stop = threading.Event()
    stream = None
    if SCENARIOS[scenario]:
        stream = Stream(args.catalogue, args.aircraft)
        t = threading.Thread(target=stream.run, args=(*SCENARIOS[scenario].values(), stop))
        t.start()

    time.sleep(args.warmup)
    cpu0, t0 = cpu_seconds(handle), time.perf_counter()
    frames0 = stream.frames if stream else 0
    samples = []
    prev_cpu, prev_t = cpu0, t0
    while time.perf_counter() - t0 < args.seconds:
        time.sleep(args.interval)
        if proc.poll() is not None:
            sys.exit(f"wctrl exited early with code {proc.returncode}")
        cpu, now = cpu_seconds(handle), time.perf_counter()
        m = memory(handle)
        samples.append((100 * (cpu - prev_cpu) / (now - prev_t), m.WorkingSetSize, m.PrivateUsage))
        prev_cpu, prev_t = cpu, now
    cpu1, t1 = cpu_seconds(handle), time.perf_counter()
    m = memory(handle)
    frames = (stream.frames - frames0) if stream else 0

    # Let --seconds run out rather than killing it: TerminateProcess skips the
    # daemon's clear on exit, and the panels latch whatever was last written.
    stop.set()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.terminate()
        proc.wait()

    mb = 1 / (1024 * 1024)
    return dict(
        scenario=scenario,
        fps=frames / (t1 - t0),
        cpu_avg=100 * (cpu1 - cpu0) / (t1 - t0),
        cpu_peak=max(s[0] for s in samples),
        ws_avg=sum(s[1] for s in samples) / len(samples) * mb,
        ws_peak=m.PeakWorkingSetSize * mb,
        private=max(s[2] for s in samples) * mb,
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--scenario", choices=[*SCENARIOS, "all"], default="all")
    ap.add_argument("--aircraft", default="A-10C_2", help="name written to _ACFT_NAME")
    ap.add_argument("--module", default="A-10C", help="catalogue file to draw outputs from")
    ap.add_argument("--seconds", type=float, default=30, help="measured window per scenario")
    ap.add_argument("--warmup", type=float, default=3)
    ap.add_argument("--interval", type=float, default=1, help="sample period for peaks")
    ap.add_argument("--live", action="store_true", help="drive the real panels")
    args = ap.parse_args()
    args.catalogue = os.path.join(ROOT, "data", "catalogue", args.module + ".json")

    if not os.path.exists(os.path.join(ROOT, "target", "release", "wctrl.exe")):
        sys.exit("build it first: cargo build --release --bin wctrl")
    if not os.path.exists(args.catalogue):
        sys.exit(f"{args.catalogue} missing - build it with: cargo run --bin wctrl -- catalogue")

    mode = "live, panels driven" if args.live else "dry run"
    print(f"wctrl run ({mode}), {args.aircraft}, {args.seconds:g}s per scenario\n")
    print(f"{'scenario':<9} {'frames/s':>8} {'CPU avg':>8} {'CPU peak':>9} "
          f"{'WS avg':>8} {'WS peak':>8} {'private':>8}")
    names = list(SCENARIOS) if args.scenario == "all" else [args.scenario]
    for name in names:
        r = measure(args, name)
        print(f"{r['scenario']:<9} {r['fps']:>8.1f} {r['cpu_avg']:>7.2f}% {r['cpu_peak']:>8.2f}% "
              f"{r['ws_avg']:>6.1f}MB {r['ws_peak']:>6.1f}MB {r['private']:>6.1f}MB", flush=True)
    print("\nCPU is percent of one core. WS is working set; private is committed memory.")


if __name__ == "__main__":
    main()
