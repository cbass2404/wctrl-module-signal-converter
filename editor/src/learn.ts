// Learn mode: naming a signal by moving it in the cockpit.
//
// A module publishes hundreds of signals and as many as 1,440, named by
// DCS-BIOS rather than by anyone who flies. The search box answers "what is it
// called" only if you already half know. This answers "I do not know what this
// is called": flip it, and see what moved.
//
// It is also why the search box needs no browse-everything mode. Scrolling
// 1,440 identifiers was never going to find anything.

import { learnAgain, learnPoll, learnStart, learnStop } from "./api";
import type { LearnChange, SignalView } from "./types";

/** How often the window asks what has moved. Fast enough to feel live. */
const POLL_MS = 400;

/** Enough to find what you flipped, few enough to read. */
const MAX_ROWS = 25;

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

/**
 * The module and aircraft of the profile being edited.
 *
 * Held here rather than passed down through every picker. There is exactly one
 * profile open at a time, and threading it through the binding editor and the
 * display editor to reach a button would touch a lot of code that has no
 * interest in it.
 */
let context: { module: string; aircraft: string[] } | null = null;

export function setLearnContext(module: string, aircraft: string[]): void {
  context = { module, aircraft };
}

/** Closes whatever learn panel is open. One at a time, one socket. */
let closeOpen: (() => void) | null = null;

/**
 * Whether any panel still wants the listener.
 *
 * Learn mode is off unless a panel is open. Nothing joins the multicast group
 * until the user presses Learn, and closing the panel releases it again, so a
 * window left open beside a mission is not reading the stream. The cost of
 * that is about a second of "reading the cockpit" on the next open, which is
 * the right trade: the user is deciding when to spend it.
 */
let wanted = false;

/**
 * Let the listener go, unless something takes it in this same tick.
 *
 * A panel closing to make way for another is the ordinary case, and both
 * happen synchronously. Deciding a tick later means the replacement has
 * already claimed it, so the socket is not torn down and rebuilt for a click.
 */
function releaseListener(): void {
  wanted = false;
  window.setTimeout(() => {
    if (!wanted) void learnStop();
  }, 0);
}

/** Called when the editor leaves a profile, so nothing keeps listening. */
export function stopLearning(): void {
  closeOpen?.();
  wanted = false;
  context = null;
  void learnStop();
}

/**
 * Show a value the way the log does: quoted when it is characters.
 *
 * On a display field the spaces are the layout, so `" 1"` and `"1 "` are
 * different readings and printing them bare would make them look identical.
 */
function show(value: string, text: boolean): string {
  return text ? JSON.stringify(value) : value;
}

interface PanelOptions {
  /** What this picker can actually take, already filtered by its caller. */
  signals: SignalView[];
  onPick: (id: string) => void;
}

/**
 * The learn panel, opened under one signal picker.
 *
 * Returns its own element and a closer. Polling stops when it closes, and the
 * listener stops when the editor leaves the profile, so a window left open on
 * the profile list is not sitting on a socket.
 */
