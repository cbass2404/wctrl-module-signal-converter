// Editing the fields of a segment display.
//
// A display field is not a lamp and is deliberately not shaped like one. A lamp
// asks "under what conditions"; a field asks "which cells, fed by what". There
// are no conditions here at all: a field has one owner, because on an aircraft
// that drives its own glass the cockpit has already decided what belongs there,
// and on any other the user has.

import { signalPicker } from "./typeahead";
import type { Device, DisplayInfo, Readout, SignalView } from "./types";

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
    const r = parseCells(other.cells);
    if (r && first <= r[1] && r[0] <= last) {
      return `Cells ${other.cells} are already taken by ${other.source || "another field"}.`;
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

function isText(signals: SignalView[], id: string): boolean {
  // A signal the catalogue reports as characters needs no conversion. Anything
  // else is a number, and a number needs to be told what the dial reads.
  return signals.find((x) => x.id === id)?.text ?? false;
}

/** How many characters a text signal will hand over, or 0. */
function textLength(signals: SignalView[], id: string): number {
  return signals.find((x) => x.id === id)?.length ?? 0;
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

  // --- cells ---------------------------------------------------------------
  const cells = el("input", { type: "text", class: "cells", value: readout.cells });
  const cellNote = el("div", { class: "meta" });
  const recheck = (): void => {
    const problem = cellProblem(cells.value, display, all, readout);
    cells.classList.toggle("bad", problem !== null);
    if (problem) {
      cellNote.textContent = problem;
      cellNote.classList.add("bad");
      return;
    }
    cellNote.classList.remove("bad");
    const range = parseCells(cells.value);
    const n = range ? range[1] - range[0] + 1 : 0;
    cellNote.textContent = range
      ? `${n} cell${n === 1 ? "" : "s"}, ${shapesIn(range, display)}`
      : "";
  };
  cells.addEventListener("input", () => {
    recheck();
    if (!cells.classList.contains("bad")) {
      readout.cells = cells.value.trim();
      onChange();
    }
  });
  recheck();
  tr.append(el("td", {}, cells, cellNote));

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
  tr.append(el("td", {}, picker, extras));

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
      readouts.push({ device: device.key, display: display.key, cells: "0", source: "" });
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
            el("tr", {}, el("th", {}, "Cells"), el("th", {}, "Shows"), el("th", { class: "num" }, "")),
          ),
          body,
        ),
      ),
    );
  }
  return wrap;
}
