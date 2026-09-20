// Editing the fields of a display.
//
// A display field is not a lamp and is deliberately not shaped like one. A lamp
// asks "under what conditions"; a field asks "which cells, fed by what". There
// are no conditions here at all: a field has one owner, because on an aircraft
// that drives its own glass the cockpit has already decided what belongs there,
// and on any other the user has.
//
// Two things shape this file. A field's content is a chain of pieces, each
// either characters the user typed or a signal, each with its own colour and
// size, so `RALT 250M` is one field rather than three. And the window lists
// every area of every screen, in the order they sit on the glass, the way it
// lists every lamp of a device: adding a field used to append it to whatever
// was already there, so a new row landed at the bottom of the list however far
// up the panel it was drawn, and the only way to reorder was to delete
// everything and start again.

import { dividerRule, fontGlyphs } from "./api";
import { iconButton } from "./binding";
import { confirmAction } from "./confirm";
import { contentOf, isLiteral, kindOf, newSpan, setContent } from "./content";
import type { SpanKind } from "./content";
import { flagSlot } from "./flags";
import { noteEditor } from "./note";
import { signalPicker } from "./typeahead";
import type {
  Device,
  DisplayInfo,
  FontChoice,
  FontGlyphs,
  Profile,
  Readout,
  RegionInfo,
  RuleCell,
  SignalView,
  Span,
} from "./types";

/** The swatch beside each colour name, so the menu shows what it means. */
const SWATCH: Record<string, string> = {
  black: "#000000",
  amber: "#ff9d1c",
  white: "#f2f4f7",
  cyan: "#3fe0e0",
  green: "#36d14a",
  magenta: "#e668d8",
  red: "#f0564a",
  yellow: "#f2e14c",
  brown: "#9a6b3f",
  grey: "#9aa3ad",
  khaki: "#c3bb72",
};

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  ...kids: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  node.append(...kids);
  return node;
}

/** Parse `"34"` or `"2-8"`. Returns null for anything else. */
export function parseCells(text: string): [number, number] | null {
  const m = /^\s*(\d+)\s*(?:-\s*(\d+)\s*)?$/.exec(text);
  if (!m) return null;
  const first = Number(m[1]);
  const last = m[2] === undefined ? first : Number(m[2]);
  return last < first ? null : [first, last];
}

/**
 * Why a cell run will not work, or null if it will.
 *
 * Checked here rather than left to the daemon because the failure is silent on
 * hardware: a run that overlaps another quietly loses, and one that asks a
 * seven-segment cell for a letter leaves a blank that looks like a dead panel.
 */
export function cellProblem(
  text: string,
  display: DisplayInfo,
  others: Readout[],
  self: Readout,
): string | null {
  const range = parseCells(text);
  if (!range) return 'Write a cell like "34", or a run like "2-8".';
  const [first, last] = range;
  if (last >= display.cells) {
    return `${display.key} has cells 0 to ${display.cells - 1}.`;
  }
  for (const other of others) {
    if (other === self || other.display !== self.display || other.device !== self.device) continue;
    // Two different seats are the one case where sharing is the point: they
    // cannot both be occupied, so they cannot both be painting, and a window
    // shared between them is the whole reason the seat field exists. Refusing
    // it here would block the arrangement the daemon is built to allow.
    if (self.seat !== undefined && other.seat !== undefined && self.seat !== other.seat) continue;
    const r = parseCells(other.cells);
    if (r && first <= r[1] && r[0] <= last) {
      const taken = other.divider ? "a divider" : other.source || "another field";
      return `${describe(other.cells, display)} is already taken by ${taken}.`;
    }
  }
  return null;
}

/** Shapes present in a run, for the hint under the cell box. */
function shapesIn(range: [number, number], display: DisplayInfo): string {
  const kinds = new Set<string>();
  for (let i = range[0]; i <= range[1] && i < display.shapes.length; i++) {
    const shape = display.shapes[i];
    if (shape) kinds.add(shape);
  }
  const pretty: Record<string, string> = {
    alnum16: "letters and digits",
    digit7: "digits only",
    single: "a single mark",
  };
  return [...kinds].map((k) => pretty[k] ?? k).join(", ");
}

/** The region covering exactly this run, if one does. */
export function regionFor(cells: string, display: DisplayInfo): RegionInfo | undefined {
  return display.regions.find((r) => r.cells === cells);
}

/** How to say a run out loud: its region name if it has one, else the cells. */
export function describe(cells: string, display: DisplayInfo): string {
  const region = regionFor(cells, display);
  if (region) return region.name;
  const range = parseCells(cells);
  if (!range) return cells;
  return range[0] === range[1] ? `cell ${range[0]}` : `cells ${cells}`;
}

/** `"cell 34"` or `"cells 30-33"`, for the parenthetical in a menu row. */
function extent(cells: string): string {
  const range = parseCells(cells);
  if (!range) return cells;
  return range[0] === range[1] ? `cell ${range[0]}` : `cells ${cells}`;
}

function isText(signals: SignalView[], id: string): boolean {
  // A signal the catalogue reports as characters needs no conversion. Anything
  // else is a number, and a number needs to be told what the dial reads.
  return signals.find((x) => x.id === id)?.text ?? false;
}

/** How many characters a text signal will hand over, or 0. */
function textLength(signals: SignalView[], id: string): number {
  return signals.find((x) => x.id === id)?.length ?? 0;
}

/**
 * The highlighting twin DCS-BIOS exports beside a text signal, if it has one.
 *
 * The F-16 is the only module in the catalogue that does this, sending each DED
 * line as `DED_Ln` and its highlighting as `DED_Ln_FORMAT`. The naming is a
 * convention rather than a rule, which is why finding the twin fills the box in
 * and does not replace it.
 */
function twinOf(signals: SignalView[], source: string): string | undefined {
  if (!source) return undefined;
  const id = `${source}_FORMAT`;
  return signals.some((x) => x.id === id && x.text) ? id : undefined;
}

/**
 * Keep the highlighting signal with the source it belongs to.
 *
 * A field repointed from DED line 1 to line 2 wants line 2's twin and never
 * line 1's. The old one would go on marking characters inverse against text it
 * no longer describes, which is a real text signal, so nothing in `validate`
 * would say a word and the glass would simply highlight the wrong characters.
 * A twin the user chose themselves is left where they put it.
 */
function followTwin(span: Span, signals: SignalView[], was: string): void {
  const chosen = span.format !== undefined && span.format !== twinOf(signals, was);
  // A number has no characters to mark, so its highlighting goes whoever chose
  // it. This is the one place that is fair: the source has just been changed by
  // hand, so the field is already being rewritten.
  if (chosen && !isText(signals, span.source ?? "")) {
    delete span.format;
    return;
  }
  if (chosen) return;
  const twin = twinOf(signals, span.source ?? "");
  if (twin) span.format = twin;
  else delete span.format;
}

/** How DCS-BIOS reports the occupied crew station, where it reports one. */
const SEAT_SIGNAL = "SEAT_POSITION";

/**
 * The crew stations this module has, or an empty list.
 *
 * Empty for most modules, and the control is hidden entirely when it is. Only
 * 5 of the 50 catalogued aircraft publish a seat, and offering the choice on
 * the other 45 would be offering a field that can never paint.
 */
