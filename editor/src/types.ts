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
/** One alternative within `any_of`: conditions that must all hold together. */
export interface Branch {
  conditions: Condition[];
}

export interface Binding {
  device: string;
  led: string;
  conditions: Condition[];
  /**
   * Lit whenever the profile is active, reading nothing.
   *
   * Different from an empty condition list, which means "not decided yet" and
   * drives the lamp off. On a lamp that dims, this is also how a fixed
   * brightness is set. Mutually exclusive with `conditions`.
   */
  always?: boolean;
  /**
   * Alternatives, any one of which lights the lamp. Each branch holds only when
   * all of its own conditions hold, so this is a list of ANDs joined by OR.
   * Mutually exclusive with `conditions` and with `always`.
   */
  any_of?: Branch[];
  /**
   * Mirror another lamp on the same device, by name. A link rather than a copy,
   * so changing what the other lamp reads moves this one with it.
   *
   * Only meaningful between lamps that dim: an indicator takes 0 or 1 and has
   * no level to follow. Mutually exclusive with the other three forms.
   */
  same_as?: string | null;
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

export interface ValueLabel {
  value: number;
  label: string;
}

/** One bindable signal, as the typeahead and the hint box need it. */
export interface SignalView {
  id: string;
  description: string;
  category: string;
  control_type: string;
  lamp: boolean;
  max_value: number;
  /** "0 if light is off, 1 if light is on", and the like. */
  reads: string;
  /** Non-empty for signals with few enough values to label individually. */
  values: ValueLabel[];
}

export interface ModuleChoice {
  key: string;
  aircraft: string[];
  signals: number;
  lamps: number;
}
