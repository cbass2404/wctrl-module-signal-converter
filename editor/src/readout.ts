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
import { cautionSlot, flagSlot } from "./flags";
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
  // else is a number, which can be shown as sent or converted.
  return signals.find((x) => x.id === id)?.text ?? false;
}

/** How many characters a text signal will hand over, or 0. */
function textLength(signals: SignalView[], id: string): number {
  return signals.find((x) => x.id === id)?.length ?? 0;
}

/** The largest number a signal reports. The backend fills in 65535 where DCS-BIOS gives none. */
function maxOf(signals: SignalView[], id: string): number {
  return signals.find((x) => x.id === id)?.max_value ?? 65535;
}

/** How a reading draws a number: as DCS-BIOS sends it, converted, or named. */
type ReadingKind = "sent" | "converted" | "aliased";

const READING_LABELS: Record<ReadingKind, string> = {
  sent: "as sent",
  converted: "converted to",
  aliased: "as aliases",
};

/**
 * The catalogue's name for each position of a switch, as a starting set of
 * aliases. Empty for a signal whose positions have no names, which is most
 * counts: their labels are only the numbers again.
 */
function namedPositions(signal: SignalView | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  if (!signal || signal.text) return out;
  if (!signal.values.some((v) => v.label !== String(v.value))) return out;
  for (const v of signal.values) out[String(v.value)] = v.label;
  return out;
}

/**
 * A sensible way to show a number the user has just chosen, so that picking
 * the signal is usually the only step, the way a lamp's test is filled in.
 * It is only where the choice starts: every signal can be switched to any of
 * the three.
 *
 * A switch whose positions have names arrives aliased to them, ready to be
 * shortened. A full word is a needle's position rather than a quantity, and 0
 * to 65535 means nothing on a screen, so it arrives converted, to a range the
 * user then sets from the dial. Anything else is a count or a selector whose
 * value already is the number, and is shown as sent.
 */
function startReading(span: Span, signal: SignalView | undefined): void {
  delete span.reads;
  delete span.decimals;
  delete span.value_aliases;
  if (!signal || signal.text) return;
  const named = namedPositions(signal);
  if (Object.keys(named).length > 0) span.value_aliases = named;
  else if (signal.max_value >= 65535) span.reads = [0, 100];
}

/**
 * How a number is drawn, laid out the way a lamp's test is: what to do with
 * it, then the numbers that go with that.
 *
 * As sent draws the value DCS-BIOS reports. Converted spreads the signal's
 * whole range, 0 to its maximum, across what the dial is marked with, which
 * is the linear conversion the daemon does. Whether `reads` is present is
 * what says which, so a field written by hand reads back as whichever it is.
 */
