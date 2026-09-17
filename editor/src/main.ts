// The window. Two screens: the profile library, and one profile being edited.
//
// No framework. The editor is a fixed list of lamp rows with a search box on
// each, which is not enough state to be worth a runtime, and a smaller install
// matters for something that ships next to a daemon.

import { displaySection } from "./readout";
import {
  createProfile,
  defaultProfile,
  listDevices,
  listModules,
  listProfiles,
  listSignals,
  openProfile,
  resetProfile,
  saveProfile,
} from "./api";
import { bindingEditor } from "./binding";
import { infoIcon } from "./typeahead";
import type { Binding, Device, Led, ModuleChoice, Profile, ProfileSummary, SignalView } from "./types";

const app = document.getElementById("app") as HTMLElement;

/** Loaded once: the hardware inventory does not change while the window is open. */
let devices: Device[] = [];
let modules: ModuleChoice[] = [];

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

function clear(): void {
  app.replaceChildren();
}

/** Failures are shown in the window, never swallowed and never only in a console. */
function showError(where: string, e: unknown): void {
  const message = e instanceof Error ? e.message : String(e);
  app.prepend(el("div", { class: "error" }, `${where}: ${message}`));
}

// ------------------------------------------------------------------- library

async function showLibrary(): Promise<void> {
  clear();
  const header = el(
    "header",
    {},
    el("h1", {}, "Profiles"),
    el("div", { class: "spacer" }),
    el("button", { class: "primary", id: "new" }, "New profile"),
  );
  app.append(header);

  let rows: ProfileSummary[];
  try {
    rows = await listProfiles();
  } catch (e) {
    showError("Loading profiles", e);
    return;
  }

  header.querySelector("#new")?.addEventListener("click", () => void showNewProfile());

  if (rows.length === 0) {
    app.append(
      el(
        "p",
        { class: "empty" },
        "No profiles yet. Create one and every lamp on your panels will be listed, ready to assign.",
      ),
    );
    return;
  }

  const list = el("ul", { class: "profiles" });
  for (const row of rows) {
    const meta = row.error
      ? el("span", { class: "bad" }, row.error)
      : el(
          "span",
          { class: "meta" },
          `${row.module} · ${row.aircraft.join(", ")} · ${row.bound} of ${row.total} lamps assigned`,
        );

    const actions = el("div", { class: "actions" });
    if (!row.error) {
      const edit = el("button", {}, "Edit");
      edit.addEventListener("click", () => void showProfile(row.file));
      actions.append(edit);
    }
    if (row.has_default) {
      const reset = el("button", { class: "danger" }, "Reset");
      reset.addEventListener("click", () => void resetOne(row));
      actions.append(reset);
    }

    list.append(
      el("li", {}, el("div", { class: "grow" }, el("strong", {}, row.name), el("br"), meta), actions),
    );
  }
  app.append(list);
}

/** Reset discards the user's work, so it asks first and says exactly what it does. */
async function resetOne(row: ProfileSummary): Promise<void> {
  const ok = window.confirm(
    `Replace ${row.name} with the profile that shipped with wctrl?\n\n` +
      `Any changes you have made to it will be lost. Profiles you created yourself are not affected.`,
  );
  if (!ok) return;
  try {
    await resetProfile(row.file);
    await showLibrary();
  } catch (e) {
    showError("Resetting the profile", e);
  }
}

/**
 * New profiles pick a module from a list, never a typed name.
 *
 * The list is the catalogue built from the user's own DCS-BIOS, so it cannot
 * offer a module they do not have, and the aircraft names come with it because
 * one module often serves several and the mapping is not guessable.
 */
async function showNewProfile(): Promise<void> {
  if (modules.length === 0) {
    showError(
      "No modules available",
      new Error("The signal catalogue is empty. Build it from your DCS-BIOS install first."),
    );
    return;
  }

  const select = el("select", { id: "module", size: "12" });
  for (const m of modules) {
    const names = m.aircraft.length > 0 ? m.aircraft.join(", ") : "no runtime aircraft name";
    select.append(el("option", { value: m.key }, `${m.key}  ·  ${names}`));
  }

  const dialog = el(
    "dialog",
    { class: "picker" },
    el("h2", {}, "Which module?"),
    el("p", { class: "meta" }, `${modules.length} modules in your DCS-BIOS install.`),
    select,
    el(
      "div",
      { class: "actions" },
      el("button", { id: "cancel" }, "Cancel"),
      el("button", { class: "primary", id: "make" }, "Create"),
    ),
  );
  app.append(dialog);
  dialog.showModal();

  dialog.querySelector("#cancel")?.addEventListener("click", () => dialog.close());
  dialog.querySelector("#make")?.addEventListener("click", () => {
    const key = select.value;
    dialog.close();
    void (async () => {
      try {
        const file = await createProfile(key);
        await showProfile(file);
      } catch (e) {
        showError("Creating the profile", e);
      }
    })();
  });
}