function seats(signals: SignalView[]): { value: number; label: string }[] {
  const signal = signals.find((s) => s.id === SEAT_SIGNAL);
  if (!signal) return [];
  if (signal.values.length > 0) {
    // DCS-BIOS labels them, and its labels carry a stray bracket on the last
    // one because the description they are cut from ends in one.
    return signal.values.map((v) => ({ value: v.value, label: v.label.replace(/\)$/, "") }));
  }
  return Array.from({ length: signal.max_value + 1 }, (_, i) => ({
    value: i,
    label: `Seat ${i}`,
  }));
}

/**
 * Which crew station a field paints from.
 *
 * "Any seat" is the default and the right answer for a single-seat aircraft or
 * a reading that does not change between stations. Choosing a station is what
 * lets the same window show the pilot one thing and the gunner another, which
 * is the only case where two fields may share cells.
 */
function seatChooser(
  readout: Readout,
  options: { value: number; label: string }[],
  onChange: () => void,
): HTMLElement {
  const menu = el("select", { class: "seat" });
  menu.append(el("option", { value: "" }, "Any seat"));
  for (const seat of options) {
    menu.append(el("option", { value: String(seat.value) }, seat.label));
  }
  menu.value = readout.seat === undefined ? "" : String(readout.seat);
  menu.addEventListener("change", () => {
    if (menu.value === "") delete readout.seat;
    else readout.seat = Number(menu.value);
    onChange();
  });
  return el(
    "label",
    { class: "meta" },
    "shown in ",
    menu,
    el(
      "span",
      { class: "meta block" },
      "This aircraft reports which station you are in, and DCS-BIOS exports " +
        "both of them at once. Pick one and the field paints only from that " +
        "seat, which is what lets two fields share the same cells.",
    ),
  );
}

/** The sentinel for the "somewhere else" row of the region menu. A cell run
 *  is digits and a dash, so this can never collide with one. */
const CUSTOM = "custom";

/**
 * Where a field goes, chosen by name.
 *
 * The cells are still what gets stored, because that is what the daemon draws
 * and a region is only a label for a run. But nobody deciding what to put on a
 * panel knows what `30-33` is, so the menu leads and the run is the escape
 * hatch, for a field that wants part of a region or a display with no regions
 * mapped yet.
 */
function cellChooser(
  readout: Readout,
  display: DisplayInfo,
  all: Readout[],
  onChange: () => void,
): HTMLElement {
  const menu = el("select", { class: "cells" });
  for (const region of display.regions) {
    menu.append(
      el("option", { value: region.cells }, `${region.name}  (${extent(region.cells)})`),
    );
  }
  menu.append(el("option", { value: CUSTOM }, "Somewhere else..."));

  const box = el("input", { type: "text", class: "cells run", value: readout.cells });
  const note = el("div", { class: "meta" });

  const known = (): boolean => regionFor(readout.cells, display) !== undefined;

  const recheck = (): void => {
    const problem = cellProblem(readout.cells, display, all, readout);
    box.classList.toggle("bad", problem !== null);
    note.classList.toggle("bad", problem !== null);
    if (problem) {
      note.textContent = problem;
      return;
    }
    const range = parseCells(readout.cells);
    const n = range ? range[1] - range[0] + 1 : 0;
    const region = regionFor(readout.cells, display);
    // The region note is the useful half here: the name says where it is, the
    // note says what the aircraft this panel was built for puts there.
    const where = region?.note ? region.note : "";
    const shape = range ? `${n} cell${n === 1 ? "" : "s"}, ${shapesIn(range, display)}` : "";
    note.textContent = where ? `${shape}. ${where}` : shape;
  };

  const showBox = (): void => {
    box.hidden = known();
  };

  menu.value = known() ? readout.cells : CUSTOM;
  showBox();
  recheck();

  menu.addEventListener("change", () => {
    if (menu.value === CUSTOM) {
      // Keep whatever run it already had rather than clearing it, so picking
      // this by accident costs nothing.
      box.hidden = false;
      box.focus();
      recheck();
      return;
    }
    readout.cells = menu.value;
    box.value = menu.value;
    box.hidden = true;
    recheck();
    onChange();
  });

  box.addEventListener("input", () => {
    const problem = cellProblem(box.value.trim(), display, all, readout);
    if (problem) {
      box.classList.add("bad");
      note.textContent = problem;
      note.classList.add("bad");
      return;
    }
    readout.cells = box.value.trim();
    // Typing the cells of a real region selects it, rather than leaving the
    // menu saying "somewhere else" about a place that has a name.
    menu.value = known() ? readout.cells : CUSTOM;
    recheck();
    onChange();
  });

  return el("td", {}, menu, box, note);
}

/**
 * Values the module words differently from the glyph table.
 *
 * The case this exists for is real and was found by flying: DCS-BIOS reports
 * the Hornet scratchpad cursor as `--` where the cockpit shows `_`, and `--`
 * is not a glyph, so the cell goes dark. Nothing can guess that substitution,
 * which is why it is here rather than in the display map.
 */
function aliasEditor(span: Span, onChange: () => void): HTMLElement {
  const wrap = el("div", { class: "aliases" });
  const rows = el("div", { class: "alias-rows" });
  // Held as pairs rather than edited in place on the object, because renaming a
  // key means deleting and re-adding it, and a half-typed name would collide
  // with whatever it passes through on the way.
  const pairs: [string, string][] = Object.entries(span.aliases ?? {});

  const store = (): void => {
    const out: Record<string, string> = {};
    for (const [from, to] of pairs) {
      if (from !== "") out[from] = to;
    }
    if (Object.keys(out).length === 0) delete span.aliases;
    else span.aliases = out;
    onChange();
  };

  const draw = (): void => {
    rows.textContent = "";
    pairs.forEach((pair, i) => {
      const from = el("input", {
        type: "text",
        class: "alias",
        value: pair[0],
        placeholder: "sends",
      });
      const to = el("input", {
        type: "text",
        class: "alias",
        value: pair[1],
        placeholder: "draw",
      });
      from.addEventListener("input", () => {
        pair[0] = from.value;
        store();
      });
      to.addEventListener("input", () => {
        pair[1] = to.value;
        store();
      });
      const drop = el("button", { class: "icon danger", title: "Remove this alias" }, "\u{1F5D1}");
      drop.addEventListener("click", () => {
        pairs.splice(i, 1);
        draw();
        store();
      });
      rows.append(el("div", { class: "alias-row" }, from, el("span", { class: "meta" }, "shows as"), to, drop));
    });
  };

  const add = el("button", { class: "add small" }, "Add a substitution");
  add.addEventListener("click", () => {
    pairs.push(["", ""]);
    draw();
    // Not stored yet: an empty row is not an alias, and writing one would make
    // every click of this button dirty the profile.
  });

  draw();
  wrap.append(
    el("label", { class: "meta" }, "substitutions"),
    rows,
    add,
    el(
      "span",
      { class: "meta block" },
      "For a value this display cannot draw. DCS-BIOS reports the Hornet " +
        "scratchpad cursor as -- where the cockpit shows _, and -- is not a " +
        "glyph, so without a substitution the cell goes dark.",
    ),
  );
  return wrap;
}

/**
 * What a divider shows, in place of a signal picker.
 *
 * There is nothing to choose: it reads no signal, so it has no range, no
 * highlighting and no alignment. What it does have is a width, which is what
 * decides where the dashes fall, so the rule is drawn here as the panel will
 * draw it. The backend works it out; one rule written twice is one rule that
 * can drift.
 */