function conversionRow(
  span: Span,
  signal: SignalView | undefined,
  set: string | null,
  edited: () => void,
  rebuild: () => void,
): HTMLElement {
  const max = signal?.max_value ?? 65535;
  const select = el("select", { class: "test" });
  for (const kind of Object.keys(READING_LABELS) as ReadingKind[]) {
    select.append(el("option", { value: kind }, READING_LABELS[kind]));
  }
  select.value = span.value_aliases ? "aliased" : span.reads ? "converted" : "sent";
  select.addEventListener("change", () => {
    // Converting starts from the signal's own range, which draws exactly what
    // as sent did, so the choice changes nothing until a number is changed.
    // Aliasing starts from the catalogue's position names, where it has any.
    // Decimals mean nothing on a whole number sent as it is.
    delete span.reads;
    delete span.decimals;
    delete span.value_aliases;
    if (select.value === "converted") span.reads = [0, max];
    if (select.value === "aliased") span.value_aliases = namedPositions(signal);
    rebuild();
  });

  if (span.value_aliases) {
    return el(
      "div",
      {},
      el("div", { class: "test-row" }, select),
      valueAliasEditor(span, set, edited),
      el(
        "span",
        { class: "meta block" },
        `What to draw for each value DCS-BIOS sends, 0 to ${max}. Shorten ` +
          "them to fit the cells you have. A value with no alias is drawn as " +
          "the number.",
      ),
    );
  }

  const values = el("span", { class: "values-row" });
  if (span.reads) {
    const number = (value: number, attrs: Record<string, string> = {}): HTMLInputElement =>
      el("input", { type: "number", class: "value", value: String(value), ...attrs });
    const low = number(span.reads[0]);
    const high = number(span.reads[1]);
    const dp = number(span.decimals ?? 0, { min: "0", max: "3" });
    const sync = (): void => {
      span.reads = [Number(low.value) || 0, Number(high.value) || 0];
      const places = Number(dp.value) || 0;
      if (places > 0) span.decimals = places;
      else delete span.decimals;
      edited();
    };
    for (const box of [low, high, dp]) box.addEventListener("input", sync);
    values.append(
      low,
      el("span", { class: "sep" }, "to"),
      high,
      el("span", { class: "sep" }, "with"),
      dp,
      el("span", { class: "sep" }, "decimals"),
    );
  }

  return el(
    "div",
    {},
    el("div", { class: "test-row" }, select, values),
    el(
      "span",
      { class: "meta block" },
      span.reads
        ? `DCS-BIOS sends 0 to ${max}, and this is what the dial is marked ` +
            "with at each end. It reports a needle as a position, not a value, " +
            "so this is yours to give. A face that starts below zero or runs " +
            "backwards is fine."
        : `The number DCS-BIOS sends, 0 to ${max}, drawn as it is. Right for ` +
            "a count or a selector. A needle wants converting.",
    ),
  );
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
 * What a number draws at each value, one row a value.
 *
 * Laid out like the substitutions, which are the same shape of thing. Arrives
 * filled with the catalogue's position names where there are any, which are
 * often longer than the cells a user has to spare, so every one is theirs to
 * shorten, clear or remove. A character the font lacks is said beside the
 * row as it is typed, the way it is for typed text.
 */
function valueAliasEditor(span: Span, set: string | null, onChange: () => void): HTMLElement {
  const wrap = el("div", { class: "aliases" });
  const rows = el("div", { class: "alias-rows" });
  // Pairs rather than the object, for the same reason as the substitutions:
  // changing a value half way through typing it would collide with another.
  const pairs: [string, string][] = Object.entries(span.value_aliases ?? {});

  const store = (): void => {
    const out: Record<string, string> = {};
    for (const [value, alias] of pairs) {
      if (value !== "" && alias !== "") out[value] = alias;
    }
    // Kept even when empty: the reading is still set to aliases, and dropping
    // the key would flip the menu back to as sent under the user's hands.
    span.value_aliases = out;
    onChange();
  };

  const draw = (): void => {
    rows.textContent = "";
    pairs.forEach((pair, i) => {
      const value = el("input", {
        type: "number",
        class: "alias",
        min: "0",
        value: pair[0],
        placeholder: "sends",
      });
      const alias = el("input", {
        type: "text",
        class: "alias",
        value: pair[1],
        placeholder: "draw",
      });
      const trouble = el("span", { class: "meta bad" });
      const check = (): void => {
        const missing = set
          ? [...new Set([...pair[1]].filter((c) => !set.includes(c)))]
          : [];
        alias.classList.toggle("bad", missing.length > 0);
        trouble.textContent = missing.length
          ? `The font does not draw ${missing.map((c) => JSON.stringify(c)).join(", ")}.`
          : "";
      };
      value.addEventListener("input", () => {
        pair[0] = value.value;
        store();
      });
      alias.addEventListener("input", () => {
        pair[1] = alias.value;
        check();
        store();
      });
      const drop = el("button", { class: "icon danger", title: "Remove this alias" }, "\u{1F5D1}");
      drop.addEventListener("click", () => {
        pairs.splice(i, 1);
        draw();
        store();
      });
      check();
      rows.append(
        el("div", { class: "alias-row" }, value, el("span", { class: "meta" }, "shows as"), alias, drop, trouble),
      );
    });
  };

  const add = el("button", { class: "add small" }, "Add an alias");
  add.addEventListener("click", () => {
    pairs.push(["", ""]);
    draw();
  });

  draw();
  wrap.append(el("label", { class: "meta" }, "aliases"), rows, add);
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
    labelEditor(
      readout,
      () => cellCount(readout.cells),
      display,
      profile,
      () => {
        refresh();
        onChange();
      },
    ).node,
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
  on: { label?: string; label_colour?: string },
  room: () => number | null,
  display: DisplayInfo,
  profile: Profile,
  onChange: () => void,
): { node: HTMLElement; check: () => void } {
  const box = el("input", {
    type: "text",
    class: "span-text",
    value: on.label ?? "",
    placeholder: "none",
  });

  const menu = el("select", { class: "colour" });
  // Absent rather than a colour of its own, so a label on a rule that is
  // already green does not arrive white until somebody notices.
  menu.append(el("option", { value: "" }, "same as the rule"));
  for (const name of display.colours) menu.append(el("option", { value: name }, name));
  menu.value = on.label_colour ?? "";
  menu.addEventListener("change", () => {
    if (menu.value) on.label_colour = menu.value;
    else delete on.label_colour;
    onChange();
  });

  const trouble = el("div", { class: "meta" });
  const check = (): void => {
    const label = on.label ?? "";
    menu.disabled = label === "";
    if (label === "") {
      box.classList.remove("bad");
      trouble.classList.remove("bad");
      trouble.textContent = "";
      return;
    }
    const cells = room();
    if (cells === null) {
      // A rule with no fixed width is as wide as the rest of the line leaves
      // it, which changes with every reading beside it. A label on one would
      // fit at one reading and be dropped at the next, coming and going on the
      // glass with nothing to say why, so the backend refuses it.
      box.classList.add("bad");
      trouble.classList.add("bad");
      trouble.textContent =
        "A label needs a rule that cannot change width, so give this one a " +
        "fixed width below. Without one it is as wide as the rest of the row " +
        "leaves it, and the label would come and go as the readings beside it " +
        "change.";
      return;
    }
    // A dash and a blank each side of it. The backend refuses a label with
    // less than that rather than crowding the line, so saying so here saves
    // the user finding out from the problem list.
    const needs = [...label].length + 4;
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
    if (box.value) on.label = box.value;
    else delete on.label;
    check();
    onChange();
  });
  check();

  return {
    node: el(
      "div",
      { class: "rule-label" },
      el("label", { class: "meta" }, "labelled ", box),
      el("label", { class: "meta" }, "in ", menu),
      trouble,
    ),
    check,
  };
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
  if (key === "width" && value === 0) return false;
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
  // A box is the whole answer, and the only one that holds for a gauge with no
  // range: whatever it reads, it draws this many cells.
  if (span.width) return span.width;
  // A gap takes what is left over, so it never asks for room of its own and
  // can never be the reason content will not fit.
  if (span.gap) return 0;
  if (isLiteral(span)) return (span.text ?? "").length;
  if (!span.source) return 0;
  if (isText(signals, span.source)) return textLength(signals, span.source);
  // As sent is a conversion onto the signal's own range, so both measure the
  // same way, as the daemon does. Aliases count as what they draw, and only
  // leave the number to measure when some value has none.
  const max = maxOf(signals, span.source);
  const aliases = span.value_aliases ?? {};
  const longest = Math.max(0, ...Object.values(aliases).map((a) => [...a].length));
  const everyValue =
    Object.keys(aliases).length > max &&
    Array.from({ length: max + 1 }, (_, v) => String(v) in aliases).every(Boolean);
  if (everyValue) return longest;
  const ends = span.reads ?? [0, max];
  const dp = span.decimals ?? 0;
  return Math.max(longest, ...ends.map((end) => end.toFixed(dp).length));
}