// -------------------------------------------------------------------- editor

/** State for the profile currently open. */
interface Session {
  file: string;
  profile: Profile;
  signals: SignalView[];
  /** This lamp as it shipped, keyed device and lamp. Empty for a profile the
   * user created, which has no shipped version to revert to. */
  shipped: Map<string, Binding>;
  /**
   * The profile as it stood when the rows were built, serialised.
   *
   * Compared against rather than a flag being set, so undoing an edit clears
   * the unsaved marker instead of leaving it stuck on. A flag cannot tell the
   * difference between "changed" and "changed back".
   */
  baseline: string;
  dirty: boolean;
  refreshDirty: () => void;
}

/**
 * Key for looking a lamp up across the profile, the inventory and the
 * shipped copy. A NUL cannot occur in a device key or a lamp name, so the
 * two parts can never run together into a colliding key.
 */
const lampKey = (device: string, led: string): string => `${device}\u0000${led}`;

async function showProfile(file: string): Promise<void> {
  let profile: Profile;
  try {
    profile = await openProfile(file);
  } catch (e) {
    showError("Opening the profile", e);
    return;
  }

  // Only this profile's module is loaded, never the whole catalogue.
  let signals: SignalView[] = [];
  let signalError: string | null = null;
  try {
    signals = await listSignals(profile.module);
  } catch (e) {
    signalError = e instanceof Error ? e.message : String(e);
  }

  // The shipped copy, so one lamp can be reverted without resetting the file.
  // A profile the user made has none, which is not an error.
  const shipped = new Map<string, Binding>();
  try {
    const original = await defaultProfile(file);
    for (const b of original?.bindings ?? []) {
      shipped.set(lampKey(b.device, b.led), b);
    }
  } catch {
    // A missing or unreadable default only costs the revert button.
  }

  clear();

  const save = el("button", { class: "primary" }, "Save");
  const state = el("span", { class: "meta" }, "");
  const session: Session = {
    file,
    profile,
    signals,
    shipped,
    baseline: "",
    dirty: false,
    refreshDirty: () => {
      session.dirty = JSON.stringify(session.profile) !== session.baseline;
      state.textContent = session.dirty ? "unsaved changes" : "";
      if (session.dirty) save.removeAttribute("disabled");
      else save.setAttribute("disabled", "");
    },
  };
  save.setAttribute("disabled", "");

  const back = el("button", {}, "← Profiles");
  back.addEventListener("click", () => {
    if (session.dirty && !window.confirm("Leave without saving? Your changes will be lost.")) return;
    void showLibrary();
  });

  save.addEventListener("click", () => {
    void (async () => {
      try {
        await saveProfile(session.file, session.profile);
        session.baseline = JSON.stringify(session.profile);
        session.dirty = false;
        state.textContent = "saved";
        save.setAttribute("disabled", "");
      } catch (e) {
        showError("Saving the profile", e);
      }
    })();
  });

  const toggle = el("button", {}, "Expand all");
  app.append(
    el(
      "header",
      {},
      back,
      el(
        "div",
        { class: "grow" },
        el("h1", {}, profile.name),
        el("span", { class: "meta block" }, `${profile.module} · ${profile.aircraft.join(", ")}`),
      ),
      state,
      toggle,
      save,
    ),
  );

  if (signalError) {
    app.append(
      el(
        "div",
        { class: "error" },
        `No signals for ${profile.module}: ${signalError}. Lamps can be viewed but not assigned.`,
      ),
    );
  }

  // Bindings are keyed by device and lamp so a lamp with no row in the file
  // still appears. The lamp inventory is the list, not the profile's contents.
  const byLamp = new Map<string, Binding>();
  for (const b of profile.bindings) {
    byLamp.set(lampKey(b.device, b.led), b);
  }

  const sections = devices.map((device) => deviceSection(device, byLamp, session));
  app.append(...sections);

  // Taken after the rows are built, because building them adds a binding for
  // any lamp the file did not already list. Measuring before that would have
  // every profile arrive already dirty.
  session.baseline = JSON.stringify(session.profile);

  // Collapsed on open, so a profile with several panels does not arrive as a
  // wall of lamps. One control opens and closes all of them.
  //
  // The label tracks the sections rather than the last click, so opening every
  // section by hand leaves a button offering to collapse them. A mixed state
  // has no right answer, so the label is left alone: whichever way it reads,
  // one click still reaches a definite state.
  // What the button currently offers, held explicitly rather than read back
  // from the sections. A mixed state deliberately leaves the label alone, so
  // the sections cannot say which action the label is promising.
  let offersCollapse = false;
  const setLabel = (collapse: boolean): void => {
    offersCollapse = collapse;
    toggle.textContent = collapse ? "Collapse all" : "Expand all";
  };
  const syncToggle = (): void => {
    if (sections.every((s) => s.open)) setLabel(true);
    else if (sections.every((s) => !s.open)) setLabel(false);
  };
  for (const s of sections) s.addEventListener("toggle", syncToggle);
  syncToggle();

  toggle.addEventListener("click", () => {
    // Does what the label says. Deciding from the sections instead would make a
    // button reading "Collapse all" expand everything as soon as one section
    // had been closed by hand.
    const expand = !offersCollapse;
    for (const section of sections) section.open = expand;
    syncToggle();
  });
}

