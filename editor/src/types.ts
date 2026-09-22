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
  /** The device holding the `same_as` lamp, when it is not this one. */
  same_as_device?: string | null;
  on: number | null;
  off: number;
  note?: string;
}

/**
 * One piece of a field's content: characters the user typed, or a signal.
 *
 * A field is a chain of these drawn end to end, because a reading on its own
 * is rarely a readout. `250` says nothing that `RALT 250M` does not say
 * better, and the label, the number and the unit each want their own colour
 * and size.
 *
 * A piece carries `text` or `source`, never both. Everything else on it shapes
 * the one value it draws, so a chain can hold two signals that need different
 * treatment.
 */
export interface Span {
  /** Characters drawn exactly as given, reading nothing. */
  text?: string;
  /** Catalogue signal id. */
  source?: string;
  /**
   * Draw nothing, and take whatever cells the rest of the chain leaves.
   *
   * How content reaches both ends of a line. A label at the left and its value
   * hard against the right is a thing a CDU page does constantly, and counting
   * the blanks by hand only works until the value changes width, which is the
   * moment it matters. Two or more gaps split what is left evenly, which
   * spaces three pieces across a line.
   */
  gap?: boolean;
  /** What the gauge reads at each end of its travel. Numbers only. */
  reads?: [number, number];
  decimals?: number;
  /**
   * What to draw for each value of a number, in place of the number: `SEMI`
   * for a knob at 3. A value with no entry draws as the number. Numbers only.
   */
  value_aliases?: Record<string, string>;
  /** Values this module words differently from the glyph table. */
  aliases?: Record<string, string>;
  /** A second text signal whose `i` marks the characters to draw inverse. */
  format?: string;
  /** What colour a text grid draws this piece in. */
  colour?: string;
  /**
   * Draw in the grid's small font.
   *
   * Every font here draws fewer characters small than large, so marking a
   * piece small can take away a character that was fine at full size.
   */
  small?: boolean;
  /** Draw this whole piece inverse, on glass that draws inverse at all. */
  inverse?: boolean;
  /** A second text signal picking each cell's colour through `codes`. */
  colours?: { source: string; codes: Record<string, string> };
  /** Stand-in characters this module sends, rewritten one for one. */
  replace?: Record<string, string>;
  /**
   * Draw this piece in exactly this many cells, whatever it reads.
   *
   * Without one a chain only holds still at its ends: a reading that goes from
   * four characters to three pulls everything after it one cell left, so a
   * layout built around one width comes apart at another. A box is measured
   * before the gaps are, so what surrounds it never moves.
   *
   * It also bounds a gauge with no range, which nothing else does, and turns
   * "this may run past its cells" into an exact answer.
   */
  width?: number;
  /** Where the value sits inside `width`. Means nothing without one. */
  align?: "left" | "right" | "centre";
  /**
   * Fill this gap with a rule rather than with blanks.
   *
   * A rule between two pieces of a chain, where `divider` is a rule instead of
   * a whole field. Elastic, it takes whatever the two ends leave, which is
   * what the three separate fields it replaces could never do.
   */
  rule?: boolean;
  /** Characters set into the middle of this piece's rule. Needs a `width`. */
  label?: string;
  /** The label's colour, its own rather than the rule's. */
  label_colour?: string;
}

/**
 * One field of a display, and the content that fills it.
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
  /**
   * The pieces of this field, when it has more than one.
   *
   * A field of one piece is written flat instead, with that piece's `source`
   * and styling beside the cells, which is how every profile written before
   * chains is stored and how they have to stay: an update never rewrites a row
   * the user has changed, so a field that came back as a `content` array where
   * a `source` used to be would freeze every row against every later fix.
   *
   * Use `contentOf` rather than reading this, which gives the pieces whichever
   * way the field happens to be written.
   */
  content?: Span[];
  source: string;
  /** Characters drawn as given, for a field of one literal piece. */
  text?: string;
  /** A field of one gap, which is a blank run and refused as such. */
  gap?: boolean;
  small?: boolean;
  inverse?: boolean;
  replace?: Record<string, string>;
  colours?: { source: string; codes: Record<string, string> };
  /**
   * Draw a fixed rule across these cells instead of reading a signal.
   *
   * A screen only half used has no edge to it: the Apache exports only its
   * keyboard unit and the A-10C's CDU starts ten lines down, so the rest of
   * the glass is dark and the page runs off into it. A rule gives it one.
   * Text grids only, which is what `text_grid` on the display decides.
   */
  divider?: boolean;
  /**
   * What colour a text grid draws this in.
   *
   * Chosen in the window on a divider, and carried through untouched on a
   * field, whose colour follows what the aircraft's own CDU does. A new
   * divider starts on the colour the display's other fields agree on, so the
   * rule matches the page it is ruling rather than arriving white on a green
   * screen.
   */
  colour?: string;
  /**
   * Characters set into the middle of a rule, naming what it divides.
   *
   * A rule ends a page; a labelled rule says what the page was. It reads
   * nothing, like the rest of a divider. Dividers only.
   */
  label?: string;
  /**
   * The label's colour, which is its own rather than the rule's: a label drawn
   * in the line's colour reads as part of the line. Absent means it follows
   * the rule, which is what a label on an already coloured rule should look
   * like until somebody says otherwise.
   */
  label_colour?: string;
  /**
   * What the gauge reads in the cockpit at each end of its travel.
   *
   * For a number, and meaningless for a signal that already reports
   * characters. Absent draws the number as sent. DCS-BIOS gives a needle as a
   * position, not a quantity, and nothing says what the dial face is marked
   * with, so this is the user's to supply. Handles faces that start below
   * zero, and ones that run backwards.
   */
  reads?: [number, number];
  decimals?: number;
  /** What to draw for each value of a number, in place of the number. */
  value_aliases?: Record<string, string>;
  align?: "left" | "right" | "centre";
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
  /**
   * A second text signal, laid out across the same cells, whose `i` marks the
   * characters to draw inverse.
   *
   * The F-16 DED is the case DCS-BIOS exports: each line arrives as `DED_Ln`
   * and its highlighting as `DED_Ln_FORMAT`, one character for one.
   */
  format?: string;
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