function dividerCell(opts: RowOptions): { node: HTMLElement; refresh: () => void } {
  const { readout, display, profile, onChange } = opts;
  const preview = el("div", { class: "divider-preview" });
  const paint = (cells: RuleCell[]): void => {
    preview.textContent = "";
    for (const cell of cells) {
      const colour = cell.label ? readout.label_colour ?? readout.colour : readout.colour;
      // Spaces carry the shape here, so they have to survive being drawn in
      // HTML, which collapses a run of them to one.
      const piece = el("span", {}, cell.text.replace(/ /g, "\u00a0"));
      piece.style.color = SWATCH[colour ?? "white"] ?? "";
      preview.append(piece);
    }
  };
  // What the last request was for. The user keeps typing while one is in
  // flight, and a late answer about a shorter label must not be painted over
  // the rule they are looking at.
  let asked = "";
  const refresh = (): void => {
    const range = parseCells(readout.cells);
    const width = range ? range[1] - range[0] + 1 : 0;
    const label = readout.label ?? "";
    const mine = `${width}\u0000${label}`;
    asked = mine;
    void dividerRule(width, label).then(
      (cells) => {
        if (asked === mine) paint(cells);
      },
      () => {
        if (asked === mine) preview.textContent = "";
      },
    );
  };
  refresh();

  const node = el(
    "div",
    { class: "readout-extras" },
    el("span", { class: "meta" }, "A rule. It reads nothing and never changes."),
    preview,
    colourChooser(readout, display, () => {
      refresh();
      onChange();
    }),
    labelEditor(readout, display, profile, () => {
      refresh();
      onChange();
    }),
    el(
      "span",
      { class: "meta block" },
      "A blank cell at each end and an unbroken line between them, so it sits " +
        "clear of whatever is beside it. It is on the glass from the moment " +
        "the aircraft loads, which is what makes it an edge for a page that " +
        "does not fill the screen.",
    ),
    noteEditor(readout, "field", onChange),
  );
  const reset = resetButton(opts);
  if (reset) node.append(reset);
  return { node, refresh };
}

/**
 * What the rule is dividing, set into the middle of it.
 *
 * A rule ends a page and a labelled rule says what the page was, which is what
 * a CDU does with its own. It is not a field: it reads nothing, and it is part
 * of the rule rather than something sharing the cells with it, so it is edited
 * here rather than being a piece of content.
 *
 * The colour is its own. A label drawn in the line's colour reads as part of
 * the line, which is the one thing a label should not do, so "same as the
 * rule" is offered as a choice rather than left as the only behaviour.
 */
function labelEditor(
  readout: Readout,
  display: DisplayInfo,
  profile: Profile,
  onChange: () => void,
): HTMLElement {
  const box = el("input", {
    type: "text",
    class: "span-text",
    value: readout.label ?? "",
    placeholder: "none",
  });

  const menu = el("select", { class: "colour" });
  // Absent rather than a colour of its own, so a label on a rule that is
  // already green does not arrive white until somebody notices.
  menu.append(el("option", { value: "" }, "same as the rule"));
  for (const name of display.colours) menu.append(el("option", { value: name }, name));
  menu.value = readout.label_colour ?? "";
  menu.addEventListener("change", () => {
    if (menu.value) readout.label_colour = menu.value;
    else delete readout.label_colour;
    onChange();
  });

  const trouble = el("div", { class: "meta" });
  const check = (): void => {
    const label = readout.label ?? "";
    menu.disabled = label === "";
    if (label === "") {
      box.classList.remove("bad");
      trouble.classList.remove("bad");
      trouble.textContent = "";
      return;
    }
    // A margin, a dash and a blank each side of it. The backend refuses a
    // label with less than that rather than crowding the line, so saying so
    // here saves the user finding out from the problem list.
    const range = parseCells(readout.cells);
    const cells = range ? range[1] - range[0] + 1 : 0;
    const needs = [...label].length + 6;
    if (cells < needs) {
      box.classList.add("bad");
      trouble.classList.add("bad");
      trouble.textContent =
        `A label here needs ${needs} cells and this rule has ${cells}. It takes ` +
        "a blank each side so it does not read as part of the line, and a dash " +
        "each side so the line is still a line.";
      return;
    }
    const set = alphabet(display, profile, false);
    const missing = set ? [...new Set([...label].filter((c) => !set.includes(c)))] : [];
    box.classList.toggle("bad", missing.length > 0);
    trouble.classList.toggle("bad", missing.length > 0);
    trouble.textContent = missing.length
      ? `The font does not draw ${missing.map((c) => JSON.stringify(c)).join(", ")}, so ` +
        `${missing.length === 1 ? "that cell" : "those cells"} would be blank on the panel.`
      : "";
  };

  box.addEventListener("input", () => {
    // The colour is kept when the text goes, so clearing it to retype does not
    // quietly throw the choice away.
    if (box.value) readout.label = box.value;
    else delete readout.label;
    check();
    onChange();
  });
  check();

  return el(
    "div",
    { class: "rule-label" },
    el("label", { class: "meta" }, "labelled ", box),
    el("label", { class: "meta" }, "in ", menu),
    trouble,
  );
}

/**
 * What colour the glass draws this rule in.
 *
 * Offered on a divider and nowhere else. A field's colour is the aircraft's
 * business, matching what its own CDU draws, and is left as the profile has it;
 * a rule is the user's own addition, so its colour is theirs to pick. The list
 * comes from the backend, so it cannot offer one the panel has no index for.
 */
function colourChooser(
  readout: Readout,
  display: DisplayInfo,
  onChange: () => void,
): HTMLElement {
  const menu = el("select", { class: "colour" });
  for (const name of display.colours) {
    menu.append(el("option", { value: name }, name));
  }
  menu.value = readout.colour ?? "white";
  menu.addEventListener("change", () => {
    readout.colour = menu.value;
    onChange();
  });
  return el(
    "label",
    { class: "meta" },
    "drawn in ",
    menu,
    el(
      "span",
      { class: "meta block" },
      "Match the page it is ruling. Black is the screen's own background, so a " +
        "rule drawn in it is a rule nobody can see.",
    ),
  );
}

interface RowOptions {
  readout: Readout;
  display: DisplayInfo;
  profile: Profile;
  all: Readout[];
  signals: SignalView[];
  /** This field as the shipped default has it, where there is one. */
  shipped?: Readout | undefined;
  onChange: () => void;
  onRemove: () => void;
  /** Swap this field for another and redraw the screen it is on. */
  onReplace: (next: Readout) => void;
}

/**
 * Whether a value is one the backend would write down.
 *
 * It leaves out everything that means the same as saying nothing, so a field
 * carrying `small: false` and a field carrying no `small` at all are the same
 * field. This mirrors the `skip_serializing_if` on each key rather than
 * dropping everything falsy, because seat 0 is a real answer and so is a
 * range that starts at 0.
 */
function meaningful(key: string, value: unknown): boolean {
  if (value === undefined || value === null || value === false || value === "") return false;
  if (key === "decimals" && value === 0) return false;
  if (key === "align" && value === "left") return false;
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === "object") return Object.keys(value as object).length > 0;
  return true;
}