/**
 * A lamp missing from the profile gets a row added to it on first use, so a
 * profile written against fewer devices still edits cleanly.
 */
function bindingFor(device: Device, led: Led, byLamp: Map<string, Binding>, session: Session): Binding {
  const key = lampKey(device.key, led.name);
  const existing = byLamp.get(key);
  if (existing) return existing;
  const fresh: Binding = { device: device.key, led: led.name, conditions: [], on: null, off: 0 };
  byLamp.set(key, fresh);
  session.profile.bindings.push(fresh);
  return fresh;
}

function deviceSection(
  device: Device,
  byLamp: Map<string, Binding>,
  session: Session,
): HTMLDetailsElement {
  const count = el("span", { class: "meta" }, "");
  const refreshCount = (): void => {
    const assigned = device.leds.filter(
      (l) => (byLamp.get(lampKey(device.key, l.name))?.conditions.length ?? 0) > 0,
    ).length;
    count.textContent = `${assigned} of ${device.leds.length} lamps assigned`;
  };

  const rows = el("tbody");
  for (const led of device.leds) {
    rows.append(lampRow(device, led, byLamp, session, refreshCount));
  }
  refreshCount();

  const table = el(
    "table",
    { class: "lamps" },
    el(
      "thead",
      {},
      el(
        "tr",
        {},
        el("th", {}, "Lamp"),
        el("th", {}, "Driven by"),
        el("th", { class: "num" }, "Output"),
      ),
    ),
    rows,
  );

  // Whether this profile drives the panel at all. Distinct from binding
  // nothing: an unbound panel is still swept, so it goes dark, which is what
  // you want for one you can see. Switching this off leaves it untouched,
  // which is what you want for a panel that is physically covered. A WinWing
  // ICP and UFC share a swing arm, and whichever is in use hides the other.
  const disabled = session.profile.disabled_devices ?? [];
  const drive = el("input", { type: "checkbox" }) as HTMLInputElement;
  drive.checked = !disabled.includes(device.key);
  const section = el("details", { class: "device" });
  const applyDriveState = (): void => {
    section.classList.toggle("off", !drive.checked);
  };
  drive.addEventListener("click", (e) => e.stopPropagation());
  drive.addEventListener("change", () => {
    const list = session.profile.disabled_devices ?? [];
    const at = list.indexOf(device.key);
    if (drive.checked) {
      if (at >= 0) list.splice(at, 1);
    } else if (at < 0) {
      list.push(device.key);
    }
    if (list.length > 0) session.profile.disabled_devices = list;
    else delete session.profile.disabled_devices;
    applyDriveState();
    session.refreshDirty();
  });
  applyDriveState();

  if (!session.profile.readouts) session.profile.readouts = [];
  const glass = displaySection(
    device,
    session.profile.readouts,
    session.signals,
    session.refreshDirty,
  );

  section.append(
    el(
      "summary",
      {},
      el("span", { class: "name" }, device.display_name),
      count,
      el("label", { class: "drive meta" }, drive, " drive this panel"),
    ),
    table,
  );
  if (glass) section.append(glass);
  return section;
}

/**
 * The details behind a lamp, out of the way until asked for.
 *
 * Everything here is true of the hardware rather than of the binding, so it is
 * read once when someone is curious and never again. On screen permanently it
 * would crowd the conditions, which are the part being worked on.
 */
function lampHint(led: Led): HTMLElement {
  const lines: (Node | string)[] = [
    el("strong", {}, led.label || led.name),
    // The identifier, which is what --verbose prints when this lamp is written
    // and what learn mode will have to match against. Kept out of the row and
    // put here, where it is available without being in the way.
    el("code", { class: "hint-id block" }, led.name),
    el(
      "span",
      { class: "meta block" },
      led.dimmable ? `dimmer, 0 to ${led.max}` : `on or off, ${led.on_value} is on`,
    ),
    el("span", { class: "meta block" }, `part 0x${led.part_id.toString(16)}, index ${led.index}`),
  ];
  if (led.note) {
    lines.push(el("span", { class: "block" }, led.note));
  }
  if (!led.verified) {
    lines.push(
      el(
        "span",
        { class: "block" },
        "Not confirmed on hardware. If it will not light, the fault may be ours rather than yours.",
      ),
    );
  }
  return infoIcon("Lamp details", ...lines);
}

