// The signal picker.
//
// A module carries hundreds to 1,440 signals, so this is a search box rather
// than a list. Nothing is shown until three characters are typed, and the match
// runs over description, category and identifier at once.
//
// Rows carry two lines because descriptions repeat: 696 of CH-47F's 1,440
// signals share a description with another signal in the same module, and
// `Call Button Light (Yellow)` occurs six times, separated only by category.

import type { SignalView } from "./types";

const MIN_CHARS = 3;

/** Enough to scroll through, few enough to render without stutter. */
const MAX_ROWS = 50;

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else node.setAttribute(k, v);
  }
  node.append(...children);
  return node;
}

/** Kept clear of the window edges, which is where a clipped hint comes from. */
const HINT_MARGIN = 10;

/**
 * Put the hint beside its icon, inside the window.
 *
 * Positioned from script rather than from CSS because the correct side depends
 * on where the icon happens to sit: the icon is in a table column that reaches
 * the right edge, so a box that always opened rightwards was cut off there. CSS
 * cannot ask how much room is left.
 *
 * Fixed positioning also takes the hint out of the flow entirely, so no ancestor
 * can clip it.
 */
function placeHint(info: HTMLElement, hint: HTMLElement): void {
  const icon = info.getBoundingClientRect();

  // Measure the natural box before deciding where to put it.
  hint.style.left = "0px";
  hint.style.top = "0px";
  hint.style.maxWidth = `${Math.min(300, window.innerWidth - HINT_MARGIN * 2)}px`;
  const box = hint.getBoundingClientRect();

  // Prefer the right of the icon, flip to the left when it will not fit, and
  // clamp when neither side has room rather than letting it run off.
  let left = icon.right + 8;
  if (left + box.width > window.innerWidth - HINT_MARGIN) {
    left = icon.left - box.width - 8;
  }
  if (left < HINT_MARGIN) {
    left = Math.max(HINT_MARGIN, window.innerWidth - box.width - HINT_MARGIN);
  }

  // Vertically it rides up from the bottom edge rather than being cut off, which
  // matters for a signal with many labelled positions near the foot of a list.
  let top = icon.top - 4;
  if (top + box.height > window.innerHeight - HINT_MARGIN) {
    top = window.innerHeight - box.height - HINT_MARGIN;
  }
  if (top < HINT_MARGIN) top = HINT_MARGIN;

  hint.style.left = `${left}px`;
  hint.style.top = `${top}px`;
}

/**
 * The hint currently showing, if any.
 *
 * One shared reference and one shared listener, rather than a listener per
 * icon. Every re-render of a binding builds fresh icons, so per-icon window
 * listeners would accumulate for the life of the window and keep every detached
 * hint alive with them.
 */
let open: { info: HTMLElement; hint: HTMLElement } | null = null;

// A scroll under an open hint would leave it behind, because fixed positioning
// does not move with the page. Capturing, so it also catches scrolling inside
// the results list rather than only on the window.
window.addEventListener(
  "scroll",
  () => {
    if (open) placeHint(open.info, open.hint);
  },
  { passive: true, capture: true },
);

/**
 * The rules and parameters of one signal, as the catalogue records them.
 *
 * Shown behind an info icon that opens on hover *and* on focus, because a
 * hover-only hint cannot be reached from the keyboard.
 */
export function hintFor(signal: SignalView): HTMLElement {
  const lines = el(
    "span",
    { class: "hint" },
    el("strong", {}, signal.id),
    el("span", { class: "meta block" }, `${signal.control_type} · ${signal.category}`),
  );
  if (signal.reads) {
    lines.append(el("span", { class: "block" }, `${signal.reads}, 0 to ${signal.max_value}`));
  } else {
    lines.append(el("span", { class: "block" }, `0 to ${signal.max_value}`));
  }
  if (signal.values.length > 0) {
    const table = el("span", { class: "values block" });
    for (const v of signal.values) {
      table.append(el("span", {}, `${v.value} = ${v.label}`));
    }
    lines.append(table);
  }

  const info = el(
    "span",
    { class: "info", tabindex: "0", role: "button", "aria-label": "Signal details" },
    "i",
    lines,
  );

  // The CSS reveals the hint on hover and focus; these only decide where it
  // lands, once it is showing and can be measured.
  const place = (): void => {
    open = { info, hint: lines };
    placeHint(info, lines);
  };
  const forget = (): void => {
    if (open?.info === info) open = null;
  };
  info.addEventListener("mouseenter", place);
  info.addEventListener("focus", place);
  info.addEventListener("mouseleave", forget);
  info.addEventListener("blur", forget);

  return info;
}