/** The same value with its keys in one order and its silent ones dropped. */
function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value === null || typeof value !== "object") return value;
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(value as object).sort()) {
    const v = (value as Record<string, unknown>)[key];
    if (meaningful(key, v)) out[key] = canonical(v);
  }
  return out;
}

/**
 * A field reduced to what it actually says.
 *
 * Two fields that draw the same thing are not written the same way. One part
 * is stored flat beside the cells and a chain is stored as `content`, and a
 * field touched in this window is a chain from that moment until the backend
 * writes it out again, so comparing the objects would call every field the
 * user had so much as clicked on a changed one. This is what lets the reset
 * button tell a field somebody worked on from one sitting exactly as it was
 * delivered.
 *
 * The cells are left out, being how the field was found in the first place.
 */
function fieldShape(r: Readout): string {
  return JSON.stringify(
    canonical({
      divider: r.divider,
      // Only a rule keeps a colour of its own. On anything else that key holds
      // the one part's colour, and `contentOf` has already taken it there.
      colour: r.divider ? r.colour : undefined,
      seat: r.seat,
      align: r.align,
      note: r.note,
      content: contentOf(r),
    }),
  );
}

/** The shipped field for these cells, where the default has one. */
function shippedFor(readout: Readout, shipped: Readout[]): Readout | undefined {
  return shipped.find(
    (s) =>
      s.device === readout.device && s.display === readout.display && s.cells === readout.cells,
  );
}

/**
 * Put one field back the way it shipped, leaving every other field alone.
 *
 * In the same place on every field the default has a version of, and disabled
 * while it already matches so that it is never a no-op. A profile the user
 * made has no shipped version and no button.
 */
function resetButton(opts: RowOptions): HTMLElement | null {
  const { readout, display } = opts;
  const shipped = opts.shipped;
  if (!shipped) return null;
  const button = el("button", { class: "add revert", type: "button" }, "Reset this field");
  if (fieldShape(shipped) === fieldShape(readout)) {
    button.disabled = true;
    button.title = "This field matches how it shipped.";
    return button;
  }
  button.title = "Put this field back the way it shipped. No other field is touched.";
  button.addEventListener("click", () => {
    // Both sides, the way a lamp's reset shows them. Saying only that colours
    // and sizes go back leaves the decision to be made blind.
    void confirmAction(
      `Reset ${describe(readout.cells, display)} to how it shipped?\n\n` +
        `Now:\n${describeField(readout, display)}\n\n` +
        `Shipped:\n${describeField(shipped, display)}\n\n` +
        "Its colours, sizes and note go back with it. No other field is touched.",
      "Reset",
    ).then((ok) => {
      if (ok) opts.onReplace(structuredClone(shipped));
    });
  });
  return button;
}

/**
 * The widest a piece can ever draw, in cells.
 *
 * The same arithmetic the backend does for the overflow caution, kept here
 * because the preview needs it per keystroke and a round trip for every
 * character typed would make the window feel stuck. The caution itself still
 * comes from the backend, so the number the user is warned with is worked out
 * once, in one place.
 */
function spanWidth(span: Span, signals: SignalView[]): number {
  // A gap takes what is left over, so it never asks for room of its own and
  // can never be the reason content will not fit.
  if (span.gap) return 0;
  if (isLiteral(span)) return (span.text ?? "").length;
  if (!span.source) return 0;
  if (isText(signals, span.source)) return textLength(signals, span.source);
  if (!span.reads) return 0;
  const dp = span.decimals ?? 0;
  return Math.max(...span.reads.map((end) => end.toFixed(dp).length));
}

/** Whether any piece is a gauge with no range, so nothing bounds the width. */
function unbounded(spans: Span[], signals: SignalView[]): boolean {
  return spans.some(
    (s) =>
      !s.gap &&
      !isLiteral(s) &&
      s.source !== "" &&
      !isText(signals, s.source ?? "") &&
      !s.reads,
  );
}

/**
 * The font this display will actually be drawn with, for this profile.
 *
 * The aircraft's own wherever it has one: the glyphs were drawn to match what
 * its module sends, so the profile's choice does not come into it. Null when
 * the aircraft has no font of its own and none has been picked yet, which is
 * when there is no alphabet to check anything against.
 */
export function fontInUse(display: DisplayInfo, profile: Profile): string | null {
  if (!display.text_grid) return null;
  for (const aircraft of profile.aircraft) {
    const native = display.native_fonts[aircraft];
    if (native) return native;
  }
  return profile.font ?? null;
}

/**
 * The font to start on where the profile has not said.
 *
 * The widest alphabet, preferring one that has lowercase. A screen being
 * filled by hand is mostly words, and the fonts are drawn for aircraft rather
 * than for typing: most are uppercase and punctuation the CDU happens to send.
 * Derived rather than named so that a font added later is considered on what
 * it can draw. Today it picks the F-14BU's.
 */
function widestFont(fonts: FontChoice[]): FontChoice {
  const rank = (f: FontChoice): number =>
    f.large.length + ([...f.large].some((c) => c >= "a" && c <= "z") ? 1000 : 0);
  return fonts.reduce((best, f) => (rank(f) > rank(best) ? f : best));
}

/**
 * Settle which font this screen draws with, before anything is drawn.
 *
 * A text grid with no font draws nothing, in the preview and on the panel
 * alike, and nothing can be checked against it either, so "none" is a state
 * worth nobody's time and is not offered. The choice is written onto the
 * profile here rather than waiting for someone to open the menu, so what the
 * preview shows is what the panel will show. This runs while the page is
 * being built, before the baseline is taken, so it does not make a profile
 * arrive unsaved.
 *
 * A font the profile names but this build no longer offers is replaced for
 * the same reason: it draws nothing, and leaving it would show one font in
 * the menu while the profile named another.
 */
function chooseFont(display: DisplayInfo, profile: Profile): void {
  if (!display.text_grid || display.fonts.length === 0) return;
  if (allNative(display, profile)) return;
  if (display.fonts.some((f) => f.file === profile.font)) return;
  profile.font = widestFont(display.fonts).file;
}

/** Whether every aircraft this profile serves brings its own font. */
function allNative(display: DisplayInfo, profile: Profile): boolean {
  return profile.aircraft.every((a) => display.native_fonts[a] !== undefined);
}

/** What the font in use can draw, at the size asked for. */
function alphabet(display: DisplayInfo, profile: Profile, small: boolean): string | null {
  const file = fontInUse(display, profile);
  if (!file) return null;
  const font = display.fonts.find((f) => f.file === file);
  if (!font) return null;
  return small ? font.small : font.large;
}

// Glyph bitmaps, kept for as long as the window is up. A font never changes
// under us, and refetching one on every keystroke would make typing crawl.
const GLYPHS = new Map<string, Promise<FontGlyphs>>();

function glyphsFor(display: string, font: string): Promise<FontGlyphs> {
  const key = `${display}\u0000${font}`;
  let held = GLYPHS.get(key);
  if (!held) {
    held = fontGlyphs(display, font);
    GLYPHS.set(key, held);
  }
  return held;
}

/**
 * The line as the panel will draw it, in the font it will draw it with.
 *
 * Checking the typed characters against the alphabet is not enough, because
 * these fonts reuse slots: in the A-10C font `%` draws a question mark. A
 * preview made of the typed string would agree with the user and disagree with
 * the glass, which is the one thing it is here to stop.
 *
 * A reading is drawn as a dim block per cell rather than as sample characters.
 * Nothing here knows what the aircraft will send, and inventing a number would
 * be the same lie in a different place.
 */