/**
 * Whether this binding's `on` value affects anything.
 *
 * `on` is what a satisfied test resolves to, and conditions combine by taking
 * the dimmest. A scale ignores `on` entirely and spreads its source across the
 * lamp's range, so a binding made only of scales never consults it. Mixed with
 * a threshold it does matter, because the threshold resolves to `on` and the
 * minimum makes it a ceiling on the scaled value.
 *
 * A mirror takes its value from the lamp it follows, so `on` is unused there
 * too; only `off` still applies.
 */
function onValueMatters(binding: Binding): boolean {
  if (binding.same_as) return false;
  if (binding.always) return true;
  const groups = binding.any_of?.length ? binding.any_of : [{ conditions: binding.conditions }];
  return groups.some((g) => g.conditions.some((c) => !("scale" in c.on_when)));
}

function lampRow(
  device: Device,
  led: Led,
  byLamp: Map<string, Binding>,
  session: Session,
  refreshCount: () => void,
): HTMLTableRowElement {
  const binding = bindingFor(device, led, byLamp, session);

  // The label, and only the label. It is what is printed on the panel the user
  // is looking at. The identifier behind it is a key in the profile file, which
  // is the editor's job to handle rather than the user's to read.
  //
  // Type sits here too rather than in a column of its own. It describes the
  // hardware, not the binding, and never changes, so a whole column of width
  // was being spent on something read once. The conditions need it far more.
  // The icon follows the name, everywhere in the window. The name is what is
  // being read; the icon only says that there is more if you want it.
  const name = el(
    "td",
    {},
    el("div", { class: "named" }, el("strong", {}, led.label || led.name), lampHint(led)),
    el("span", { class: "meta block kind" }, led.dimmable ? `dimmer 0..${led.max}` : "on / off"),
  );
  if (!led.verified) name.append(el("span", { class: "unverified" }, "unverified"));

  const output = el("td", { class: "num" });
  const renderOutput = (): void => {
    output.replaceChildren();

    // An indicator acks 255 and lights nothing, which is not "off" and cost a
    // long detour once. There is no brightness to offer, only its on value.
    if (!led.dimmable) {
      output.append(el("span", { class: "meta" }, String(led.on_value)));
      return;
    }
    if (binding.same_as) {
      output.append(el("span", { class: "meta" }, "matched"));
      return;
    }
    if (!onValueMatters(binding)) {
      // Either nothing is assigned yet, or every test is a scale. A scale
      // ignores `on` and spreads the source across the lamp's own range, so an
      // input here would be a control that quietly does nothing.
      output.append(
        el("span", { class: "meta" }, binding.conditions.length || binding.any_of?.length ? `0..${led.max}` : ""),
      );
      return;
    }

    const input = el("input", {
      type: "number",
      class: "value",
      min: "0",
      max: String(led.max),
      value: String(binding.on ?? led.on_value),
    });
    input.title = "Brightness when this lamp is lit.";
    input.addEventListener("change", () => {
      const n = Number(input.value);
      binding.on = Number.isFinite(n) ? Math.min(Math.max(n, 0), led.max) : led.on_value;
      input.value = String(binding.on);
      session.refreshDirty();
    });
    output.append(input);
  };
  renderOutput();

  const driven = el("td", {});
  driven.append(
    bindingEditor({
      binding,
      led,
      signals: session.signals,
      // Only dimmers, and never the lamp itself: an on/off lamp has no level to
      // follow, which is what the mirror copies.
      siblings: led.dimmable
        ? device.leds.filter((l) => l.dimmable && l.name !== led.name)
        : [],
      shipped: session.shipped.get(lampKey(device.key, led.name)),
      onChange: () => {
        session.refreshDirty();
        refreshCount();
        // The binding decides whether an output value can do anything, so the
        // cell has to follow it: switching a test from a scale to a threshold
        // is what turns the field on.
        renderOutput();
      },
    }),
  );

  return el(
    "tr",
    { "data-device": device.key, "data-led": led.name },
    name,
    driven,
    output,
  );
}

// ---------------------------------------------------------------------- boot

async function start(): Promise<void> {
  try {
    [devices, modules] = await Promise.all([listDevices(), listModules()]);
  } catch (e) {
    showError("Loading the hardware inventory", e);
    return;
  }
  await showLibrary();
}

void start();