/** Score a signal against the query. Lower sorts first; null means no match. */
function score(signal: SignalView, q: string): number | null {
  const description = signal.description.toLowerCase();
  const category = signal.category.toLowerCase();
  const id = signal.id.toLowerCase();

  // A description that starts with the query is almost always what was meant.
  if (description.startsWith(q)) return 0;
  if (id.startsWith(q)) return 1;
  if (description.includes(q)) return 2;
  if (id.includes(q)) return 3;
  if (category.includes(q)) return 4;
  return null;
}

export function search(signals: SignalView[], query: string): SignalView[] {
  const q = query.trim().toLowerCase();
  if (q.length < MIN_CHARS) return [];
  const hits: { signal: SignalView; rank: number }[] = [];
  for (const signal of signals) {
    const rank = score(signal, q);
    if (rank !== null) hits.push({ signal, rank });
  }
  // Stable within a rank, and the incoming list is already lamps-first, so a
  // cockpit lamp outranks a switch that matched equally well.
  hits.sort((a, b) => a.rank - b.rank);
  return hits.slice(0, MAX_ROWS).map((h) => h.signal);
}

export interface PickerOptions {
  signals: SignalView[];
  /** Currently bound signal id, or the empty string. */
  value: string;
  onPick: (id: string) => void;
}

/**
 * A search box bound to one condition's source.
 *
 * The empty state is deliberate. Learn mode, which watches the cockpit while
 * the user flips a switch, is how someone finds a signal they cannot name, so
 * there is no browse-everything mode to fall back on here.
 */
export function signalPicker(opts: PickerOptions): HTMLElement {
  const byId = new Map(opts.signals.map((s) => [s.id, s]));

  const input = el("input", {
    type: "text",
    class: "signal",
    placeholder: "search signals",
    spellcheck: "false",
    autocomplete: "off",
  });
  input.value = opts.value;

  const results = el("div", { class: "results" });

  // Keep the input focused while a row is being clicked.
  //
  // Without this, pressing the mouse on a row blurs the input, the blur handler
  // schedules the list to close, and the click has to win a race against that
  // timer. It usually did, which made the failure look like the hit area was
  // wrong: the row highlighted on hover but only sometimes took the click.
  // Cancelling the mousedown means focus never moves and there is no race.
  results.addEventListener("mousedown", (e) => e.preventDefault());
  const detail = el("div", { class: "detail" });
  const wrap = el("div", { class: "picker-field" }, input, results, detail);

  function showDetail(id: string): void {
    detail.replaceChildren();
    const signal = byId.get(id);
    if (!signal) {
      if (id) detail.append(el("span", { class: "bad" }, `${id} is not in this module`));
      return;
    }
    detail.append(
      el("span", { class: "meta" }, `${signal.description} · ${signal.category}`),
      hintFor(signal),
    );
  }

  function close(): void {
    results.replaceChildren();
    results.classList.remove("open");
  }

  function pick(signal: SignalView): void {
    input.value = signal.id;
    close();
    showDetail(signal.id);
    opts.onPick(signal.id);
  }

  input.addEventListener("input", () => {
    const hits = search(opts.signals, input.value);
    results.replaceChildren();
    if (hits.length === 0) {
      close();
      return;
    }
    for (const signal of hits) {
      const row = el(
        "button",
        { class: "result", type: "button" },
        el("span", { class: "desc" }, signal.description),
        el("span", { class: "sub" }, signal.category),
        el("span", { class: "id" }, signal.id),
      );
      row.addEventListener("click", () => pick(signal));
      results.append(row);
    }
    results.classList.add("open");
  });

  // Enter takes the first hit, which is the common case once the query is
  // specific enough to leave one obvious answer at the top.
  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      close();
      return;
    }
    if (e.key !== "Enter") return;
    const first = results.querySelector<HTMLButtonElement>("button.result");
    if (first) {
      e.preventDefault();
      first.click();
    }
  });

  // Losing focus commits whatever is typed if it names a real signal, so a
  // pasted identifier works without going through the list.
  input.addEventListener("blur", () => {
    window.setTimeout(() => {
      close();
      const typed = input.value.trim();
      if (typed !== opts.value && byId.has(typed)) {
        opts.onPick(typed);
      }
      showDetail(typed);
    }, 120);
  });

  showDetail(opts.value);
  return wrap;
}
