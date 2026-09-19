// Mirrors the shapes `editor/src-tauri/src/view.rs` serialises, and the profile
// shapes `dsc-config` serialises. Kept as plain types rather than generated,
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
   * How the alternatives combine. Absent means the brightest wins; `"latest"`
   * follows the alternative whose signal changed last, which is how a lamp
   * follows one of two seats' knobs with nothing saying which seat is taken.
   */
  pick?: "brightest" | "latest";
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

/**
 * One field of a segment display, and the signal that feeds it.
 *
 * A field has exactly one owner: nothing chooses between two sources for the
 * same cells at runtime, because the cockpit has already decided what belongs
 * there, or the user has.
 */
export interface Readout {
  device: string;
  display: string;
  /** `"34"` for one cell, `"2-8"` for a run. */
  cells: string;
  source: string;
  /**
   * What the gauge reads in the cockpit at each end of its travel.
   *
   * Required for a number, meaningless for a signal that already reports
   * characters. DCS-BIOS gives a needle as a position, not a quantity, and
   * nothing says what the dial face is marked with, so this is the user's to
   * supply. Handles faces that start below zero, and ones that run backwards.
   */
  reads?: [number, number];
  decimals?: number;
  align?: "left" | "right";
  /** Values this module words differently from the glyph table. */
  aliases?: Record<string, string>;
  /**
   * Only paint this field from one crew station.
   *
   * DCS-BIOS exports the whole cockpit whatever seat you are in, so a multicrew
   * aircraft publishes both stations at once and a field cannot tell which
   * reading is yours. Two fields may share cells when their seats differ.
   *
   * Only offered on a module that publishes `SEAT_POSITION`, which is 5 of the
   * 50 catalogued.
   */
  seat?: number;
  note?: string;
}

/**
 * One condition or display field this DCS-BIOS cannot back, placed by index
 * into the profile that was checked. It loads and stays off.
 */
export type FlagView = { text: string } & (
  | { at: "condition"; binding: number; index: number }
  | { at: "branch"; binding: number; branch: number; index: number }
  | { at: "field"; readout: number }
);

/**
 * What a check found. Problems stop the profile loading; cautions and flags
 * do not.
 */
export interface Findings {
  problems: string[];
  cautions: string[];
  flags: FlagView[];
  /** One line for the page, only when a flagged row needs the DCS-BIOS nightly. */
  notice: string | null;
}

export interface Profile {
  schema_version: number;
  name: string;
  author: string;
  profile_version: string;
  aircraft: string[];
  module: string;
  bindings: Binding[];
  readouts?: Readout[];
  /**
   * Devices this aircraft should not drive at all.
   *
   * Not the same as binding nothing. An unbound device is still swept, so it
   * goes dark, which is what you want for a panel you can see. A disabled one
   * is never written to, which is what you want for a panel that is physically
   * covered: a WinWing ICP and UFC share a swing arm, and whichever is in use
   * hides the other.
   */
  disabled_devices?: string[];
}

export interface Led {
  name: string;
  label: string;
  kind: "dimmer" | "indicator";
  max: number;
  on_value: number;
  dimmable: boolean;
  verified: boolean;
  /** Anything worth knowing that the name does not say. Often empty. */
  note: string;
  /**
   * Lamps this dimmer hides at 0, by name. Non-empty marks a gate, such as the
   * PTO2's SL and FLAG, whose value at zero is its daylight floor.
   */
  governs: string[];
  part_id: number;
  index: number;
}

/** A segment display, as far as the window needs to know about one. */
/** One named area of the glass, offered in place of a raw cell run. */
export interface RegionInfo {
  name: string;
  /** `"34"` or `"30-33"`, the same spelling a readout stores. */
  cells: string;
  /** What the aircraft this panel was built for puts here. May be empty. */
  note: string;
}

export interface DisplayInfo {
  key: string;
  cells: number;
  /** Cell index to shape, so a run that cannot take letters can be flagged. */
  shapes: string[];
  regions: RegionInfo[];
}

export interface Device {
  key: string;
  display_name: string;
  product_name: string;
  leds: Led[];
  displays: DisplayInfo[];
}

/** A profile picked for import, before anything is written. */
export interface ImportPreview {
  /** Where it was picked from, handed back to `importProfile`. */
  path: string;
  name: string;
  author: string;
  module: string;
  aircraft: string[];
  bound: number;
  total: number;
  /** Rows reading something this DCS-BIOS cannot back. They load and stay off. */
  flagged: number;
  cautions: string[];
}

export interface ProfileSummary {
  file: string;
  name: string;
  module: string;
  aircraft: string[];
  /** Each aircraft's family, in order: which aircraft this profile could take. */
  families: string[];
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
  /**
   * True when this signal reports characters rather than a number.
   *
   * A lamp binding compares numbers, so the lamp picker hides these. A display
   * field is the opposite case and wants them, and needs no gauge range for one.
   */
  text: boolean;
  /** Characters in the field, for a text signal. Zero otherwise. */
  length: number;
  /** "0 if light is off, 1 if light is on", and the like. */
  reads: string;
  /** Non-empty for signals with few enough values to label individually. */
  values: ValueLabel[];
}

/** What the editor found when it checked the catalogue against DCS-BIOS. */
export interface CatalogueStatus {
  level: "ok" | "caution" | "error";
  text: string;
}

export interface ModuleChoice {
  key: string;
  aircraft: string[];
  signals: number;
  lamps: number;
}

/** A release other than the one running, from GitHub. */
export interface Update {
  /** VERSION.md as this build carries it. */
  current: string;
  /** The newest release, as its tag has it less the leading v. */
  latest: string;
}

/** One signal that moved in the cockpit, as learn mode reports it. */
export interface LearnChange {
  id: string;
  /** What it read before the first movement. Null if it was still arriving. */
  from: string | null;
  to: string;
  /**
   * How many times it has moved since the panel was opened.
   *
   * One is a switch being thrown. A hundred is a gauge, and the list is ordered
   * by this so the thing you just did comes out on top.
   */
  moves: number;
  /** Characters rather than a number, so the value is shown quoted. */
  text: boolean;
}

/** Everything the learn panel asks for on a poll. */
export interface LearnReport {
  module: string;
  listening: boolean;
  /**
   * Whether every signal has a starting value yet. Before this an empty list
   * means "still reading the cockpit" and after it means "nothing moved".
   */
  ready: boolean;
  datagrams: number;
  /** What DCS says it is flying, once the stream has said. */
  aircraft: string | null;
  error: string | null;
  changes: LearnChange[];
}
