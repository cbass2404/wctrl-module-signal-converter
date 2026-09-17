// Mirrors the shapes `editor/src-tauri/src/view.rs` serialises, and the profile
// shapes `wctrl-config` serialises. Kept as plain types rather than generated,
// because there are few of them and a generator is another thing to install.

/** `{ "equals": 1 }`, `{ "between": [21000, 25000] }`, and so on. */
export type OnWhen =
  | { equals: number }
  | { in: number[] }
  | { gte: number }
  | { lte: number }
  | { between: [number, number] }
  | { scale: [number, number] };

export interface Condition {
  source: string;
  on_when: OnWhen;
}

/**
 * One lamp and what drives it.
 *
 * `conditions` is a list, and every one of them must hold. A single-condition
 * binding is the common case but not the shape: the A-10C half-flaps lamp needs
 * the lever at MVR *and* the gauge inside the half window, because the flaps
 * pass through that window on the way to DN and would otherwise flash the lamp.
 * An empty list is a placeholder, which is a normal state rather than an error.
 */
export interface Binding {
  device: string;
  led: string;
  conditions: Condition[];
  on: number | null;
  off: number;
  note?: string;
}

export interface Profile {
  schema_version: number;
  name: string;
  author: string;
  profile_version: string;
  aircraft: string[];
  module: string;
  bindings: Binding[];
}

export interface Led {
  name: string;
  label: string;
  kind: "dimmer" | "indicator";
  max: number;
  on_value: number;
  dimmable: boolean;
  verified: boolean;
  part_id: number;
  index: number;
}

export interface Device {
  key: string;
  display_name: string;
  product_name: string;
  leds: Led[];
}

export interface ProfileSummary {
  file: string;
  name: string;
  module: string;
  aircraft: string[];
  bound: number;
  total: number;
  has_default: boolean;
  error: string | null;
}

export interface ModuleChoice {
  key: string;
  aircraft: string[];
  signals: number;
  lamps: number;
}

/** Human-readable summary of one condition, for a row that is not being edited. */
export function describeOnWhen(w: OnWhen): string {
  if ("equals" in w) return `= ${w.equals}`;
  if ("in" in w) return `is one of ${w.in.join(", ")}`;
  if ("gte" in w) return `>= ${w.gte}`;
  if ("lte" in w) return `<= ${w.lte}`;
  if ("between" in w) return `${w.between[0]} to ${w.between[1]}`;
  return `scaled from ${w.scale[0]} to ${w.scale[1]}`;
}