/** A caution about one display field, by its index in `readouts`. */
export interface FieldCaution {
  readout: number;
  text: string;
}

/**
 * What a check found. Problems stop the profile loading; cautions and flags
 * do not.
 */
export interface Findings {
  problems: string[];
  /** About the profile as a whole, listed at the top of the page. */
  cautions: string[];
  /** About what one display field will draw, shown on that field. */
  field_cautions: FieldCaution[];
  flags: FlagView[];
  /** One line for the page, only when a flagged row needs the DCS-BIOS nightly. */
  notice: string | null;
}

/** Whether the converter daemon is running, and whether there is one to start. */
export interface ConverterState {
  running: boolean;
  /** The process holding the panels, when one does. */
  pid: number | null;
  /** False in a checkout with no daemon built beside the editor. */
  can_start: boolean;
}

export interface Profile {
  schema_version: number;
  name: string;
  author: string;
  profile_version: string;
  aircraft: string[];
  module: string;
  /**
   * The text grid font to upload, for an aircraft whose own CDU is not one
   * DCS-BIOS exports.
   *
   * An aircraft with a CDU of its own takes its font from the aircraft and is
   * offered no choice. This is for every other one, where nothing has an
   * opinion about what the screen should look like.
   */
  font?: string;
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
  /**
   * Devices that take another device's setup, keyed by the one that follows.
   * Only between variants, and one step deep. The follower's own rows are
   * kept and ignored while it follows.
   */
  follows?: Record<string, string>;
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
  /** Whether this glass can draw a character inverse, which is what decides
   *  whether a highlighting signal is worth offering. */
  draws_inverse: boolean;
  /** Whether this glass is a text grid, drawing characters from a font rather
   *  than from a fixed glyph table. Only a grid can draw a divider. */
  text_grid: boolean;
  /** The colours this glass draws, in the order the panel indexes them. Empty
   *  on anything but a text grid. Named by the backend so the window cannot
   *  offer one the hardware has no index for. */
  colours: string[];
  /** Every font this glass can be given. Empty on anything but a text grid. */
  fonts: FontChoice[];
  /**
   * Runtime aircraft name to the font its own CDU matches.
   *
   * An aircraft in here takes that font and is offered no choice: the glyphs
   * were drawn to match what its module sends, so picking another would only
   * be a way to draw the wrong symbol.
   */
  native_fonts: Record<string, string>;
}

/** One cell of a rule, as the backend lays it out. */
export interface RuleCell {
  text: string;
  /** Part of the label rather than the line, so it takes the label's colour. */
  label: boolean;
}

/** One font a text grid can be given. */
export interface FontChoice {
  /** Path relative to the display, which is what a profile stores. */
  file: string;
  /** The font's own name, which is the aircraft it was drawn for. */
  name: string;
  /** Every character it draws at full size. */
  large: string;
  /** Every character it draws small, a subset of `large` in all of them. */
  small: string;
}

/**
 * One font's glyphs, for drawing a line the way the panel will.
 *
 * Checking the typed characters against the alphabet is not enough, because
 * these fonts reuse slots: in the A-10C font `%` draws a question mark. A
 * window that showed the typed string would agree with the user and disagree
 * with the panel.
 */
export interface FontGlyphs {
  width: number;
  height: number;
  /** Character to its rows, `.` dark and `X` lit, at full size. */
  large: Record<string, string[]>;
  small: Record<string, string[]>;
}

export interface Device {
  key: string;
  display_name: string;
  product_name: string;
  leds: Led[];
  displays: DisplayInfo[];
  /**
   * Other devices that are this one under another name: the MCDU's three
   * seats, the MFD's three positions. A profile can point this one at any of
   * them rather than setting it up again.
   */
  variants: string[];
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