function glyphPreview(
  readout: Readout,
  display: DisplayInfo,
  profile: Profile,
  signals: SignalView[],
): { node: HTMLElement; refresh: () => void } {
  const canvas = el("canvas", { class: "glyph-preview" });
  const note = el("div", { class: "meta" });

  const refresh = (): void => {
    const range = parseCells(readout.cells);
    const width = range ? range[1] - range[0] + 1 : 0;
    const file = fontInUse(display, profile);
    if (!display.text_grid || !file || width === 0) {
      canvas.hidden = true;
      note.textContent =
        display.text_grid && !file ? "No font ships for this screen, so it cannot be drawn here." : "";
      return;
    }
    canvas.hidden = false;
    note.textContent = "";
    void glyphsFor(display.key, file).then(
      (font) => {
        // Which piece each cell comes from, so a cell can be drawn in that
        // piece's colour and size, or left as a block where a reading goes.
        // Built per piece rather than flat, because a gap cannot be measured
        // until everything that is not a gap has been laid out, the same way
        // the daemon does it.
        const spans = contentOf(readout);
        const groups: { span: Span; ch: string | null }[][] = [];
        const gaps: number[] = [];
        for (const span of spans) {
          if (span.gap) {
            gaps.push(groups.length);
            groups.push([]);
            continue;
          }
          const group: { span: Span; ch: string | null }[] = [];
          if (isLiteral(span)) {
            for (const ch of span.text ?? "") group.push({ span, ch });
          } else {
            for (let i = 0; i < spanWidth(span, signals); i += 1) group.push({ span, ch: null });
          }
          groups.push(group);
        }
        if (gaps.length > 0) {
          const fixed = groups.reduce((n, g) => n + g.length, 0);
          const spare = Math.max(0, width - fixed);
          const each = Math.floor(spare / gaps.length);
          const extra = spare % gaps.length;
          gaps.forEach((at, n) => {
            const take = each + (n < extra ? 1 : 0);
            groups[at] = Array.from({ length: take }, () => ({ span: spans[at] as Span, ch: " " }));
          });
        }
        const cells = groups.flat();
        // Cropped and padded the way the field will be, so the preview shows
        // the loss rather than a line that fits in the window and not on the
        // panel.
        const shown =
          readout.align === "right"
            ? cells.slice(Math.max(0, cells.length - width))
            : cells.slice(0, width);

        const scale = 0.6;
        const cw = Math.round(font.width * scale);
        const chh = Math.round(font.height * scale);
        canvas.width = cw * width;
        canvas.height = chh;
        canvas.style.width = `${cw * width}px`;
        canvas.style.height = `${chh}px`;
        const ctx = canvas.getContext("2d");
        if (!ctx) return;
        ctx.fillStyle = "#05070a";
        ctx.fillRect(0, 0, canvas.width, canvas.height);

        const offset = readout.align === "right" ? width - shown.length : 0;
        shown.forEach((cell, i) => {
          const x = (offset + i) * cw;
          const colour = SWATCH[cell.span.colour ?? "white"] ?? "#f2f4f7";
          if (cell.ch === null) {
            // Where a reading will go. Drawn as a bar rather than as digits,
            // because nothing here knows what the aircraft will send.
            ctx.fillStyle = "#2a3340";
            ctx.fillRect(x + 1, chh * 0.3, cw - 2, chh * 0.4);
            return;
          }
          const table = cell.span.small ? font.small : font.large;
          const rows = table[cell.ch];
          if (!rows) {
            // A character the font has no glyph for draws nothing at all on
            // the panel, so it draws nothing here either, marked so the blank
            // is visibly a missing glyph rather than a space.
            ctx.strokeStyle = "#7a2b2b";
            ctx.strokeRect(x + 1.5, 1.5, cw - 3, chh - 3);
            return;
          }
          ctx.fillStyle = cell.span.inverse ? "#05070a" : colour;
          if (cell.span.inverse) {
            ctx.save();
            ctx.fillStyle = colour;
            ctx.fillRect(x, 0, cw, chh);
            ctx.restore();
            ctx.fillStyle = "#05070a";
          }
          rows.forEach((row, ry) => {
            [...row].forEach((bit, rx) => {
              if (bit === "." || bit === " ") return;
              ctx.fillRect(x + rx * scale, ry * scale, scale + 0.5, scale + 0.5);
            });
          });
        });
      },
      () => {
        canvas.hidden = true;
        note.textContent = "The font could not be read.";
      },
    );
  };

  return { node: el("div", { class: "preview-wrap" }, canvas, note), refresh };
}

/**
 * One piece of a field: what it draws, and how.
 *
 * Text and a signal are the two answers to the same question, so they share a
 * row and a chooser rather than being two kinds of thing to add.
 */
