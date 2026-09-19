// Rows this DCS-BIOS cannot back, as the last check found them.
//
// The check places each flag by index, but the window draws rows from the
// profile's own objects and redraws them on every edit. So a flag is resolved
// once, to the condition or field object it names, and marks are keyed on
// that: a row drawn later finds its mark without anyone threading indices
// through the editors, and a row deleted takes its mark with it.
//
// None of this withholds Save. The daemon runs the rest of the profile and
// turns these rows off; the file keeps them so they work again once DCS-BIOS
// is updated.

import type { FlagView, Profile } from "./types";

const reasons = new WeakMap<object, string>();
const slots = new WeakMap<object, HTMLElement>();
let marked: object[] = [];

function fill(slot: HTMLElement, text: string | undefined): void {
  slot.hidden = !text;
  slot.textContent = text ? `⚠ ${text}` : "";
}

/** The condition or field a flag names, if the profile still has it. */
function locate(profile: Profile, f: FlagView): object | undefined {
  switch (f.at) {
    case "condition":
      return profile.bindings[f.binding]?.conditions[f.index];
    case "branch":
      return profile.bindings[f.binding]?.any_of?.[f.branch]?.conditions[f.index];
    case "field":
      return profile.readouts?.[f.readout];
  }
}

/**
 * Where a row shows its mark. Empty and hidden until a check flags the row,
 * and filled at once when it already has been.
 */
export function flagSlot(row: object): HTMLElement {
  const slot = document.createElement("div");
  slot.className = "flag";
  slots.set(row, slot);
  fill(slot, reasons.get(row));
  return slot;
}

/**
 * Replace every mark with what the latest check found. `profile` must be the
 * one that was checked, or the indices point at the wrong rows.
 */
export function showFlags(profile: Profile, flags: FlagView[]): void {
  for (const row of marked) {
    reasons.delete(row);
    const slot = slots.get(row);
    if (slot) fill(slot, undefined);
  }
  marked = [];
  for (const f of flags) {
    const row = locate(profile, f);
    // A field reading two missing signals is flagged twice, and both say the
    // same thing about it.
    if (!row || reasons.has(row)) continue;
    reasons.set(row, f.text);
    marked.push(row);
    const slot = slots.get(row);
    if (slot) fill(slot, f.text);
  }
}
