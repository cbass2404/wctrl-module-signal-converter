// Editing the fields of a segment display.
//
// A display field is not a lamp and is deliberately not shaped like one. A lamp
// asks "under what conditions"; a field asks "which cells, fed by what". There
// are no conditions here at all: a field has one owner, because on an aircraft
// that drives its own glass the cockpit has already decided what belongs there,
// and on any other the user has.

import { flagSlot } from "./flags";
import { signalPicker } from "./typeahead";
import type { Device, DisplayInfo, Readout, RegionInfo, SignalView } from "./types";

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
      return `${describe(other.cells, display)} is already taken by ${other.source || "another field"}.`;
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
function aliasEditor(readout: Readout, onChange: () => void): HTMLElement {
  const wrap = el("div", { class: "aliases" });
  const rows = el("div", { class: "alias-rows" });
  // Held as pairs rather than edited in place on the object, because renaming a
  // key means deleting and re-adding it, and a half-typed name would collide
  // with whatever it passes through on the way.
  let pairs: [string, string][] = Object.entries(readout.aliases ?? {});

  const store = (): void => {
    const out: Record<string, string> = {};
    for (const [from, to] of pairs) {
      if (from !== "") out[from] = to;
    }
    if (Object.keys(out).length === 0) delete readout.aliases;
    else readout.aliases = out;
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

interface RowOptions {
  readout: Readout;
  display: DisplayInfo;
  all: Readout[];
  signals: SignalView[];
  onChange: () => void;
  onRemove: () => void;
}

function row(opts: RowOptions): HTMLTableRowElement {
  const { readout, display, all, signals, onChange } = opts;
  const tr = el("tr");

  tr.append(cellChooser(readout, display, all, onChange));

  // --- source --------------------------------------------------------------
  const extras = el("div", { class: "readout-extras" });
  const drawExtras = (): void => {
    extras.textContent = "";
    if (!readout.source) return;

    if (isText(signals, readout.source)) {
      // A field that already reports characters needs no range, and offering
      // one would invite a conversion that means nothing.
      delete readout.reads;
      delete readout.decimals;
      const chars = textLength(signals, readout.source);
      const span = parseCells(readout.cells);
      const width = span ? span[1] - span[0] + 1 : 0;
      if (chars > width && width > 0) {
        extras.append(
          el(
            "span",
            { class: "meta block" },
            `This signal is ${chars} characters and you have given it ${width} ` +
              `cell${width === 1 ? "" : "s"}. ` +
              (width === 1
                ? "A single cell takes the whole value as one glyph, which is " +
                  "how a two-character field is drawn on this hardware."
                : "The extra characters are cropped from whichever end the " +
                  "alignment is not anchored to."),
          ),
        );
      }
    } else {
      const low = el("input", {
        type: "number",
        class: "num small",
        value: String(readout.reads?.[0] ?? 0),
      });
      const high = el("input", {
        type: "number",
        class: "num small",
        value: String(readout.reads?.[1] ?? 100),
      });
      const dp = el("input", {
        type: "number",
        class: "num small",
        min: "0",
        max: "3",
        value: String(readout.decimals ?? 0),
      });
      const sync = (): void => {
        readout.reads = [Number(low.value), Number(high.value)];
        readout.decimals = Number(dp.value) || 0;
        onChange();
      };
      for (const box of [low, high, dp]) box.addEventListener("input", sync);
      if (!readout.reads) sync();
      extras.append(
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

    // Alignment only means something across more than one cell.
    const range = parseCells(readout.cells);
    if (range && range[1] > range[0]) {
      const right = el("input", { type: "checkbox" });
      right.checked = readout.align === "right";
      right.addEventListener("change", () => {
        if (right.checked) readout.align = "right";
        else delete readout.align;
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
            "Anchor the text to the last cell. A scratchpad wants this: digits " +
              "enter at the right, and DCS-BIOS can send more characters than " +
              "there are cells.",
          ),
        ),
      );
    }

    const stations = seats(signals);
    if (stations.length > 1) {
      extras.append(seatChooser(readout, stations, onChange));
    }

    extras.append(aliasEditor(readout, onChange));
  };

  const picker = signalPicker({
    signals,
    value: readout.source,
    onPick: (id) => {
      readout.source = id;
      drawExtras();
      onChange();
    },
  });
  drawExtras();
  tr.append(el("td", {}, picker, flagSlot(readout), extras));

  // --- remove --------------------------------------------------------------
  const remove = el("button", { class: "icon danger", title: "Remove this field" }, "\u{1F5D1}");
  let armed = false;
  remove.addEventListener("click", () => {
    if (!armed) {
      armed = true;
      remove.classList.add("armed");
      remove.title = "Click again to remove";
      return;
    }
    opts.onRemove();
  });
  tr.append(el("td", { class: "num" }, remove));
  return tr;
}

/**
 * The first region nothing has claimed yet, so "Add a field" lands somewhere
 * usable rather than always on cell 0 with an overlap warning already showing.
 */
function firstFree(display: DisplayInfo, taken: Readout[]): string {
  for (const region of display.regions) {
    if (!taken.some((r) => r.cells === region.cells)) return region.cells;
  }
  return display.regions[0]?.cells ?? "0";
}

/** The display section for one device, or null if it has no glass. */
export function displaySection(
  device: Device,
  readouts: Readout[],
  signals: SignalView[],
  onChange: () => void,
): HTMLElement | null {
  if (device.displays.length === 0) return null;
  const wrap = el("div", { class: "displays" });

  for (const display of device.displays) {
    const mine = (): Readout[] =>
      readouts.filter((r) => r.device === device.key && r.display === display.key);

    const body = el("tbody");
    const redraw = (): void => {
      body.textContent = "";
      const list = mine();
      if (list.length === 0) {
        body.append(
          el(
            "tr",
            {},
            el(
              "td",
              { colspan: "3", class: "meta" },
              "Nothing is shown on this display yet.",
            ),
          ),
        );
      }
      for (const r of list) {
        body.append(
          row({
            readout: r,
            display,
            all: readouts,
            signals,
            onChange,
            onRemove: () => {
              readouts.splice(readouts.indexOf(r), 1);
              redraw();
              onChange();
            },
          }),
        );
      }
    };
    redraw();

    const add = el("button", { class: "add" }, "Add a field");
    add.addEventListener("click", () => {
      readouts.push({
        device: device.key,
        display: display.key,
        cells: firstFree(display, mine()),
        source: "",
      });
      redraw();
      onChange();
    });

    wrap.append(
      el(
        "div",
        { class: "display" },
        el(
          "div",
          { class: "display-head" },
          el("span", { class: "name" }, `${display.key} display`),
          el("span", { class: "meta" }, `${display.cells} cells`),
          add,
        ),
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