function spanEditor(
  spans: Span[],
  index: number,
  opts: RowOptions,
  redraw: () => void,
  refresh: () => void,
): HTMLElement {
  const { readout, display, profile, signals, onChange } = opts;
  // The array the chain is drawing, not another copy of it. A field still in
  // the flat shape gives a fresh array and fresh pieces every time `contentOf`
  // is called, so deriving one here handed this a piece that was not the one
  // in the chain, and every edit had to be written as a replacement.
  const span = spans[index] as Span;
  const wrap = el("div", { class: "span" });
  // What a keystroke has to do: put the change on the profile, tell whoever is
  // watching, and update the parts that read the content. Not rebuild the
  // chain, which would throw away the box being typed into.
  const edited = (): void => {
    setContent(readout, spans);
    refresh();
    onChange();
  };

  const kind = el("select", { class: "span-kind" });
  kind.append(el("option", { value: "signal" }, "a reading"));
  kind.append(el("option", { value: "text" }, "text"));
  kind.append(el("option", { value: "gap" }, "a gap"));
  kind.value = kindOf(span);
  kind.addEventListener("change", () => {
    // Everything on a piece describes the one value it draws, so switching
    // what it draws leaves none of it meaningful. Colour and size are the
    // exception: they are about how it looks, and the user picked them. A gap
    // keeps none of it, because it draws nothing to style.
    const next = kind.value as SpanKind;
    if (next === "gap") {
      spans[index] = newSpan("gap");
    } else {
      const kept: Span = { colour: span.colour, small: span.small, inverse: span.inverse };
      spans[index] = next === "text" ? { ...kept, text: "" } : { ...kept, source: "" };
    }
    setContent(readout, spans);
    redraw();
    onChange();
  });

  const body = el("div", { class: "span-body" });

  if (span.gap) {
    body.append(
      el(
        "span",
        { class: "meta block" },
        "Blank, and as wide as whatever the rest of the row leaves. Put one " +
          "between two pieces to push them to opposite ends, or use two to " +
          "space three pieces evenly. It draws nothing itself, so it has " +
          "nothing to colour.",
      ),
    );
  } else if (isLiteral(span)) {
    const set = alphabet(display, profile, span.small ?? false);
    const box = el("input", {
      type: "text",
      class: "span-text",
      value: span.text ?? "",
      placeholder: "RALT",
    });
    const trouble = el("div", { class: "meta" });
    const check = (): void => {
      if (!set) {
        trouble.textContent = display.text_grid
          ? "No font picked yet, so nothing can be checked."
          : "";
        trouble.classList.toggle("bad", false);
        return;
      }
      const missing = [...new Set([...(span.text ?? "")].filter((c) => !set.includes(c)))];
      box.classList.toggle("bad", missing.length > 0);
      trouble.classList.toggle("bad", missing.length > 0);
      trouble.textContent = missing.length
        ? `The font does not draw ${missing.map((c) => JSON.stringify(c)).join(", ")}, so ` +
          `${missing.length === 1 ? "that cell" : "those cells"} would be blank on the panel.`
        : "";
    };
    box.addEventListener("input", () => {
      span.text = box.value;
      check();
      edited();
    });
    check();
    body.append(box, trouble);
  } else {
    const picker = signalPicker({
      signals,
      value: span.source ?? "",
      onPick: (id) => {
        const was = span.source ?? "";
        span.source = id;
        followTwin(span, signals, was);
        // A different signal brings a different set of controls with it: a
        // range where the old one was a number, none where it reports
        // characters, a highlighting twin or not. So this one rebuilds.
        setContent(readout, spans);
        redraw();
        onChange();
      },
    });
    body.append(picker);

    if (span.source) {
      if (isText(signals, span.source)) {
        // A signal that already reports characters needs no range, and
        // offering one would invite a conversion that means nothing.
        delete span.reads;
        delete span.decimals;
      } else {
        const low = el("input", {
          type: "number",
          class: "num small",
          value: String(span.reads?.[0] ?? 0),
        });
        const high = el("input", {
          type: "number",
          class: "num small",
          value: String(span.reads?.[1] ?? 100),
        });
        const dp = el("input", {
          type: "number",
          class: "num small",
          min: "0",
          max: "3",
          value: String(span.decimals ?? 0),
        });
        const sync = (): void => {
          span.reads = [Number(low.value), Number(high.value)];
          span.decimals = Number(dp.value) || 0;
          edited();
        };
        for (const box of [low, high, dp]) box.addEventListener("input", sync);
        if (!span.reads) sync();
        body.append(
          el(
            "label",
            { class: "meta" },
            "reads ",
            low,
            " to ",
            high,
            el(
              "span",
              { class: "meta block" },
              "What the dial is marked with in the cockpit. DCS-BIOS reports a " +
                "needle as a position, not a value, so this is yours to give. " +
                "A face that starts below zero or runs backwards is fine.",
            ),
          ),
          el("label", { class: "meta" }, "decimals ", dp),
        );
      }
      // A substitution is about what this signal sends, so it belongs to the
      // piece that reads it rather than to the field around it.
      body.append(aliasEditor(span, edited));
    }
  }

  // --- how it looks --------------------------------------------------------
  const style = el("div", { class: "span-style" });

  if (display.text_grid && !span.gap) {
    const colour = el("select", { class: "colour" });
    for (const name of display.colours) colour.append(el("option", { value: name }, name));
    colour.value = span.colour ?? "white";
    colour.addEventListener("change", () => {
      span.colour = colour.value;
      edited();
    });
    style.append(el("label", { class: "meta" }, "in ", colour));

    const small = el("input", { type: "checkbox" });
    small.checked = span.small === true;
    small.addEventListener("change", () => {
      if (small.checked) span.small = true;
      else delete span.small;
      // The small font draws fewer characters than the large one, so what the
      // typed text is checked against has changed. That check is built with
      // the box, so this one really does rebuild.
      setContent(readout, spans);
      redraw();
      onChange();
    });
    style.append(
      el(
        "label",
        { class: "meta" },
        small,
        " small",
        el(
          "span",
          { class: "meta block" },
          "The grid's small font, which a CDU uses for its labels. It draws " +
            "fewer characters than the large one, so a character that was fine " +
            "may stop being drawn.",
        ),
      ),
    );
  }

  if (display.draws_inverse && isLiteral(span)) {
    const flip = el("input", { type: "checkbox" });
    flip.checked = span.inverse === true;
    flip.addEventListener("change", () => {
      if (flip.checked) span.inverse = true;
      else delete span.inverse;
      edited();
    });
    style.append(el("label", { class: "meta" }, flip, " inverse"));
  }

  // A highlighting signal marks characters, so it means nothing over a number,
  // and nothing at all on glass with no inverse form to draw.
  if (
    display.draws_inverse &&
    !isLiteral(span) &&
    span.source &&
    (isText(signals, span.source) || span.format !== undefined)
  ) {
    style.append(
      spanFormatChooser(span, signals, edited),
    );
  }

  const up = el("button", { class: "icon", title: "Move this piece earlier" }, "↑");
  up.disabled = index === 0;
  up.addEventListener("click", () => {
    const moved = spans.splice(index, 1)[0];
    if (!moved) return;
    spans.splice(index - 1, 0, moved);
    setContent(readout, spans);
    redraw();
    onChange();
  });
  const down = el("button", { class: "icon", title: "Move this piece later" }, "↓");
  down.disabled = index === spans.length - 1;
  down.addEventListener("click", () => {
    const moved = spans.splice(index, 1)[0];
    if (!moved) return;
    spans.splice(index + 1, 0, moved);
    setContent(readout, spans);
    redraw();
    onChange();
  });
  const drop = el("button", { class: "icon danger", title: "Remove this piece" }, "×");
  drop.disabled = spans.length === 1;
  drop.addEventListener("click", () => {
    spans.splice(index, 1);
    setContent(readout, spans);
    redraw();
    onChange();
  });

  wrap.append(
    el("div", { class: "span-head" }, kind, el("span", { class: "spacer" }), up, down, drop),
    body,
    style,
  );
  return wrap;
}

/**
 * The highlighting signal for one piece.
 *
 * The same thing `formatChooser` does for a whole field, against a piece. Kept
 * separate rather than made general, because the field-wide one is what a
 * profile written before chains still uses and the two say different things
 * about what they cover.
 */
function spanFormatChooser(
  span: Span,
  signals: SignalView[],
  onChange: () => void,
): HTMLElement {
  const twin = span.source ? twinOf(signals, span.source) : undefined;
  const box = el("input", { type: "checkbox" });
  box.checked = span.format !== undefined;
  box.disabled = twin === undefined && span.format === undefined;
  box.addEventListener("change", () => {
    if (box.checked && twin) span.format = twin;
    else delete span.format;
    onChange();
  });
  return el(
    "label",
    { class: "meta" },
    box,
    ` highlighting${twin ? ` from ${twin}` : ""}`,
    el(
      "span",
      { class: "meta block" },
      "A second signal the module sends beside this one, one character for " +
        "one, marking which characters to draw inverse.",
    ),
  );
}