/**
 * How many blanks go before content of `len` held to `width`.
 *
 * One place rather than four, because a box, a field, the preview and the rule
 * all have to put the odd cell on the same side or the window stops agreeing
 * with the panel. Read the other way round, with the shorter length first, it
 * gives the glyphs a wider value loses off its front.
 */
function padBefore(align: string | undefined, len: number, width: number): number {
  const spare = Math.max(0, width - len);
  if (align === "right") return spare;
  if (align === "centre") return spare - Math.floor(spare / 2);
  return 0;
}

/** How many cells a run covers, and 0 for a run that does not parse. */
function cellCount(cells: string): number {
  const range = parseCells(cells);
  return range ? range[1] - range[0] + 1 : 0;
}

/** One cell of the preview: which piece drew it, what it draws, and in what. */
type PreviewCell = { span: Span; ch: string | null; colour?: string };

/**
 * Hold a piece's cells to its box, the way the daemon does.
 *
 * Padded on the side `align` says and cropped from the end it anchors away
 * from, which is the same bargain the field makes with its run.
 */
function boxFit(span: Span, cells: PreviewCell[]): PreviewCell[] {
  const width = span.width ?? 0;
  if (width === 0 || cells.length === width) return cells;
  if (cells.length > width) {
    const front = padBefore(span.align, width, cells.length);
    return cells.slice(front, front + width);
  }
  const before = padBefore(span.align, cells.length, width);
  const blank = (): PreviewCell => ({ span, ch: " " });
  return [
    ...Array.from({ length: before }, blank),
    ...cells,
    ...Array.from({ length: width - cells.length - before }, blank),
  ];
}

