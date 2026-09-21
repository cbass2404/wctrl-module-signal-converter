// Editing what drives one lamp.
//
// A binding carries a *list* of conditions and every one of them must hold. The
// single-condition case is the common one but not the shape: the A-10C
// half-flaps lamp needs the lever at MVR and the gauge inside the half window,
// because the flaps travel through that window on the way to DN and the lamp
// would otherwise flash on the way past.

import { confirmAction } from "./confirm";
import { flagSlot } from "./flags";
import { noteEditor } from "./note";
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

/**
 * A one-glyph button: `pencil` opens a thing for editing, `done` keeps the
 * edit and `cancel` puts it back. Shared with the profile header's rename.
 */
export function iconButton(
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

/** A lamp a mirror can point at, on this device or another. */
export interface MirrorTarget {
  device: string;
  deviceName: string;
  led: Led;
}

export interface BindingEditorOptions {
  binding: Binding;
  led: Led;
  signals: SignalView[];
  /**
   * The lamps this one could match, this device's first. Filtered to dimmers
   * by the caller, since only they can mirror or be mirrored. Asked for on
   * each draw, because matching one lamp takes it out of every other list.
   */
  targets: () => MirrorTarget[];
  /** A device's name as the window shows it, for a mirror on another panel. */
  deviceName: (key: string) => string;
  /**
   * This lamp as the shipped profile has it, when there is one. Drives the
   * per-lamp revert, so one lamp can be put back without discarding every other
   * edit in the profile the way the profile-level Reset does.
   */
  shipped?: Binding | undefined;
  /** Called whenever the binding changes, so the window can mark itself dirty. */
  onChange: () => void;
  /**
   * Called when a change is settled: a condition kept with its tick or
   * deleted, or a form chosen that has no edit step (always on, matching
   * another lamp, a revert). Not called while a condition is being edited, nor
   * on cancel, which only returns to what was already counted.
   */
  onCommit: () => void;
}

/** `extra` appended inside `field`, which is returned. */
function under(field: HTMLElement, extra: HTMLElement): HTMLElement {
  field.append(extra);
  return field;
}

/** What a revert compares and copies. The device and lamp never change. */
function meaningfulPart(b: Binding): string {
  return JSON.stringify({
    conditions: b.conditions,
    any_of: b.any_of ?? [],
    pick: b.pick ?? "brightest",
    always: b.always ?? false,
    same_as: b.same_as ?? null,
    same_as_device: b.same_as_device ?? null,
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
 * A whole lamp in words, a line per condition, for a question that has to say
 * what a change will do before it is made rather than leave the user to find
 * out after.
 */
function describeBinding(
  b: Binding,
  byId: Map<string, SignalView>,
  deviceName: (key: string) => string,
): string {
  const lines: string[] = [];
  const groups = groupsOf(b).filter((g) => g.conditions.length > 0);
  const condition = (c: Condition): string =>
    c.source ? `${c.source} ${describeTest(c.on_when, byId.get(c.source))}` : "an unfinished condition";
  if (b.always) lines.push("Always on, reading no signal");
  else if (b.same_as) {
    const elsewhere = b.same_as_device && b.same_as_device !== b.device;
    lines.push(`Matches ${b.same_as}${elsewhere ? ` on ${deviceName(b.same_as_device!)}` : ""}`);
  }
  else if (groups.length === 0) lines.push("Unassigned, so driven off");
  else if (groups.length === 1) lines.push(groups[0]!.conditions.map(condition).join("\nand "));
  else {
    const how = b.pick === "latest" ? "the one that changed last wins" : "the brightest wins";
    lines.push(`Any of these, ${how}:`);
    for (const g of groups) lines.push(`- ${g.conditions.map(condition).join(" and ")}`);
  }
  if (b.on !== null) lines.push(`Lit at ${b.on}`);
  if (b.off !== 0) lines.push(`Off at ${b.off}`);
  if (b.note) lines.push(`Note: ${b.note}`);
  return lines.join("\n");
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

  function committed(): void {
    opts.onCommit();
    changed();
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
    // Choosing between alternatives means nothing without two of them, and
    // the profile check rejects it.
    if (branches.length < 2) delete binding.pick;
  }

  function viewRow(condition: Condition, signal: SignalView | undefined): HTMLElement {
    const text = el("div", { class: "grow" });
    if (signal) {
      text.append(
        el("div", { class: "named desc" }, signal.description, hintFor(signal)),
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

    // Under the signal, so the reason sits with what it is about.
    text.append(flagSlot(condition));
    const row = el("div", { class: "condition-view" }, text);
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
      // The flag goes inside the picker's column. As a sibling it would be
      // another item in this flex row and squeeze the picker to nothing.
      under(
        signalPicker({
          // Text signals are hidden here. A condition compares a number, so one
          // that reports characters could never satisfy it, and offering it would
          // be offering a choice that silently never lights the lamp. They belong
          // to display fields, which is where they are offered.
          signals: signals.filter((s) => !s.text),
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
        flagSlot(condition),
      ),
      el("div", { class: "test-row" }, select, valueControls(condition, signal, changed)),
      el(
        "div",
        { class: "row-actions" },
        iconButton("done", "\u2713", "Keep these changes", () => {
          editing.delete(condition);
          opts.onCommit();
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
          void confirmAction(`Delete this condition?\n\n${what}${consequence}`, "Delete").then((ok) => {
            if (!ok) return;
            const at = group.conditions.indexOf(condition);
            if (at >= 0) group.conditions.splice(at, 1);
            editing.delete(condition);
            normalise();
            committed();
          });
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
        committed();
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
    const on = binding.same_as_device ?? binding.device;
    const targets = opts.targets();
    const known = targets.some((t) => t.device === on && t.led.name === target);
    const where = on === binding.device ? "" : ` on ${opts.deviceName(on)}`;
    const text = el("div", { class: "grow" });
    if (known) {
      text.append(
        el("span", { class: "desc" }, `Matches ${target}${where}`),
        el(
          "span",
          { class: "sub" },
          el(
            "span",
            { class: "test" },
            "follows it",
          ),
        ),
      );
    } else {
      text.append(
        el(
          "span",
          { class: "bad" },
          `${target} is not a lamp that dims on ${where ? opts.deviceName(on) : "this device"}`,
        ),
      );
    }

    const row = el("div", { class: "condition-view always" }, text);
    if (targets.length > 1) {
      // Grouped by panel, since the same lamp name turns up on several. The
      // value is the position in the list: a lamp name can hold a slash.
      const pick = el("select", { class: "test" });
      let group: HTMLElement | null = null;
      for (const [i, t] of targets.entries()) {
        if (!group || group.getAttribute("label") !== t.deviceName) {
          group = el("optgroup", { label: t.deviceName });
          pick.append(group);
        }
        group.append(el("option", { value: String(i) }, t.led.label || t.led.name));
      }
      pick.value = String(targets.findIndex((t) => t.device === on && t.led.name === target));
      pick.addEventListener("change", () => {
        const chosen = targets[Number(pick.value)];
        if (!chosen) return;
        pointAt(chosen);
        committed();
      });
      row.append(pick);
    }
    row.append(
      iconButton("cancel", "\u2715", "Stop matching another lamp", () => {
        binding.same_as = null;
        delete binding.same_as_device;
        committed();
      }),
    );
    return row;
  }

  /** The device is written only when it is another one, as the file has it. */
  function pointAt(t: MirrorTarget): void {
    binding.same_as = t.led.name;
    if (t.device === binding.device) delete binding.same_as_device;
    else binding.same_as_device = t.device;
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
        opts.onCommit();
        addCondition({ conditions: binding.conditions });
      });
      host.append(swap);
      appendFooter();
      return;
    }

    if (binding.same_as) {
      host.append(mirrorRow(binding.same_as));
      appendFooter();
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
        committed();
      });
      const choices = el("div", { class: "choices" }, assign, on);

      // Only between lamps that dim, and only where there is another one to
      // point at, on this panel or any other.
      if (opts.targets().length > 0) {
        const match = el("button", { class: "add", type: "button" }, "Match another lamp");
        match.title = "Follow another dimmer, on this panel or another, so both move together.";
        match.addEventListener("click", () => {
          // Asked again: another lamp may have started following this one
          // since the button was drawn, and matching now would be a loop.
          const first = opts.targets()[0];
          if (first) pointAt(first);
          committed();
        });
        choices.append(match);
      }

      host.append(choices);
      appendFooter();
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
      // Brightest is OR for on/off tests. Latest is for a lamp that follows one
      // of several knobs with nothing saying which is in use: two seats with a
      // lighting knob each, where brightest would mean turning both down.
      const pick = el("select", { class: "test" });
      pick.append(
        el("option", { value: "brightest" }, "the brightest one"),
        el("option", { value: "latest" }, "the one whose signal moved last"),
      );
      pick.value = binding.pick ?? "brightest";
      pick.title =
        "The one whose signal moved last suits a knob per seat with no signal for which seat is in use: the knob turned last drives the lamp. Until one moves, the brightest does.";
      pick.addEventListener("change", () => {
        if (pick.value === "latest") binding.pick = "latest";
        else delete binding.pick;
        changed();
      });
      host.append(
        el("div", { class: "meta" }, "When more than one alternative could light the lamp, use ", pick, "."),
        el("div", { class: "meta" }, "Within an alternative, every condition must hold."),
      );
    } else if ((groups[0]?.conditions.length ?? 0) > 1) {
      host.append(el("div", { class: "meta" }, "The lamp lights only when every condition holds."));
    }

    appendFooter();
  }

  /**
   * What sits under every lamp whatever form its binding takes.
   *
   * The note belongs on all of them, the unassigned ones most of all: six
   * lamps in the A-10C default are left unassigned with the reasoning in a
   * note, and a lamp with no conditions is exactly the one a reader has the
   * most questions about.
   */
  function appendFooter(): void {
    // The note on the left, Reset alone in the corner. Reset is the only
    // control in this cell that throws work away, and it was sitting at the
    // bottom of the same stack as the buttons that add things, one slip from
    // the last of them.
    const footer = el("div", { class: "binding-footer" });
    footer.append(noteEditor(binding, "lamp", opts.onChange));
    appendRevert(footer);
    host.append(footer);
  }

  /**
   * On every lamp that has a shipped version, so the way back is always in
   * the same place. Disabled while the lamp already matches, so it is never a
   * no-op. A profile the user made has no shipped version and no button.
   */
  function appendRevert(into: HTMLElement): void {
    const shipped = opts.shipped;
    if (!shipped) return;
    const revert = el("button", { class: "add revert", type: "button" }, "Reset this lamp");
    if (meaningfulPart(shipped) === meaningfulPart(binding)) {
      revert.disabled = true;
      revert.title = "This lamp matches how it shipped.";
    } else {
      revert.title = "Put this lamp back the way it shipped. No other lamp is touched.";
      revert.addEventListener("click", () => void confirmRevert(shipped));
    }
    into.append(revert);
  }

  /** Asks first, showing both setups, so a revert is never made blind. */
  async function confirmRevert(shipped: Binding): Promise<void> {
    const question =
      `Reset ${led.name} to how it shipped?\n\n` +
      `Now:\n${describeBinding(binding, byId, opts.deviceName)}\n\n` +
      `Shipped:\n${describeBinding(shipped, byId, opts.deviceName)}\n\n` +
      "No other lamp is touched.";
    if (!(await confirmAction(question, "Reset"))) return;
    binding.conditions = structuredClone(shipped.conditions);
    binding.any_of = structuredClone(shipped.any_of ?? []);
    if (shipped.pick) binding.pick = shipped.pick;
    else delete binding.pick;
    binding.always = shipped.always ?? false;
    binding.same_as = shipped.same_as ?? null;
    if (shipped.same_as_device) binding.same_as_device = shipped.same_as_device;
    else delete binding.same_as_device;
    binding.on = shipped.on;
    binding.off = shipped.off;
    binding.note = shipped.note;
    editing.clear();
    committed();
  }

  render();
  return host;
}
