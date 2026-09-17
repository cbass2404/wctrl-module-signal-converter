// Editing what drives one lamp.
//
// A binding carries a *list* of conditions and every one of them must hold. The
// single-condition case is the common one but not the shape: the A-10C
// half-flaps lamp needs the lever at MVR and the gauge inside the half window,
// because the flaps travel through that window on the way to DN and the lamp
// would otherwise flash on the way past.

import { hintFor, signalPicker } from "./typeahead";
import type { Binding, Branch, Condition, Led, OnWhen, SignalView } from "./types";

type TestKind = "equals" | "in" | "gte" | "lte" | "between" | "scale";

const TEST_LABELS: Record<TestKind, string> = {
  equals: "is exactly",
  in: "is one of",
  gte: "is at least",
  lte: "is at most",
  between: "is between",
  scale: "scaled across",
};

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

export function testKind(w: OnWhen): TestKind {
  return Object.keys(w)[0] as TestKind;
}

/**
 * A sensible test for a signal the user has just chosen, so that picking the
 * signal is usually the only step.
 *
 * A one-bit signal is on or off. A wide signal onto a lamp that can dim is
 * almost always meant to follow it, which is what backlights do. Anything else
 * gets a threshold the user can adjust.
 */
export function defaultTest(signal: SignalView | undefined, led: Led): OnWhen {
  if (!signal) return { equals: 1 };
  if (signal.max_value <= 1) return { equals: 1 };
  if (led.dimmable) return { scale: [0, signal.max_value] };
  return { gte: Math.round(signal.max_value / 2) };
}

/** Rebuild a test of a different kind, keeping any numbers that still apply. */
function convert(w: OnWhen, kind: TestKind, signal: SignalView | undefined): OnWhen {
  const max = signal?.max_value ?? 65535;
  const current = numbersOf(w);
  const first = current[0] ?? 1;
  switch (kind) {
    case "equals":
      return { equals: first };
    case "in":
      return { in: current.length > 0 ? current : [first] };
    case "gte":
      return { gte: first };
    case "lte":
      return { lte: first };
    case "between":
      return { between: [current[0] ?? 0, current[1] ?? max] };
    case "scale":
      return { scale: [current[0] ?? 0, current[1] ?? max] };
  }
}

function numbersOf(w: OnWhen): number[] {
  if ("equals" in w) return [w.equals];
  if ("in" in w) return w.in;
  if ("gte" in w) return [w.gte];
  if ("lte" in w) return [w.lte];
  if ("between" in w) return w.between;
  return w.scale;
}

function numberInput(value: number, max: number, onChange: (n: number) => void): HTMLInputElement {
  const input = el("input", {
    type: "number",
    class: "value",
    min: "0",
    max: String(max),
    value: String(value),
  });
  input.addEventListener("change", () => {
    const n = Number(input.value);
    onChange(Number.isFinite(n) ? n : 0);
  });
  return input;
}

/**
 * The value side of a condition.
 *
 * A signal with labelled positions gets a dropdown of exactly those positions,
 * so the user cannot ask for a value the signal is incapable of reporting.
 */
function valueControls(
  condition: Condition,
  signal: SignalView | undefined,
  onChange: () => void,
): HTMLElement {
  const kind = testKind(condition.on_when);
  const max = signal?.max_value ?? 65535;
  const wrap = el("span", { class: "values-row" });

  const discrete = signal && signal.values.length > 0;

  if (kind === "equals" && discrete && signal) {
    const select = el("select", { class: "value" });
    for (const v of signal.values) {
      const option = el("option", { value: String(v.value) }, `${v.value} = ${v.label}`);
      select.append(option);
    }
    select.value = String(numbersOf(condition.on_when)[0] ?? 0);
    select.addEventListener("change", () => {
      condition.on_when = { equals: Number(select.value) };
      onChange();
    });
    wrap.append(select);
    return wrap;
  }

  if (kind === "in") {
    const current = numbersOf(condition.on_when);
    const input = el("input", {
      type: "text",
      class: "value wide",
      value: current.join(", "),
      placeholder: "1, 2",
    });
    input.addEventListener("change", () => {
      const parsed = input.value
        .split(",")
        .map((part) => Number(part.trim()))
        .filter((n) => Number.isFinite(n));
      condition.on_when = { in: parsed.length > 0 ? parsed : [0] };
      onChange();
    });
    wrap.append(input);
    return wrap;
  }

  if (kind === "between" || kind === "scale") {
    const [lo, hi] = numbersOf(condition.on_when);
    wrap.append(
      numberInput(lo ?? 0, max, (n) => {
        const pair: [number, number] = [n, numbersOf(condition.on_when)[1] ?? max];
        condition.on_when = kind === "between" ? { between: pair } : { scale: pair };
        onChange();
      }),
      el("span", { class: "sep" }, "to"),
      numberInput(hi ?? max, max, (n) => {
        const pair: [number, number] = [numbersOf(condition.on_when)[0] ?? 0, n];
        condition.on_when = kind === "between" ? { between: pair } : { scale: pair };
        onChange();
      }),
    );
    return wrap;
  }

  wrap.append(
    numberInput(numbersOf(condition.on_when)[0] ?? 0, max, (n) => {
      condition.on_when =
        kind === "equals" ? { equals: n } : kind === "gte" ? { gte: n } : { lte: n };
      onChange();
    }),
  );
  return wrap;
}