/** Every piece of a field, in the order they are drawn. */
function chainEditor(opts: RowOptions, refreshPreview: () => void): HTMLElement {
  const { readout, display, profile, signals, onChange } = opts;
  const wrap = el("div", { class: "chain" });

  // What an edit inside a piece updates, as against `redraw`, which builds the
  // chain again. Rebuilding on a keystroke threw away the box being typed into
  // and put a new one in its place, which is what losing focus after every
  // character was. Replaced on each redraw, since it closes over that pass's
  // elements.
  let refresh = (): void => {};

  const redraw = (): void => {
    wrap.textContent = "";
    const spans = contentOf(readout);
    spans.forEach((_, i) =>
      wrap.append(spanEditor(spans, i, opts, redraw, () => refresh())),
    );

    // --- adding ------------------------------------------------------------
    const add = el("div", { class: "chain-add" });
    const addOne = (kind: SpanKind, label: string): HTMLElement => {
      const button = el("button", { class: "add" }, label);
      button.addEventListener("click", () => {
        const next = contentOf(readout);
        next.push(newSpan(kind));
        setContent(readout, next);
        redraw();
        onChange();
      });
      return button;
    };
    add.append(
      addOne("signal", "+ a reading"),
      addOne("text", "+ text"),
      addOne("gap", "+ a gap"),
    );
    wrap.append(add);

    // --- how wide it comes out ---------------------------------------------
    // Read again on each refresh rather than once here: every character typed
    // changes it, and it is the line that says the field will not fit.
    const range = parseCells(readout.cells);
    const cells = range ? range[1] - range[0] + 1 : 0;
    const hasGap = spans.some((s) => s.gap);
    const fit = el("div", { class: "meta" });
    const drawFit = (): void => {
      const widest = spans.reduce((n, s) => n + spanWidth(s, signals), 0);
      const loose = unbounded(spans, signals);
      fit.classList.remove("bad");
      fit.textContent = "";
      if (cells === 1) {
        fit.textContent =
          "A single cell takes the whole value as one glyph, which is how a " +
          "two-character field is drawn on this hardware.";
      } else if (widest > cells) {
        const lost = widest - cells;
        const end = readout.align === "right" ? "first" : "last";
        fit.classList.add("bad");
        fit.textContent =
          `This needs up to ${widest} cells and has ${cells}. The ${end} ` +
          `${lost} character${lost === 1 ? "" : "s"} would be dropped, and ` +
          "nothing on the panel would say so.";
      } else if (loose) {
        fit.textContent =
          `A gauge with no range has no known width, so this may run past its ${cells} cells.`;
      } else if (hasGap) {
        const spare = cells - widest;
        fit.textContent =
          `Up to ${widest} of ${cells} cells, and the ${spare} left over go to ` +
          `the gap${spans.filter((s) => s.gap).length === 1 ? "" : "s"}. ` +
          "A reading that grows takes the room back from there, so the ends stay put.";
      } else if (widest > 0) {
        fit.textContent = `Up to ${widest} of ${cells} cells.`;
      }
    };
    wrap.append(fit);

    // --- the rest of the field ---------------------------------------------
    const extras = el("div", { class: "readout-extras" });
    if (range && range[1] > range[0] && !hasGap) {
      const right = el("input", { type: "checkbox" });
      right.checked = readout.align === "right";
      right.addEventListener("change", () => {
        if (right.checked) readout.align = "right";
        else delete readout.align;
        redraw();
        onChange();
      });
      extras.append(
        el(
          "label",
          { class: "meta" },
          right,
          " right aligned",
          el(
            "span",
            { class: "meta block" },
            "Anchor the content to the last cell. A scratchpad wants this: " +
              "digits enter at the right, and DCS-BIOS can send more " +
              "characters than there are cells.",
          ),
        ),
      );
    }
    const stations = seats(signals);
    if (stations.length > 1) extras.append(seatChooser(readout, stations, onChange));
    extras.append(noteEditor(readout, "field", onChange));
    const reset = resetButton(opts);
    if (reset) extras.append(reset);
    wrap.append(extras);

    const preview = glyphPreview(readout, display, profile, signals);
    wrap.append(preview.node);
    // Everything a keystroke moves: what the field will draw, how wide it comes
    // out, and the row's own one-line preview further up the page.
    refresh = (): void => {
      drawFit();
      preview.refresh();
      refreshPreview();
    };
    refresh();
  };

  redraw();
  return wrap;
}

function row(opts: RowOptions): HTMLTableRowElement {
  const { readout, display, all, onChange } = opts;
  const tr = el("tr");

  // A divider's preview follows its width, so a change of cells has to reach
  // it. Everything else in a row reads the cells only when it is drawn.
  const rule = readout.divider ? dividerCell(opts) : null;
  let rebuild = (): void => {};
  const changed = (): void => {
    rule?.refresh();
    rebuild();
    onChange();
  };
  tr.append(cellChooser(readout, display, all, changed));

  if (rule) {
    tr.append(el("td", {}, rule.node));
    tr.append(el("td", { class: "num" }, removeButton(opts)));
    return tr;
  }

  const shows = el("td");
  const draw = (): void => {
    shows.textContent = "";
    shows.append(flagSlot(readout), chainEditor(opts, () => {}));
  };
  rebuild = draw;
  draw();
  tr.append(shows);
  tr.append(el("td", { class: "num" }, removeButton(opts)));
  return tr;
}

/**
 * What a field draws, said out loud, for a question about deleting it.
 *
 * The pieces in the order they are drawn, because a chain is not recognisable
 * from its cells alone: two rows of a CDU look identical in the list and are
 * not the same field.
 */
function describeField(readout: Readout, display: DisplayInfo): string {
  const where = describe(readout.cells, display);
  if (readout.divider) {
    return readout.label ? `The rule on ${where}, labelled ${readout.label}.` : `The rule on ${where}.`;
  }
  const pieces = contentOf(readout).map((s) => {
    if (s.gap) return "a gap";
    if (kindOf(s) === "signal") return s.source ? s.source : "a reading nobody has chosen yet";
    return s.text ? JSON.stringify(s.text) : "an empty piece of text";
  });
  return pieces.length > 0 ? `${where}: ${pieces.join(", then ")}.` : `${where}.`;
}

/**
 * Deleting a field, asked about first.
 *
 * The same red button and the same question a lamp's condition gets, for the
 * same reason: it is a small destructive control beside harmless ones, and a
 * misclick was silent and, once the profile was saved, gone. It used to arm on
 * the first click and delete on the second, which says nothing about what is
 * about to go and, since nothing styled the armed state, did not even look
 * different.
 */
function removeButton(opts: RowOptions): HTMLElement {
  const { readout, display } = opts;
  const what = readout.divider ? "rule" : "field";
  return iconButton("trash", "\u{1F5D1}", `Delete this ${what}`, () => {
    const consequence = opts.shipped
      ? "\n\nThis one shipped with the profile, so the area it sits in will offer it back."
      : "\n\nThose cells go blank, and nothing here will say a field was ever on them.";
    void confirmAction(
      `Delete this ${what}?\n\n${describeField(readout, display)}${consequence}`,
      "Delete",
    ).then((ok) => {
      if (ok) opts.onRemove();
    });
  });
}

/**
 * The colour every field on this display already draws in, if they agree.
 *
 * A new divider takes it, because a white rule across a green page reads as a
 * fault rather than a divider. Undefined where they disagree or there are
 * none, which leaves the rule on the display's own default.
 */
function agreedColour(existing: Readout[]): string | undefined {
  const used = new Set(
    existing.flatMap((r) => contentOf(r).map((s) => s.colour)).filter((c) => c !== undefined),
  );
  return used.size === 1 ? [...used][0] : undefined;
}

/** The cells of a run, as a pair, or null. */
function bounds(cells: string): [number, number] | null {
  return parseCells(cells);
}

/** Which region a field belongs to: the one holding its first cell. */
function regionOf(readout: Readout, display: DisplayInfo): RegionInfo | undefined {
  const b = bounds(readout.cells);
  if (!b) return undefined;
  return display.regions.find((region) => {
    const r = bounds(region.cells);
    return r !== null && b[0] >= r[0] && b[0] <= r[1];
  });
}