export function learnPanel(opts: PanelOptions): { node: HTMLElement; close: () => void } {
  const byId = new Map(opts.signals.map((s) => [s.id, s]));

  const status = el("div", { class: "learn-status meta" }, "Starting.");
  const rows = el("div", { class: "results open learn-rows" });
  const again = el("button", { class: "small", type: "button" }, "Watch again");
  const done = el("button", { class: "small", type: "button" }, "Stop");
  const head = el(
    "div",
    { class: "learn-head" },
    el("span", { class: "meta" }, "Reading the DCS-BIOS stream while this is open."),
    again,
    done,
  );
  const node = el("div", { class: "learn" }, head, status, rows);

  let timer: number | null = null;
  let stopped = false;

  const close = (): void => {
    stopped = true;
    if (timer !== null) window.clearTimeout(timer);
    timer = null;
    node.remove();
    if (closeOpen === close) closeOpen = null;
    releaseListener();
  };

  // A click on a row must not blur the search input before it lands, the same
  // race the results list had.
  rows.addEventListener("mousedown", (e) => e.preventDefault());

  function draw(changes: LearnChange[], note: string, bad = false): void {
    status.replaceChildren(note);
    status.classList.toggle("bad", bad);

    // A signal this field could never take is not offered, because picking it
    // would produce a binding that silently never fires. It is counted, so a
    // user who flips a scratchpad while editing a lamp is told why their switch
    // did not appear rather than left to wonder.
    const offered = changes.filter((c) => byId.has(c.id));
    const hidden = changes.length - offered.length;

    rows.replaceChildren();
    for (const change of offered.slice(0, MAX_ROWS)) {
      const signal = byId.get(change.id);
      if (!signal) continue;
      const moved =
        `${show(change.from ?? "", change.text)} → ${show(change.to, change.text)}` +
        (change.moves > 1 ? `   moved ${change.moves} times` : "");
      const row = el(
        "button",
        { class: "result", type: "button" },
        el("span", { class: "desc" }, signal.description || signal.id),
        el("span", { class: "sub" }, `${signal.category} · ${moved}`),
        el("span", { class: "id" }, signal.id),
      );
      row.addEventListener("click", () => {
        opts.onPick(change.id);
        close();
      });
      rows.append(row);
    }
    if (hidden > 0) {
      rows.append(
        el(
          "div",
          { class: "learn-hidden meta" },
          `${hidden} other signal(s) moved that this field cannot read.`,
        ),
      );
    }
    rows.classList.toggle("open", rows.childElementCount > 0);
  }

  async function tick(): Promise<void> {
    if (stopped) return;
    try {
      const report = await learnPoll();
      if (stopped) return;

      if (report.error) {
        draw([], report.error, true);
      } else if (!report.listening) {
        draw([], "Not listening.", true);
      } else if (report.datagrams === 0) {
        // Silence is the common first-run problem and it has three causes, so
        // the message names all three rather than saying "no data".
        draw(
          [],
          "No DCS-BIOS stream. Check DCS is running with a mission loaded, that " +
            "DCS-BIOS is installed in Saved Games, and that multicast is not blocked.",
        );
      } else if (context && report.aircraft && !context.aircraft.includes(report.aircraft)) {
        // Nothing they flip will ever appear, and without this the panel just
        // sits there looking broken.
        draw(
          report.changes,
          `DCS is flying ${report.aircraft}, which this profile does not cover. ` +
            `Signals below are read against ${report.module}.`,
          true,
        );
      } else if (!report.ready) {
        draw([], "Reading the cockpit. A second, while every signal gets a starting value.");
      } else if (report.changes.length === 0) {
        draw([], "Ready. Flip the switch, or move the control, in the cockpit.");
      } else {
        draw(report.changes, "Fewest movements first, so a switch you threw once is at the top.");
      }
    } catch (e) {
      draw([], e instanceof Error ? e.message : String(e), true);
    }
    if (!stopped) timer = window.setTimeout(() => void tick(), POLL_MS);
  }

  done.addEventListener("click", () => close());

  again.addEventListener("click", () => {
    void learnAgain();
    draw([], "Watching again. Flip the next one.");
  });

  closeOpen?.();
  closeOpen = close;
  wanted = true;

  void (async () => {
    if (!context) {
      draw([], "No profile open, so there is no module to read signals against.", true);
      return;
    }
    try {
      await learnStart(context.module);
    } catch (e) {
      draw([], e instanceof Error ? e.message : String(e), true);
      return;
    }
    // A fresh sheet, so what is listed is what happened after the panel opened
    // rather than whatever the cockpit did while it was closed.
    await learnAgain();
    void tick();
  })();

  return { node, close };
}

/** Whether a picker should offer the button at all. */
export function canLearn(): boolean {
  return context !== null;
}