/**
 * The cells a reading itself can fill, ignoring any box around it.
 *
 * `spanWidth` answers with the box where there is one, which is what the fit
 * line wants. The preview wants the other number: how much of the box the
 * reading can actually cover, so the blanks held around it are drawn as
 * blanks rather than as more of the reading.
 */
function signalWidth(span: Span, signals: SignalView[]): number {
  return spanWidth({ ...span, width: 0 }, signals);
}

/**
 * Whether any piece is text DCS-BIOS gives no length for, so nothing bounds
 * the width. A number always has one: its range, or its own maximum.
 */
function unbounded(spans: Span[], signals: SignalView[]): boolean {
  return spans.some(
    (s) =>
      // A box bounds what nothing else does: in one, a reading of any width
      // draws its width and no more.
      !s.width &&
      !s.gap &&
      !isLiteral(s) &&
      s.source !== "" &&
      isText(signals, s.source ?? "") &&
      textLength(signals, s.source ?? "") === 0,
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
  // Which repaint is the current one. Two things are waited on now, the font
  // and the rules, and a late answer about a layout the user has already moved
  // on from must not be painted over the one they are looking at.
  let generation = 0;

  const refresh = (): void => {
    const range = parseCells(readout.cells);
    const width = range ? range[1] - range[0] + 1 : 0;
    const file = fontInUse(display, profile);
    generation += 1;
    if (!display.text_grid || !file || width === 0) {
      canvas.hidden = true;
      note.textContent =
        display.text_grid && !file ? "No font ships for this screen, so it cannot be drawn here." : "";
      return;
    }
    canvas.hidden = false;
    note.textContent = "";
    const mine = generation;
    void (async () => {
      const font = await glyphsFor(display.key, file);
      {
        // Which piece each cell comes from, so a cell can be drawn in that
        // piece's colour and size, or left as a block where a reading goes.
        // Built per piece rather than flat, because a gap cannot be measured
        // until everything that is not a gap has been laid out, the same way
        // the daemon does it.
        const spans = contentOf(readout);
        const groups: PreviewCell[][] = [];
        const gaps: number[] = [];
        for (const span of spans) {
          if (span.gap) {
            // A boxed gap knows its width before anything else is laid out, so
            // it is filled below with the rest of the rules. An elastic one
            // waits for the measuring.
            if (!span.width) {
              gaps.push(groups.length);
              groups.push([]);
              continue;
            }
            groups.push(Array.from({ length: span.width }, () => ({ span, ch: " " })));
            continue;
          }
          const group: PreviewCell[] = [];
          if (isLiteral(span)) {
            for (const ch of span.text ?? "") group.push({ span, ch });
          } else {
            // In a box, only as many cells as the reading itself can fill:
            // the rest are blanks being held, and drawing them as more of the
            // reading would hide the thing the box is for. A reading of no
            // known width could be any of them, so it takes the lot.
            const value = span.width
              ? Math.min(span.width, signalWidth(span, signals) || span.width)
              : spanWidth(span, signals);
            for (let i = 0; i < value; i += 1) group.push({ span, ch: null });
          }
          // Held to its box before the gaps are measured, which is the whole
          // point of one: the pieces after it do not move.
          groups.push(boxFit(span, group));
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
        // The rules last, once every one of them knows how wide it is. Asked
        // for rather than worked out here, because a rule drawn twice is a
        // rule that can drift from the one the panel gets.
        await Promise.all(
          spans.map(async (span, at) => {
            if (!span.gap || !span.rule) return;
            const cells = await dividerRule(groups[at]?.length ?? 0, span.label ?? "");
            groups[at] = cells.map((cell) => ({
              span,
              ch: cell.text,
              colour: cell.label ? span.label_colour ?? span.colour : span.colour,
            }));
          }),
        );
        if (mine !== generation) return;
        const cells = groups.flat();
        // Cropped and padded the way the field will be, so the preview shows
        // the loss rather than a line that fits in the window and not on the
        // panel.
        const front = padBefore(readout.align, width, cells.length);
        const shown = cells.length > width ? cells.slice(front, front + width) : cells;

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

        const offset = padBefore(readout.align, shown.length, width);
        shown.forEach((cell, i) => {
          const x = (offset + i) * cw;
          // A rule's label carries its own, which is the whole reason it reads
          // as a label rather than as part of the line.
          const colour = SWATCH[cell.colour ?? cell.span.colour ?? "white"] ?? "#f2f4f7";
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
      }
    })().catch(() => {
      if (mine !== generation) return;
      canvas.hidden = true;
      note.textContent = "The font could not be read.";
    });
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
  // Only a text grid draws a rule, the same as a whole field's divider. Kept
  // in the list for a piece that already is one, so glass that cannot draw it
  // says so through the problem list rather than by quietly reading as a gap.
  if (display.text_grid || kindOf(span) === "rule") {
    kind.append(el("option", { value: "rule" }, "a rule"));
  }
  kind.value = kindOf(span);
  kind.addEventListener("change", () => {
    // Everything on a piece describes the one value it draws, so switching
    // what it draws leaves none of it meaningful. Colour and size are the
    // exception: they are about how it looks, and the user picked them. A gap
    // keeps none of it, because it draws nothing to style.
    const next = kind.value as SpanKind;
    if (next === "gap" || next === "rule") {
      spans[index] = newSpan(next);
    } else {
      const kept: Span = { colour: span.colour, small: span.small, inverse: span.inverse };
      spans[index] = next === "text" ? { ...kept, text: "" } : { ...kept, source: "" };
    }
    setContent(readout, spans);
    redraw();
    onChange();
  });

  const body = el("div", { class: "span-body" });

  if (span.rule) {
    body.append(spanRule(span, opts, edited));
  } else if (span.gap) {
    body.append(
      el(
        "span",
        { class: "meta block" },
        "Blank, and as wide as whatever the rest of the row leaves. Put one " +
          "between two pieces to push them to opposite ends, or use two to " +
          "space three pieces evenly. It draws nothing itself, so it has " +
          "nothing to colour. Give it a fixed width below and it stops " +
          "measuring itself and becomes a spacer of exactly that many cells.",
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
        // Only fill in a conversion for a piece that had no number before, so
        // swapping the signal under a tuned range does not discard it, the
        // same bargain a lamp's test makes.
        if (!was || isText(signals, was)) {
          startReading(span, signals.find((s) => s.id === id));
        }
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
      // A signal the catalogue says reports characters is offered no range,
      // since one would usually mean nothing. One written anyway is kept and
      // shown: DCS-BIOS is not always right about what a signal is, the check
      // cautions rather than refuses, and quietly dropping it here would undo
      // the user's choice the moment the field was drawn.
      const textual = isText(signals, span.source);
      if (!textual || span.reads || span.value_aliases) {
        // Choosing between as sent, converted and aliases changes which boxes
        // there are, so that rebuilds. Typing in them does not.
        const signal = signals.find((s) => s.id === span.source);
        const set = alphabet(display, profile, span.small ?? false);
        body.append(
          conversionRow(span, signal, set, edited, () => {
            setContent(readout, spans);
            redraw();
            onChange();
          }),
        );
      }
      // A substitution is about what this signal sends, so it belongs to the
      // piece that reads it rather than to the field around it.
      body.append(aliasEditor(span, edited));
    }
  }

  // --- how it looks --------------------------------------------------------
  const style = el("div", { class: "span-style" });

  // A plain gap draws nothing and so has nothing to colour. A rule does: it
  // is the user's own addition rather than something the cockpit decided, so
  // its colour is theirs to pick, exactly as a divider's is.
  if (display.text_grid && (!span.gap || span.rule)) {
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

  // Last in the style row, because it is about where the piece sits rather
  // than what it draws, and every kind of piece can have one: a box on a gap
  // is a spacer of exactly that many blanks, and on a rule it is what lets it
  // carry a label.
  style.append(boxControls(span, readout, edited));

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
 * A rule inside a chain: what it says, and what it will look like.
 *
 * The same rule a whole field's divider draws, as one piece of a line instead
 * of the whole of it, which is what `NAV ----- 250` needs. Elastic by default:
 * it is a gap, so it is measured last and takes whatever the pieces each side
 * leave, and a reading that grows eats into the dashes rather than pushing
 * anything off the end. That is what the three separate fields with hand
 * counted cells this replaces could never do.
 *
 * Its own colour comes from the style row, the same chooser every piece has.
 * The label's is here, beside the label, because it is the label's.
 */
function spanRule(span: Span, opts: RowOptions, edited: () => void): HTMLElement {
  const { display, profile } = opts;
  const label = labelEditor(span, () => span.width || null, display, profile, edited);
  // Re-run when the width changes, which is what decides whether a label can
  // be here at all and whether it fits.
  ruleChecks.set(span, label.check);
  return el(
    "div",
    { class: "readout-extras" },
    el(
      "span",
      { class: "meta block" },
      "A line of dashes, as wide as whatever the rest of the row leaves. It " +
        "reads nothing, so it is on the glass from the moment the aircraft " +
        "loads. Give it a fixed width below to hold it to a size, which is " +
        "also what a label needs.",
    ),
    label.node,
  );
}

/**
 * The label check belonging to each rule piece on screen.
 *
 * The width control and the label sit in different parts of the same piece and
 * are built one after the other, so this is how the first reaches the second
 * without either owning the other. A weak map because the entry is only of
 * interest while its piece is on screen, and the chain is rebuilt from
 * scratch on every redraw.
 */
const ruleChecks = new WeakMap<Span, () => void>();

/**
 * Holding one piece to a fixed number of cells.
 *
 * Without a box a chain only holds still at its ends. A reading that goes from
 * four characters to three pulls everything after it one cell left, so a
 * layout built around one width comes apart at another, and there is no way to
 * tell ahead of time whether something will eventually run off the edge.
 *
 * The alignment inside the box is the user's, because the two useful answers
 * pull opposite ways: centred is right for a label and reads as drift on a
 * number, which loses half a cell from each edge every time it sheds a
 * character, while right keeps the digits pinned and grows the blanks in
 * front of them.
 */
function boxControls(span: Span, readout: Readout, edited: () => void): HTMLElement {
  const cells = cellCount(readout.cells);
  const width = el("input", {
    type: "number",
    class: "num small",
    min: "0",
    max: String(cells),
    value: String(span.width ?? 0),
    placeholder: "0",
  });
  const align = el("select", { class: "colour" });
  align.append(el("option", { value: "left" }, "left"));
  align.append(el("option", { value: "centre" }, "centred"));
  align.append(el("option", { value: "right" }, "right"));
  align.value = span.align ?? "left";

  const trouble = el("div", { class: "meta" });
  const check = (): void => {
    align.disabled = !span.width;
    const over = (span.width ?? 0) > cells;
    width.classList.toggle("bad", over);
    trouble.classList.toggle("bad", over);
    trouble.textContent = over
      ? `This field has ${cells} cells, so a piece ${span.width} wide cannot be drawn in it.`
      : "";
    ruleChecks.get(span)?.();
  };

  width.addEventListener("input", () => {
    const n = Number(width.value);
    if (n > 0) span.width = n;
    else delete span.width;
    // The alignment says nothing without a box, and leaving it behind would be
    // a setting the window shows greyed out and the file still carries.
    if (!span.width) delete span.align;
    check();
    edited();
  });
  align.addEventListener("change", () => {
    if (align.value === "left") delete span.align;
    else span.align = align.value as "right" | "centre";
    edited();
  });
  check();

  return el(
    "div",
    { class: "span-box" },
    el("label", { class: "meta" }, "width ", width),
    el("label", { class: "meta" }, "aligned ", align),
    el(
      "span",
      { class: "meta block" },
      "Cells this piece takes whatever it draws, so the pieces after it stay " +
        "where they are as it changes width. 0 leaves it as wide as its " +
        "value. Anything too long for the box is cropped from the end the " +
        "alignment anchors away from.",
    ),
    trouble,
  );
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
    // Only a text grid draws a rule, the same as a whole field's divider.
    if (display.text_grid) add.append(addOne("rule", "+ a rule"));
    wrap.append(add);

    // --- how wide it comes out ---------------------------------------------
    // Read again on each refresh rather than once here: every character typed
    // changes it, and it is the line that says the field will not fit.
    const range = parseCells(readout.cells);
    const cells = range ? range[1] - range[0] + 1 : 0;
    const hasGap = spans.some((s) => s.gap);
    // Only a gap with no box of its own still measures itself from what is
    // left. A boxed one is a fixed run of blanks, or of dashes, and counts
    // towards the width like anything else.
    const elastic = spans.filter((s) => s.gap && !s.width).length;
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
        const end =
          readout.align === "right"
            ? "first"
            : readout.align === "centre"
              ? "outermost"
              : "last";
        fit.classList.add("bad");
        fit.textContent =
          `This needs up to ${widest} cells and has ${cells}. The ${end} ` +
          `${lost} character${lost === 1 ? "" : "s"} would be dropped, and ` +
          "nothing on the panel would say so.";
      } else if (loose) {
        fit.textContent =
          `Text DCS-BIOS gives no length for has no known width, so this may run past its ${cells} cells.`;
      } else if (elastic > 0) {
        const spare = cells - widest;
        const rules = spans.filter((s) => s.gap && s.rule && !s.width).length;
        const what = rules === elastic ? "rule" : rules > 0 ? "gap or rule" : "gap";
        fit.textContent =
          `Up to ${widest} of ${cells} cells, and the ${spare} left over go to ` +
          `the ${what}${elastic === 1 ? "" : "s"}. ` +
          "A reading that grows takes the room back from there, so the ends stay put.";
      } else if (widest > 0) {
        fit.textContent = `Up to ${widest} of ${cells} cells.`;
      }
    };
    wrap.append(fit);

    // --- the rest of the field ---------------------------------------------
    const extras = el("div", { class: "readout-extras" });
    if (range && range[1] > range[0] && !hasGap) {
      const align = el("select", { class: "colour" });
      align.append(el("option", { value: "left" }, "left"));
      align.append(el("option", { value: "centre" }, "centred"));
      align.append(el("option", { value: "right" }, "right"));
      align.value = readout.align ?? "left";
      align.addEventListener("change", () => {
        if (align.value === "left") delete readout.align;
        else readout.align = align.value as "right" | "centre";
        redraw();
        onChange();
      });
      extras.append(
        el(
          "label",
          { class: "meta" },
          "aligned ",
          align,
          el(
            "span",
            { class: "meta block" },
            "Which end of the run the whole line anchors to. A scratchpad " +
              "wants right: digits enter at the last cell, and DCS-BIOS can " +
              "send more characters than there are cells. To hold one piece " +
              "in place rather than the line, give that piece a width instead.",
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
    shows.append(flagSlot(readout), cautionSlot(readout), chainEditor(opts, () => {}));
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
    const held = s.width ? ` held to ${s.width} cells` : "";
    if (s.rule) return s.label ? `a rule labelled ${s.label}${held}` : `a rule${held}`;
    if (s.gap) return `a gap${held}`;
    if (kindOf(s) === "signal") {
      // How it draws the number, so a reset that only changes that says so
      // rather than showing the same line twice.
      const aliases = Object.entries(s.value_aliases ?? {});
      const drawn = aliases.length
        ? ` as ${aliases.map(([v, a]) => `${v}=${a}`).join(" ")}`
        : s.reads
          ? ` converted to ${s.reads[0]} to ${s.reads[1]}`
          : "";
      return `${s.source ? s.source : "a reading nobody has chosen yet"}${drawn}${held}`;
    }
    return `${s.text ? JSON.stringify(s.text) : "an empty piece of text"}${held}`;
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
 * A new rule takes it, because a white rule across a green page reads as a
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
      const add = (kind: SpanKind, label: string): HTMLElement => {
        const button = el("button", { class: "add" }, label);
        button.addEventListener("click", () => {
          const fresh: Readout = {
            device: device.key,
            display: display.key,
            cells: region.cells,
            source: "",
          };
          // Every first piece starts a chain, a rule included. A rule made
          // here used to be a whole-field divider, which holds no pieces, so
          // nothing could be added beside it and its kind could not be
          // changed. A divider already in a profile still loads and edits.
          const first = newSpan(kind);
          if (kind === "rule") first.colour = agreedColour(mine());
          setContent(fresh, [first]);
          readouts.push(fresh);
          redraw();
          onChange();
        });
        return button;
      };
      const buttons = el(
        "div",
        { class: "chain-add" },
        add("signal", "+ a reading"),
        add("text", "+ text"),
        add("gap", "+ a gap"),
      );
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
