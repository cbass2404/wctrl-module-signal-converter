#!/usr/bin/env python3
"""Build the signal catalogue from DCS-BIOS module definitions.

  python tools/build_catalogue.py [--bios <dir>] [--out data/catalogue] [--stats]

DCS-BIOS ships a JSON description of every control in every module it supports.
Each readable control carries an address/mask/shift triple, which is exactly what
is needed to decode values out of the DCS-BIOS export stream - so the catalogue
is a straight transform of those files, with no Lua evaluation required.

Output is one file per module plus an index.json mapping the aircraft names DCS
reports at runtime (LoGetSelfData().Name) onto module keys.
"""
import argparse
import json
import re
import os
from collections import Counter

DEFAULT_BIOS = os.path.expandvars(
    r"%USERPROFILE%\Saved Games\DCS\Scripts\DCS-BIOS\doc\json")

# Not aircraft; these describe the stream itself or are shared fragments.
NON_MODULE = {"AircraftAliases", "MetadataStart", "MetadataEnd", "CommonData", "NS430"}

# control_type values that represent something a panel LED could mirror.
LAMP_TYPES = {"led"}


def bios_version(bios_dir):
    """Version string from BIOSConfig.lua, two levels up from doc/json.

    Stamped into the catalogue because DCS-BIOS allocates addresses
    sequentially as controls are defined: adding one control to a module shifts
    every later address in it. A catalogue built against a different release
    would read the wrong addresses, silently, so the runtime must be able to
    tell that the catalogue and the installed DCS-BIOS no longer agree.
    """
    config = os.path.join(bios_dir, "..", "..", "BIOSConfig.lua")
    try:
        with open(config, encoding="utf-8") as fh:
            match = re.search(r'version\s*=\s*"([^"]+)"', fh.read())
            return match.group(1) if match else "unknown"
    except OSError:
        return "unknown"


def load_aliases(bios_dir):
    """runtime aircraft name -> [module keys], as shipped by DCS-BIOS."""
    path = os.path.join(bios_dir, "AircraftAliases.json")
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


# A signal with no more than this many distinct values is offered as a labelled
# dropdown; anything wider becomes a numeric range input.
DISCRETE_LIMIT = 8

# "switch position -- 0 = Down, 1 = Mid,  2 = Up"
INLINE_LABEL = re.compile(r"(\d+)\s*=\s*([^,;]+)")


def value_labels(control, out):
    """Derive [{value,label}] for a discrete signal, or None if continuous.

    The editor constrains the 'turns on at' input to exactly these values, so a
    user can never pick something the signal cannot report.
    """
    max_value = out.get("max_value")
    if out.get("type") != "integer" or not isinstance(max_value, int):
        return None
    if max_value < 1 or max_value + 1 > DISCRETE_LIMIT:
        return None

    # `positions` is authoritative when present: index == reported value.
    positions = control.get("positions") or []
    inline = {int(v): text.strip()
              for v, text in INLINE_LABEL.findall(out.get("description", ""))}

    values = []
    for value in range(max_value + 1):
        label = None
        if value < len(positions) and positions[value]:
            label = str(positions[value])
        if value in inline:
            label = "%s (%s)" % (label, inline[value]) if label else inline[value]
        values.append({"value": value, "label": label or str(value)})
    return values


def convert_module(raw):
    """DCS-BIOS module JSON -> our flat signal list."""
    signals = []
    for category, controls in raw.items():
        for identifier, control in controls.items():
            outputs = []
            for out in control.get("outputs", []):
                if "address" not in out:
                    continue
                entry = {
                    "address": out["address"],
                    "mask": out.get("mask"),
                    "shift": out.get("shift_by", 0),
                    "max_value": out.get("max_value"),
                    "type": out.get("type", "integer"),
                    "description": out.get("description", ""),
                }
                # String outputs are sized by max_length, not max_value. Without
                # it the decoder cannot tell how many bytes to read, so every
                # display signal would be unreadable.
                if out.get("max_length") is not None:
                    entry["max_length"] = out["max_length"]
                values = value_labels(control, out)
                if values is not None:
                    entry["values"] = values
                    entry["discrete"] = True
                else:
                    entry["discrete"] = False
                outputs.append(entry)
            if not outputs:
                continue  # input-only control, nothing to read
            signals.append({
                "id": identifier,
                "category": control.get("category", category),
                "description": control.get("description", ""),
                "control_type": control.get("control_type", ""),
                "outputs": outputs,
            })
    signals.sort(key=lambda s: (s["category"], s["id"]))
    return signals


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bios", default=DEFAULT_BIOS)
    ap.add_argument("--out", default="data/catalogue")
    ap.add_argument("--stats", action="store_true")
    args = ap.parse_args()

    if not os.path.isdir(args.bios):
        raise SystemExit("DCS-BIOS json not found at %s" % args.bios)

    version = bios_version(args.bios)
    aliases = load_aliases(args.bios)
    by_module = {}
    for aircraft, modules in aliases.items():
        if not aircraft:
            continue
        for module in modules:
            by_module.setdefault(module, []).append(aircraft)

    os.makedirs(args.out, exist_ok=True)
    index, totals = {}, Counter()

    for name in sorted(os.listdir(args.bios)):
        if not name.endswith(".json"):
            continue
        key = name[:-5]
        if key in NON_MODULE:
            continue
        with open(os.path.join(args.bios, name), encoding="utf-8") as fh:
            raw = json.load(fh)
        signals = convert_module(raw)
        if not signals:
            continue

        lamps = [s for s in signals if s["control_type"] in LAMP_TYPES]
        record = {
            "module": key,
            "bios_version": version,
            "aircraft": sorted(by_module.get(key, [])),
            "signal_count": len(signals),
            "lamp_count": len(lamps),
            "signals": signals,
        }
        with open(os.path.join(args.out, name), "w", encoding="utf-8") as fh:
            json.dump(record, fh, indent=1)

        index[key] = {
            "aircraft": record["aircraft"],
            "signals": len(signals),
            "lamps": len(lamps),
        }
        totals["modules"] += 1
        totals["signals"] += len(signals)
        totals["lamps"] += len(lamps)

    with open(os.path.join(args.out, "index.json"), "w", encoding="utf-8") as fh:
        json.dump({"bios_version": version,
                   "source": os.path.abspath(args.bios),
                   "modules": index}, fh, indent=1)

    print("%d modules, %d signals, %d of them lamps -> %s"
          % (totals["modules"], totals["signals"], totals["lamps"], args.out))
    print("built from DCS-BIOS %s" % version)

    if args.stats:
        print("\n%-22s %7s %6s  %s" % ("module", "signals", "lamps", "aircraft"))
        for key in sorted(index, key=lambda k: -index[k]["lamps"]):
            row = index[key]
            names = ", ".join(row["aircraft"]) or "-"
            print("%-22s %7d %6d  %s"
                  % (key, row["signals"], row["lamps"], names[:60]))


if __name__ == "__main__":
    main()