/**
 * The display section for one device, or null if it has no glass.
 *
 * `shipped` is every field of the shipped default, which is what the reset
 * buttons put back and what says a deleted field can be brought back at all.
 * Empty for a profile the user made, which has nothing to go back to.
 */
export function displaySection(
  device: Device,
  profile: Profile,
  signals: SignalView[],
  onChange: () => void,
  shipped: Readout[],
): HTMLElement | null {
  if (device.displays.length === 0) return null;
  if (!profile.readouts) profile.readouts = [];
  const readouts = profile.readouts;
  const wrap = el("div", { class: "displays" });

  for (const display of device.displays) {
    chooseFont(display, profile);
    const mine = (): Readout[] =>
      readouts.filter((r) => r.device === device.key && r.display === display.key);

    const body = el("tbody");
    const count = el("span", { class: "meta" });
    let redraw = (): void => {};

    const fieldRow = (r: Readout): HTMLTableRowElement =>
      row({
        readout: r,
        display,
        profile,
        all: readouts,
        signals,
        shipped: shippedFor(r, shipped),
        onChange,
        onRemove: () => {
          readouts.splice(readouts.indexOf(r), 1);
          redraw();
          onChange();
        },
        onReplace: (next) => {
          const at = readouts.indexOf(r);
          if (at < 0) return;
          readouts[at] = next;
          redraw();
          onChange();
        },
      });

    /** An empty region: what could go here, and the ways to put it there. */
    const emptyRow = (region: RegionInfo): HTMLTableRowElement => {
      const tr = el("tr", { class: "region-empty" });
      tr.append(
        el(
          "td",
          {},
          el("span", { class: "region-name" }, region.name),
          el("div", { class: "meta" }, extent(region.cells)),
        ),
      );
      const add = (kind: "signal" | "text" | "rule", label: string): HTMLElement => {
        const button = el("button", { class: "add" }, label);
        button.addEventListener("click", () => {
          const fresh: Readout = {
            device: device.key,
            display: display.key,
            cells: region.cells,
            source: "",
          };
          if (kind === "rule") {
            fresh.divider = true;
            fresh.colour = agreedColour(mine());
          } else {
            setContent(fresh, [newSpan(kind)]);
          }
          readouts.push(fresh);
          redraw();
          onChange();
        });
        return button;
      };
      const buttons = el("div", { class: "chain-add" }, add("signal", "+ a reading"), add("text", "+ text"));
      // Only a text grid draws a rule. A segment display draws from a glyph
      // table with no dash in it, and the daemon refuses one there.
      if (display.text_grid) buttons.append(add("rule", "+ a rule"));
      // A field the default put here and the user threw away. Without this
      // there is nothing on the screen to say one was ever here, and the way
      // back is resetting the whole profile.
      for (const was of shipped) {
        if (was.device !== device.key || was.display !== display.key) continue;
        if (regionOf(was, display) !== region) continue;
        const back = el("button", { class: "add revert" }, "+ the field that shipped here");
        back.title = "Put back the field this area shipped with. Nothing else is touched.";
        back.addEventListener("click", () => {
          readouts.push(structuredClone(was));
          redraw();
          onChange();
        });
        buttons.append(back);
      }
      tr.append(el("td", {}, el("span", { class: "meta" }, region.note || "Nothing here yet."), buttons));
      tr.append(el("td", { class: "num" }));
      return tr;
    };

    redraw = (): void => {
      body.textContent = "";
      const list = mine();
      const placed = new Set<Readout>();
      let used = 0;

      for (const region of display.regions) {
        const here = list
          .filter((r) => regionOf(r, display) === region)
          .sort((a, b) => (bounds(a.cells)?.[0] ?? 0) - (bounds(b.cells)?.[0] ?? 0));
        if (here.length === 0) {
          body.append(emptyRow(region));
          continue;
        }
        used += 1;
        // Nothing offers to add a second field beside the first any more. Two
        // readings on one line is what a chain of pieces is for, and it can
        // count the cells; a second field could not, and every one it added
        // landed on cells the row already held.
        for (const r of here) {
          placed.add(r);
          body.append(fieldRow(r));
        }
      }

      // A field whose cells sit outside every named region still has to be
      // shown, or it would be invisible here and alive on the panel.
      const loose = list.filter((r) => !placed.has(r));
      if (loose.length > 0) {
        const head = el("tr", { class: "region-head" });
        head.append(
          el("td", { colspan: "3", class: "meta" }, "Outside the named areas of this screen"),
        );
        body.append(head);
        for (const r of loose) body.append(fieldRow(r));
      }

      const total = display.regions.length;
      count.textContent = `${used} of ${total} area${total === 1 ? "" : "s"} in use`;
    };
    redraw();

    const head = el(
      "div",
      { class: "display-head" },
      el("span", { class: "name" }, `${display.key} display`),
      el("span", { class: "meta" }, `${display.cells} cells`),
      count,
    );
    const picker = fontPicker(display, profile, () => {
      redraw();
      onChange();
    });
    if (picker) head.append(picker);

    wrap.append(
      el(
        "div",
        { class: "display" },
        head,
        el(
          "table",
          { class: "readouts" },
          el(
            "thead",
            {},
            el("tr", {}, el("th", {}, "Where"), el("th", {}, "Shows"), el("th", { class: "num" }, "")),
          ),
          body,
        ),
      ),
    );
  }
  return wrap;
}

/**
 * Which font to upload to this screen.
 *
 * Offered only where the aircraft has none of its own. A module that draws a
 * CDU has glyphs drawn to match what it sends, so the choice would only be a
 * way to put the wrong symbol on the glass. Where every aircraft brings one,
 * this says which rather than asking.
 *
 * Which font it starts on, and why there is no empty choice, is in
 * `chooseFont`, which has settled it by the time this runs.
 */
function fontPicker(
  display: DisplayInfo,
  profile: Profile,
  onChange: () => void,
): HTMLElement | null {
  if (!display.text_grid || display.fonts.length === 0) return null;
  if (allNative(display, profile)) {
    const file = fontInUse(display, profile);
    const font = display.fonts.find((f) => f.file === file);
    return el(
      "span",
      { class: "meta font-fixed" },
      `font: ${font?.name ?? "the aircraft's"}, from the aircraft`,
    );
  }

  const menu = el("select", { class: "font" });
  for (const font of display.fonts) {
    menu.append(el("option", { value: font.file }, font.name));
  }
  menu.value = profile.font ?? "";
  menu.addEventListener("change", () => {
    profile.font = menu.value;
    onChange();
  });

  const about = el("span", { class: "meta block" });
  const describeFont = (): void => {
    const font = display.fonts.find((f) => f.file === profile.font);
    // `chooseFont` has already put one of these on the profile, so this is
    // only here to keep the types honest.
    if (!font) {
      about.textContent = "";
      return;
    }
    const lower = [...font.large].some((c) => c >= "a" && c <= "z");
    about.textContent =
      `${font.large.length} characters, ${font.small.length} of them in the ` +
      `small font. ${lower ? "Includes lowercase." : "Uppercase only."}`;
  };
  describeFont();
  menu.addEventListener("change", describeFont);

  return el("label", { class: "meta font-pick" }, "font ", menu, about);
}