export interface BindingEditorOptions {
  binding: Binding;
  led: Led;
  signals: SignalView[];
  /**
   * The other lamps on this device, for the mirror target list. Filtered to
   * dimmers by the caller, since only they can mirror or be mirrored.
   */
  siblings: Led[];
  /**
   * This lamp as the shipped profile has it, when there is one. Drives the
   * per-lamp revert, so one lamp can be put back without discarding every other
   * edit in the profile the way the profile-level Reset does.
   */
  shipped?: Binding | undefined;
  /** Called whenever the binding changes, so the window can mark itself dirty. */
  onChange: () => void;
}

/** What a revert compares and copies. The device and lamp never change. */
function meaningfulPart(b: Binding): string {
  return JSON.stringify({
    conditions: b.conditions,
    any_of: b.any_of ?? [],
    always: b.always ?? false,
    same_as: b.same_as ?? null,
    on: b.on ?? null,
    off: b.off,
    note: b.note ?? "",
  });
}

/**
 * A condition in words, for a row that is not being edited.
 *
 * Labelled positions are spelled out, because `is exactly 1` says nothing and
 * `is exactly 1, MVR` says the whole thing. The A-10C lever ordering is the
 * reverse of what the DCS-BIOS description implies, which is exactly the sort
 * of detail nobody should have to hold in their head while reading a profile.
 */
function describeTest(w: OnWhen, signal: SignalView | undefined): string {
  const label = (n: number): string => {
    const hit = signal?.values.find((v) => v.value === n);
    return hit ? `${n}, ${hit.label}` : String(n);
  };
  if ("equals" in w) return `is exactly ${label(w.equals)}`;
  if ("in" in w) return `is one of ${w.in.map(label).join("; ")}`;
  if ("gte" in w) return `is at least ${w.gte}`;
  if ("lte" in w) return `is at most ${w.lte}`;
  if ("between" in w) return `is between ${w.between[0]} and ${w.between[1]}`;
  return `follows it, scaled from ${w.scale[0]} to ${w.scale[1]}`;
}

/**
 * The editable cell for one lamp: every condition, and a control to add one.
 *
 * Conditions read as a sentence until the pencil is clicked. A profile is read
 * far more often than it is changed, and a wall of dropdowns is harder to check
 * at a glance than a line of text. Editing is deliberate, and the delete button
 * exists only while a row is open, so nothing is one stray click from gone.
 *
 * Rebuilt wholesale on each change rather than patched. A binding has a handful
 * of conditions, so the cost is nothing, and it removes a class of bug where
 * the controls and the data drift apart.
 */
/**
 * The groups of conditions in force, as one list either way.
 *
 * A binding is either a single AND group in `conditions` or a list of
 * alternatives in `any_of`, never both. Normalising here means the rest of the
 * editor has one shape to draw, and the arrays are shared rather than copied,
 * so editing a group edits the binding.
 */
function groupsOf(binding: Binding): Branch[] {
  if (binding.any_of && binding.any_of.length > 0) return binding.any_of;
  return [{ conditions: binding.conditions }];
}

/**
 * The editable cell for one lamp: every condition, and the controls to add one.
 *
 * Conditions read as a sentence until the pencil is clicked. A profile is read
 * far more often than it is changed, and a wall of dropdowns is harder to check
 * at a glance than a line of text. Editing is deliberate, and the delete button
 * exists only while a row is open, so nothing is one stray click from gone.
 *
 * Rebuilt wholesale on each change rather than patched. A binding has a handful
 * of conditions, so the cost is nothing, and it removes a class of bug where
 * the controls and the data drift apart.
 */
export function bindingEditor(opts: BindingEditorOptions): HTMLElement {
  const { binding, led, signals } = opts;
  const byId = new Map(signals.map((s) => [s.id, s]));
  const host = el("div", { class: "binding" });

  // Keyed on the condition itself rather than its index, so deleting one does
  // not silently open whichever row slid up into its place.
  //
  // The value is how the condition looked when editing began, which is what
  // cancel restores. `null` means it did not exist before, so cancelling a
  // freshly added condition removes it rather than restoring an empty row.
  const editing = new Map<Condition, Condition | null>();

  function changed(): void {
    opts.onChange();
    render();
  }

  /**
   * Drop empty alternatives, and collapse a single survivor back to a plain
   * condition list.
   *
   * `any_of` with one branch means exactly what `conditions` means, and leaving
   * the file in that state would ship two spellings of one thing.
   */
  function normalise(): void {
    const branches = binding.any_of ?? [];
    for (let i = branches.length - 1; i >= 0; i -= 1) {
      if ((branches[i]?.conditions.length ?? 0) === 0) branches.splice(i, 1);
    }
    if (branches.length === 1) {
      binding.conditions = branches[0]?.conditions ?? [];
      binding.any_of = [];
    }
  }

  function iconButton(
    cls: string,
    glyph: string,
    title: string,
    onClick: () => void,
  ): HTMLButtonElement {
    const button = el(
      "button",
      { class: `icon ${cls}`, type: "button", title, "aria-label": title },
      glyph,
    );
    button.addEventListener("click", onClick);
    return button;
  }

  function viewRow(condition: Condition, signal: SignalView | undefined): HTMLElement {
    const text = el("div", { class: "grow" });
    if (signal) {
      text.append(
        el("span", { class: "desc" }, signal.description),
        el(
          "span",
          { class: "sub" },
          el("code", {}, condition.source),
          el("span", { class: "test" }, describeTest(condition.on_when, signal)),
        ),
      );
    } else {
      // A signal this module does not have is called out rather than quietly
      // rendered, because the lamp it drives will never light.
      text.append(
        el("span", { class: "bad" }, `${condition.source} is not a signal in this module`),
        el(
          "span",
          { class: "sub" },
          el("span", { class: "test" }, describeTest(condition.on_when, undefined)),
        ),
      );
    }

    const row = el("div", { class: "condition-view" }, text);
    if (signal) row.append(hintFor(signal));
    row.append(
      iconButton("pencil", "\u270E", "Edit this condition", () => {
        editing.set(condition, structuredClone(condition));
        render();
      }),
    );
    return row;
  }

  function editRow(
    group: Branch,
    condition: Condition,
    signal: SignalView | undefined,
  ): HTMLElement {
    const select = el("select", { class: "test" });
    for (const kind of Object.keys(TEST_LABELS) as TestKind[]) {
      select.append(el("option", { value: kind }, TEST_LABELS[kind]));
    }
    select.value = testKind(condition.on_when);
    select.addEventListener("change", () => {
      condition.on_when = convert(condition.on_when, select.value as TestKind, signal);
      changed();
    });

    return el(
      "div",
      { class: "condition-edit" },
      signalPicker({
        signals,
        value: condition.source,
        onPick: (id) => {
          const wasUnset = condition.source === "";
          condition.source = id;
          // Only fill in a test for a condition that never had one chosen, so
          // swapping the signal under a tuned window does not discard it.
          if (wasUnset) condition.on_when = defaultTest(byId.get(id), led);
          changed();
        },
      }),
      el("div", { class: "test-row" }, select, valueControls(condition, signal, changed)),
      el(
        "div",
        { class: "row-actions" },
        iconButton("done", "\u2713", "Keep these changes", () => {
          editing.delete(condition);
          render();
        }),
        iconButton("cancel", "\u2715", "Discard changes to this condition", () => {
          const before = editing.get(condition);
          editing.delete(condition);
          if (before === null || before === undefined) {
            // It was added during this edit, so undoing that means removing it.
            const at = group.conditions.indexOf(condition);
            if (at >= 0) group.conditions.splice(at, 1);
            normalise();
          } else {
            // Restored in place, so the list keeps its order and the object
            // identity the editing map is keyed on stays valid.
            condition.source = before.source;
            condition.on_when = structuredClone(before.on_when);
          }
          changed();
        }),
        iconButton("trash", "\u{1F5D1}", "Delete this condition", () => {
          // Asked for the same way the profile-level Reset asks, and for the
          // same reason: it is a small red button next to two harmless ones,
          // and a misclick would otherwise be silent and unrecoverable once
          // the profile is saved.
          const what = condition.source
            ? `${condition.source} ${describeTest(condition.on_when, signal)}`
            : "this unfinished condition";
          const groups = groupsOf(binding);
          const last = groups.length === 1 && group.conditions.length === 1;
          const consequence = last
            ? `\n\n${led.name} will be left unassigned, which means it is driven off.`
            : `\n\n${led.name} will still need its other condition(s) to light.`;
          if (!window.confirm(`Delete this condition?\n\n${what}${consequence}`)) return;

          const at = group.conditions.indexOf(condition);
          if (at >= 0) group.conditions.splice(at, 1);
          editing.delete(condition);
          normalise();
          changed();
        }),
      ),
    );
  }

  /**
   * The lamp is simply on, with no signal behind it.
   *
   * Shown as its own row rather than as a condition, because it is the absence
   * of one. Brightness still comes from the Output column on a lamp that dims,
   * so "always on" and "always on at 40" are the same control.
   */
  function alwaysRow(): HTMLElement {
    const text = el(
      "div",
      { class: "grow" },
      el("span", { class: "desc" }, "Always on"),
      el(
        "span",
        { class: "sub" },
        el("span", { class: "test" }, "lit whenever this aircraft is loaded, reading no signal"),
      ),
    );
    const row = el("div", { class: "condition-view always" }, text);
    row.append(
      iconButton("cancel", "\u2715", "Stop driving this lamp", () => {
        binding.always = false;
        changed();
      }),
    );
    return row;
  }

  /**
   * This lamp follows another one.
   *
   * Shown as a row of its own rather than as a condition, because it reads no
   * signal: it takes whatever the other lamp resolved to.
   */
  function mirrorRow(target: string): HTMLElement {
    const known = opts.siblings.some((l) => l.name === target);
    const text = el("div", { class: "grow" });
    if (known) {
      text.append(
        el("span", { class: "desc" }, `Matches ${target}`),
        el(
          "span",
          { class: "sub" },
          el(
            "span",
            { class: "test" },
            binding.off > 0
              ? `follows it, but sits at ${binding.off} when it is off`
              : "follows it exactly",
          ),
        ),
      );
    } else {
      text.append(
        el("span", { class: "bad" }, `${target} is not a lamp that dims on this device`),
      );
    }

    const row = el("div", { class: "condition-view always" }, text);
    if (opts.siblings.length > 1) {
      const pick = el("select", { class: "test" });
      for (const l of opts.siblings) {
        pick.append(el("option", { value: l.name }, l.label || l.name));
      }
      pick.value = target;
      pick.addEventListener("change", () => {
        binding.same_as = pick.value;
        changed();
      });
      row.append(pick);
    }
    row.append(
      iconButton("cancel", "\u2715", "Stop matching another lamp", () => {
        binding.same_as = null;
        changed();
      }),
    );
    return row;
  }

  function addCondition(group: Branch): void {
    const fresh: Condition = { source: "", on_when: defaultTest(undefined, led) };
    group.conditions.push(fresh);
    editing.set(fresh, null);
    changed();
  }

  /**
   * Start a second, independent way for the lamp to light.
   *
   * The first alternative has to move the existing conditions into `any_of`,
   * because a binding carries one form or the other and never both.
   */
  function addAlternative(): void {
    const fresh: Condition = { source: "", on_when: defaultTest(undefined, led) };
    if (!binding.any_of || binding.any_of.length === 0) {
      binding.any_of = [{ conditions: binding.conditions }, { conditions: [fresh] }];
      binding.conditions = [];
    } else {
      binding.any_of.push({ conditions: [fresh] });
    }
    editing.set(fresh, null);
    changed();
  }

  function groupBlock(group: Branch, alternatives: boolean): HTMLElement {
    const block = el("div", { class: alternatives ? "group alt" : "group" });
    for (const [i, condition] of group.conditions.entries()) {
      const signal = byId.get(condition.source);
      // A condition with no signal yet has nothing to read, so it opens itself.
      const open = editing.has(condition) || condition.source === "";

      // "and" between conditions, because that is exactly what they mean and a
      // stacked list without it reads like a choice between them.
      if (i > 0) block.append(el("div", { class: "and" }, "and"));
      block.append(open ? editRow(group, condition, signal) : viewRow(condition, signal));
    }
    const add = el("button", { class: "add", type: "button" }, "+ Add condition");
    add.title = "Another test that must also hold for this alternative to light the lamp.";
    add.addEventListener("click", () => addCondition(group));
    block.append(add);
    return block;
  }

  function render(): void {
    host.replaceChildren();

    // Always-on and conditions are mutually exclusive: one reads signals and
    // the other deliberately reads none, so showing both would be a lie about
    // which one is in force.
    if (binding.always) {
      host.append(alwaysRow());
      const swap = el("button", { class: "add", type: "button" }, "Use a signal instead");
      swap.addEventListener("click", () => {
        binding.always = false;
        addCondition({ conditions: binding.conditions });
      });
      host.append(swap);
      return;
    }

    if (binding.same_as) {
      host.append(mirrorRow(binding.same_as));
      appendRevert();
      return;
    }

    const groups = groupsOf(binding);
    const alternatives = groups.length > 1;

    if (groups.every((g) => g.conditions.length === 0)) {
      // An undecided lamp gets the two ways to start. Always-on is not offered
      // on a configured lamp, where choosing it would discard existing work.
      const assign = el("button", { class: "add", type: "button" }, "Assign a signal");
      const first = groups[0] ?? { conditions: binding.conditions };
      assign.addEventListener("click", () => addCondition(first));
      const on = el("button", { class: "add", type: "button" }, "Always on");
      on.title = "Light this lamp whenever the aircraft is loaded, with no signal behind it.";
      on.addEventListener("click", () => {
        binding.always = true;
        changed();
      });
      const choices = el("div", { class: "choices" }, assign, on);

      // Only between lamps that dim, and only where there is another one to
      // point at. On a panel with a single dimmer the option would be dead.
      if (led.dimmable && opts.siblings.length > 0) {
        const match = el("button", { class: "add", type: "button" }, "Match another lamp");
        match.title = "Follow another dimmer on this device, so both move together.";
        match.addEventListener("click", () => {
          binding.same_as = opts.siblings[0]?.name ?? null;
          changed();
        });
        choices.append(match);
      }

      host.append(choices);
      appendRevert();
      return;
    }

    for (const [i, group] of groups.entries()) {
      // "or" between alternatives: any one of them lighting the lamp is enough.
      if (i > 0) host.append(el("div", { class: "or" }, "or"));
      host.append(groupBlock(group, alternatives));
    }

    const alt = el("button", { class: "add", type: "button" }, "+ Add alternative (or)");
    alt.title = "Another way for this lamp to light, independent of the conditions above.";
    alt.addEventListener("click", addAlternative);
    host.append(alt);

    if (alternatives) {
      host.append(
        el(
          "div",
          { class: "meta" },
          "Any one alternative lights the lamp. Within an alternative, every condition must hold.",
        ),
      );
    } else if ((groups[0]?.conditions.length ?? 0) > 1) {
      host.append(el("div", { class: "meta" }, "The lamp lights only when every condition holds."));
    }

    appendRevert();
  }

  /**
   * Offered only where this lamp actually differs from the shipped profile, so
   * the control is never a no-op and its presence means something.
   */
  function appendRevert(): void {
    const shipped = opts.shipped;
    if (!shipped || meaningfulPart(shipped) === meaningfulPart(binding)) return;
    const revert = el("button", { class: "add revert", type: "button" }, "Reset this lamp");
    revert.title = "Put this lamp back the way it shipped. No other lamp is touched.";
    revert.addEventListener("click", () => {
      binding.conditions = structuredClone(shipped.conditions);
      binding.any_of = structuredClone(shipped.any_of ?? []);
      binding.always = shipped.always ?? false;
      binding.same_as = shipped.same_as ?? null;
      binding.on = shipped.on;
      binding.off = shipped.off;
      binding.note = shipped.note;
      editing.clear();
      changed();
    });
    host.append(revert);
  }

  render();
  return host;
}
