// The window. Two screens: the profile library, and one profile being edited.
//
// No framework. The editor is a fixed list of lamp rows with a search box on
// each, which is not enough state to be worth a runtime, and a smaller install
// matters for something that ships next to a daemon.

import { getCurrentWindow } from "@tauri-apps/api/window";

import {
  catalogueStatus,
  checkProfile,
  openPages,
  cloneProfile,
  createProfile,
  defaultProfile,
  listDevices,
  listModules,
  listProfiles,
  listSignals,
  openProfile,
  openUpdate,
  resetProfile,
  deleteProfile,
  exportPages,
  exportProfile,
  importPick,
  importProfile,
  mergeParts,
  mergeProfile,
  saveProfile,
  updateCheck,
} from "./api";
import { bindingEditor, iconButton } from "./binding";
import { confirmAction } from "./confirm";
import { manageConverter } from "./converter";
import { loadTheme, showSettings } from "./settings";
import { showFieldCautions, showFlags } from "./flags";
import { pageBook, pagesChecked, pageSection, pageUnsaved, showPageProblems } from "./pages";
import type { PageBook } from "./pages";
import { setLearnContext, stopLearning } from "./learn";
import { infoIcon } from "./typeahead";
import type {
  Binding,
  Device,
  ExportPage,
  ImportPreview,
  Led,
  LinePart,
  MergeParts,
  MergePick,
  MergeReport,
  MergeSource,
  ModuleChoice,
  PagePlan,
  PageTake,
  Profile,
  ProfileSummary,
  SignalView,
  Update,
} from "./types";

const app = document.getElementById("app") as HTMLElement;

/**
 * Whether anything on screen would be lost by closing the window.
 *
 * Held here rather than reached for inside whichever page happens to be up,
 * so the close guard has one question to ask and every page has one thing to
 * answer. A page with nothing to lose says so by leaving it alone.
 */
let unsavedWork: () => boolean = () => false;

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

/**
 * An aircraft list short enough to read at a glance, and the whole list for a
 * tooltip.
 *
 * FC3 covers over a hundred aircraft, and printed in full it buries the line
 * it sits in. Whole names are kept up to about `limit` characters and the rest
 * are counted rather than cut mid-name, so it is plain that the list goes on.
 * At least one name is always shown, however long.
 */
function aircraftSummary(names: string[], limit = 50): { text: string; title: Record<string, string> } {
  const full = names.join(", ");
  if (full.length <= limit) return { text: full, title: {} };
  const shown: string[] = [];
  for (const name of names) {
    if (shown.length > 0 && [...shown, name].join(", ").length > limit) break;
    shown.push(name);
  }
  return {
    text: `${shown.join(", ")} +${names.length - shown.length} more`,
    title: { title: full },
  };
}

function clear(): void {
  app.replaceChildren();
}

/**
 * A checkbox over others: ticking it ticks them all, and it shows ticked,
 * unticked or part ticked as they are. Nests: going down, what it ticks is
 * told with a change event; coming up, a box that has taken on its boxes'
 * state says so with an input event, so every level of a box over boxes over
 * boxes stays true. While it ticks its boxes it does not listen to them, or
 * the first one ticked would make it part ticked and stop the rest.
 */
function parentBox(children: HTMLInputElement[]): HTMLInputElement {
  const all = el("input", { type: "checkbox" });
  let setting = false;
  const reflect = (): void => {
    if (setting) return;
    const n = children.filter((b) => b.checked).length;
    all.checked = n === children.length;
    all.indeterminate = n > 0 && n < children.length;
    all.dispatchEvent(new Event("input"));
  };
  all.addEventListener("change", () => {
    const on = all.checked;
    setting = true;
    for (const b of children) {
      if (b.checked === on && !b.indeterminate) continue;
      b.checked = on;
      b.dispatchEvent(new Event("change"));
    }
    setting = false;
    reflect();
  });
  for (const b of children) {
    b.addEventListener("change", reflect);
    b.addEventListener("input", reflect);
  }
  reflect();
  return all;
}

/**
 * Head a checklist of aircraft with Select all, when there is more than one to
 * select, with the aircraft set in under it. It starts ticked when every
 * aircraft does, and part ticked when only some do.
 */
function selectAll(list: HTMLElement, boxes: HTMLInputElement[]): void {
  if (boxes.length < 2) return;
  for (const b of boxes) b.parentElement?.classList.add("sub");
  list.prepend(el("label", { class: "group" }, parentBox(boxes), el("span", {}, "Select all")));
}

function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

/**
 * The lights and screen lines a profile could give another, every one ticked
 * to start with. Lights go a lamp at a time under a box per panel, screens a
 * line at a time under a box per screen, and every group has a box over it.
 */
function mergeChecklist(parts: MergeParts, onChange: () => void): { node: HTMLElement; pick: () => MergePick } {
  const list = el("div", { class: "checklist" });
  const tick = (): HTMLInputElement => {
    const b = el("input", { type: "checkbox" });
    b.checked = true;
    b.addEventListener("change", onChange);
    return b;
  };
  const lights: [HTMLInputElement, { device: string; led: string }][] = [];
  const lines: [HTMLInputElement, LinePart][] = [];
  const slots: [HTMLInputElement, { device: string; slot: number }][] = [];

  if (parts.lights.length > 0) {
    const panelBoxes: HTMLInputElement[] = [];
    const rows: HTMLElement[] = [];
    for (const panel of parts.lights) {
      const boxes = panel.lamps.map((lamp) => {
        const b = tick();
        lights.push([b, { device: panel.device, led: lamp.led }]);
        return b;
      });
      const box = parentBox(boxes);
      panelBoxes.push(box);
      rows.push(
        el(
          "label",
          { class: "sub" },
          box,
          el("span", {}, panel.label),
          el("span", { class: "meta" }, plural(panel.lamps.length, "lamp")),
        ),
      );
      panel.lamps.forEach((lamp, i) => {
        rows.push(el("label", { class: "sub2" }, boxes[i] as HTMLInputElement, el("span", {}, lamp.label)));
      });
    }
    list.append(el("label", { class: "group" }, parentBox(panelBoxes), el("span", {}, "Lights")), ...rows);
  }

  if (parts.lines.length > 0) {
    const screens = new Map<string, LinePart[]>();
    for (const l of parts.lines) {
      const key = `${l.device}/${l.display}`;
      screens.set(key, [...(screens.get(key) ?? []), l]);
    }
    const screenBoxes: HTMLInputElement[] = [];
    const rows: HTMLElement[] = [];
    for (const group of screens.values()) {
      const boxes = group.map((l) => {
        const b = tick();
        lines.push([b, l]);
        return b;
      });
      const box = parentBox(boxes);
      screenBoxes.push(box);
      rows.push(el("label", { class: "sub" }, box, el("span", {}, group[0]?.screen ?? "")));
      group.forEach((l, i) => {
        const b = boxes[i] as HTMLInputElement;
        rows.push(
          el(
            "label",
            { class: "sub2" },
            b,
            el("span", {}, l.line),
            el("span", { class: "meta" }, plural(l.fields, "field")),
          ),
        );
      });
    }
    list.append(el("label", { class: "group" }, parentBox(screenBoxes), el("span", {}, "Screens")), ...rows);
  }

  // A screen merges a slot at a time: slot n of the source replaces slot n
  // here, and brings its page with it.
  if (parts.slots.length > 0) {
    const screens = new Map<string, typeof parts.slots>();
    for (const s of parts.slots) screens.set(s.device, [...(screens.get(s.device) ?? []), s]);
    const screenBoxes: HTMLInputElement[] = [];
    const rows: HTMLElement[] = [];
    for (const group of screens.values()) {
      const boxes = group.map((s) => {
        const b = tick();
        slots.push([b, { device: s.device, slot: s.slot }]);
        return b;
      });
      const box = parentBox(boxes);
      screenBoxes.push(box);
      rows.push(el("label", { class: "sub" }, box, el("span", {}, `${group[0]?.screen ?? ""} pages`)));
      group.forEach((s, i) => {
        rows.push(
          el(
            "label",
            { class: "sub2" },
            boxes[i] as HTMLInputElement,
            el("span", {}, `Slot ${s.slot}: ${s.blank ? "blank screen" : s.page}`),
            el("span", { class: "meta" }, s.start ? "starts" : ""),
          ),
        );
      });
    }
    list.append(el("label", { class: "group" }, parentBox(screenBoxes), el("span", {}, "Page slots")), ...rows);
  }

  if (lights.length === 0 && lines.length === 0 && slots.length === 0) {
    list.append(el("p", { class: "meta" }, "Nothing is set up in it to merge: no lamp assigned, no screen field and no page in a slot."));
  }

  const pick = (): MergePick => ({
    lights: lights.filter(([b]) => b.checked).map(([, d]) => d),
    lines: lines
      .filter(([b]) => b.checked)
      .map(([, l]) => ({ device: l.device, display: l.display, line: l.line })),
    slots: slots.filter(([b]) => b.checked).map(([, s]) => s),
  });
  return { node: list, pick };
}

/** One panel's or line's part of a merge, as a sentence. */
function describeChange(c: MergeReport["changes"][number]): string {
  if (c.pages) {
    const what = c.added > 0 ? "gets a page" : c.replaced > 0 ? "shows another page" : c.removed > 0 ? "is emptied" : "already shows that page";
    return `${c.label} ${what}.`;
  }
  const noun = c.fields ? "field" : "lamp";
  const bits: string[] = [];
  if (c.added > 0) bits.push(`${plural(c.added, noun)} added`);
  if (c.replaced > 0) bits.push(`${plural(c.replaced, noun)} replaced`);
  if (c.removed > 0) bits.push(`${plural(c.removed, noun)} removed`);
  if (c.unchanged > 0) bits.push(`${c.unchanged} already the same`);
  return `${c.label}: ${bits.length > 0 ? bits.join(", ") : "nothing to take"}.`;
}

/**
 * Merge what was ticked, once the user has seen what it does.
 *
 * Asked of the backend first without writing, so the question states what
 * will change rather than what was ticked, and a merge the daemon would
 * refuse is refused before it is asked. Nothing is asked when nothing would
 * change. Throws the backend's refusal for the caller to show. Resolves true
 * once written.
 */
async function runMerge(
  from: MergeSource,
  fromName: string,
  into: ProfileSummary,
  pick: MergePick,
): Promise<boolean> {
  const report = await mergeProfile(from, into.file, pick, false);
  const changing = report.changes.filter((c) => c.added + c.replaced + c.removed > 0);
  if (changing.length === 0) {
    throw new Error(`Everything ticked is already the same in ${into.name}, so there is nothing to merge.`);
  }
  const ok = await confirmAction(
    `Merge into ${into.name}?\n\n` +
      `From ${fromName}, this changes:\n${changing.map(describeChange).join("\n")}\n\n` +
      `A lamp takes ${fromName}'s setup where ${fromName} assigns it, and is left as it is where ` +
      `${fromName} does not. A screen line is replaced whole: its fields become ${fromName}'s. ` +
      `A page slot takes ${fromName}'s page, which comes into the library if it is not there. ` +
      `Everything not ticked, and ${into.name}'s name, aircraft and panel settings, stay as they are.` +
      (report.notes.length > 0 ? `\n\n${report.notes.join("\n")}` : "") +
      `\n\nThe rows replaced or removed cannot be recovered.`,
    "Merge",
  );
  if (!ok) return false;
  await mergeProfile(from, into.file, pick, true);
  return true;
}

/**
 * Something done that is worth a word and no more, in the update bar's style
 * along the foot of the window. Gone after ten seconds, and a newer one
 * replaces it rather than stacking.
 */
let bannerTimer: number | undefined;
function showBanner(text: string): void {
  document.querySelector(".update.banner")?.remove();
  window.clearTimeout(bannerTimer);
  const banner = el("div", { class: "update banner" }, el("span", {}, text));
  document.body.append(banner);
  bannerTimer = window.setTimeout(() => banner.remove(), 10_000);
}

/** Failures are shown in the window, never swallowed and never only in a console. */
function showError(where: string, e: unknown): void {
  const message = e instanceof Error ? e.message : String(e);
  app.prepend(el("div", { class: "error" }, `${where}: ${message}`));
}

// ------------------------------------------------------------------- library

/** Kept across views, so opening a profile and coming back keeps the list narrowed. */
let libraryFilter = "";

/**
 * Every word typed must appear somewhere in the profile's name, module or
 * aircraft. The full aircraft list is searched, not the shortened one shown,
 * so FC3's jets are findable by name.
 */
function matchesFilter(row: ProfileSummary, filter: string): boolean {
  const haystack = [row.name, row.module, ...row.aircraft].join(" ").toLowerCase();
  return filter
    .toLowerCase()
    .split(/\s+/)
    .every((word) => haystack.includes(word));
}

async function showLibrary(): Promise<void> {
  // Nothing here is edited in place, so there is nothing to lose by closing.
  unsavedWork = () => false;
  // Nothing on the profile list can use the stream, so the socket goes with it.
  stopLearning();

  clear();
  const filter = el("input", {
    type: "search",
    class: "filter",
    placeholder: "Filter by name, module or aircraft",
    value: libraryFilter,
  }) as HTMLInputElement;
  const header = el(
    "header",
    {},
    el("h1", {}, "Profiles"),
    el("div", { class: "spacer" }),
    filter,
    el("div", { class: "spacer" }),
    el("button", { class: "primary", id: "new" }, "New profile"),
    el("button", { class: "icon gear", id: "settings", type: "button", title: "Settings", "aria-label": "Settings" }, "\u2699"),
  );
  app.append(header);

  // Which DCS-BIOS the signals came from, and whether that just changed. On
  // this page because it is the first one the window opens on, and the one
  // place a user would look after updating DCS-BIOS.
  try {
    const status = await catalogueStatus();
    const cls = { ok: "meta catalogue", caution: "cautions", error: "error" }[status.level];
    app.append(el("div", { class: cls }, status.text));
  } catch {
    // Only reachable if the command itself is missing; the list still works.
  }

  let rows: ProfileSummary[];
  try {
    rows = await listProfiles();
  } catch (e) {
    showError("Loading profiles", e);
    return;
  }

  // Import and Manage Converter live in Settings, which hands on to them
  // once it has closed, so two dialogs are never open at once.
  header.querySelector("#settings")?.addEventListener("click", () => {
    void showSettings().then((next) => {
      if (next === "import") void showImport();
      if (next === "converter") {
        void manageConverter().then((said) => {
          if (said) showBanner(said);
        });
      }
    });
  });
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

  // One aircraft, one profile. Nothing in the window makes a second claim,
  // but a file copied in by hand can, and the daemon then flies the first by
  // file name and never the other. Named here so it can be fixed.
  const claims = new Map<string, ProfileSummary[]>();
  for (const row of [...rows].sort((a, b) => (a.file < b.file ? -1 : 1))) {
    if (row.error) continue;
    for (const a of row.aircraft) claims.set(a, [...(claims.get(a) ?? []), row]);
  }
  const twice = [...claims].filter(([, owners]) => owners.length > 1);
  if (twice.length > 0) {
    const box = el(
      "div",
      { class: "cautions" },
      el("strong", {}, twice.length === 1 ? "An aircraft has two profiles:" : "Some aircraft have two profiles:"),
    );
    for (const [a, owners] of twice) {
      const names = owners.map((o) => o.name);
      box.append(
        el(
          "div",
          { class: "caution" },
          `${a} is in ${names.join(" and ")}. ${names[0]} is the one flown. Remove it from the others, or delete them.`,
        ),
      );
    }
    app.append(box);
  }

  const list = el("ul", { class: "profiles" });
  const none = el("p", { class: "empty" }, "No profiles match.");
  const shown: [ProfileSummary, HTMLElement][] = [];
  for (const row of rows) {
    const aircraft = aircraftSummary(row.aircraft);
    const meta = row.error
      ? el("span", { class: "bad" }, row.error)
      : el(
          "span",
          { class: "meta", ...aircraft.title },
          `${row.module} · ${aircraft.text} · ${row.bound} of ${row.total} lamps assigned`,
        );

    const actions = el("div", { class: "actions" });
    if (!row.error) {
      const edit = el("button", {}, "Edit");
      edit.addEventListener("click", () => void showProfile(row.file));
      actions.append(edit);
    }
    if (!row.error) {
      const copy = el("button", {}, "Copy to...");
      copy.addEventListener("click", () => void showCloneProfile(row));
      const share = el("button", {}, "Export...");
      share.addEventListener("click", () => void exportOne(row));
      actions.append(copy, share);
      // Offered only when there is something on the same module to take from.
      if (mergeSources(row, rows).length > 0) {
        const take = el("button", {}, "Merge from...");
        take.addEventListener("click", () => void showMergeFrom(row, rows));
        actions.append(take);
      }
    }
    if (row.has_default) {
      const reset = el("button", { class: "danger" }, "Reset");
      reset.addEventListener("click", () => void resetOne(row));
      actions.append(reset);
    }
    // A shipped profile goes only when another can take its aircraft. Deleted
    // otherwise, it would be seeded straight back, so the button would be a
    // Reset under another name.
    const plan = deletePlan(row, rows);
    const canDelete = !row.has_default || plan.orphans.length === 0 || plan.homes.length > 0;
    if (canDelete) {
      const remove = el("button", { class: "danger" }, "Delete");
      remove.addEventListener("click", () => void deleteOne(row, rows));
      actions.append(remove);
    }

    const item = el("li", {}, el("div", { class: "grow" }, el("strong", {}, row.name), el("br"), meta), actions);
    shown.push([row, item]);
    list.append(item);
  }
  app.append(list, none);

  const apply = (): void => {
    libraryFilter = filter.value;
    const text = libraryFilter.trim();
    let visible = 0;
    for (const [row, item] of shown) {
      const show = text === "" || matchesFilter(row, text);
      item.hidden = !show;
      if (show) visible++;
    }
    none.hidden = visible > 0;
  };
  filter.addEventListener("input", apply);
  filter.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && filter.value !== "") {
      filter.value = "";
      apply();
    }
  });
  apply();
}

/**
 * Save a copy of one profile where the user chooses, with its pages, and say
 * where it went.
 *
 * The pages its slots show always go. Where the module has others, a dialog
 * offers them first, unticked, so a page someone kept out of every slot can
 * still be shared.
 */
async function exportOne(row: ProfileSummary): Promise<void> {
  const send = async (also: string[]): Promise<void> => {
    try {
      const to = await exportProfile(row.file, also);
      if (to === null) return;
      showBanner(`Exported ${row.name} to ${to}.`);
    } catch (e) {
      showError("Exporting the profile", e);
    }
  };
  let pages: ExportPage[];
  try {
    pages = await exportPages(row.file);
  } catch (e) {
    showError("Exporting the profile", e);
    return;
  }
  if (pages.every((p) => p.used)) {
    await send([]);
    return;
  }

  const list = el("div", { class: "checklist" });
  const extra: [HTMLInputElement, string][] = [];
  for (const p of pages) {
    const box = el("input", { type: "checkbox" }) as HTMLInputElement;
    box.checked = p.used;
    box.disabled = p.used;
    if (!p.used) extra.push([box, p.id]);
    list.append(
      el("label", {}, box, el("span", {}, p.name), el("span", { class: "meta" }, p.used ? "in a slot, so it goes" : "")),
    );
  }
  const go = el("button", { class: "primary" }, "Export...");
  const cancel = el("button", {}, "Cancel");
  const dialog = el(
    "dialog",
    { class: "picker" },
    el("h2", {}, `Export ${row.name}`),
    el("p", { class: "meta" }, `Which ${row.module} pages go with it? The ones its slots show always do.`),
    list,
    el("div", { class: "actions" }, cancel, go),
  );
  app.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());
  dialog.showModal();
  cancel.addEventListener("click", () => dialog.close());
  go.addEventListener("click", () => {
    const also = extra.filter(([b]) => b.checked).map(([, id]) => id);
    dialog.close();
    void send(also);
  });
}

/**
 * Import a profile someone shared.
 *
 * The backend asks for the file and refuses one that would not load here. The
 * dialog then works like New profile's aircraft step: free aircraft start
 * ticked, ones another profile flies start unticked with where they are. Taking
 * one asks first. A profile that would be left with no aircraft is deleted, so
 * that asks too, and saying no to it cancels the import outright.
 *
 * Or it is merged into a profile already here on the same module, taking only
 * the lights and screen lines ticked, and saying what that changes before it
 * is done. Importing it whole asks nothing more than the moves and deletes
 * above, since it rewrites no profile in place.
 */
async function showImport(): Promise<void> {
  let preview: ImportPreview | null;
  let existing: ProfileSummary[];
  try {
    preview = await importPick();
    if (preview === null) return;
    existing = await listProfiles();
  } catch (e) {
    showError("Importing a profile", e);
    return;
  }
  const picked: ImportPreview = preview;
  const claimedBy = new Map<string, ProfileSummary>();
  for (const p of existing) {
    if (p.error) continue;
    for (const a of p.aircraft) if (!claimedBy.has(a)) claimedBy.set(a, p);
  }

  const boxes: HTMLInputElement[] = [];
  const list = el("div", { class: "checklist" });
  for (const a of picked.aircraft) {
    const owner = claimedBy.get(a);
    const box = el("input", { type: "checkbox", value: a });
    // Every aircraft it was made for starts ticked, even one another profile
    // flies: taking it is still asked before anything moves.
    box.checked = true;
    boxes.push(box);
    const row = el("label", {}, box, el("span", {}, a));
    if (owner) row.append(el("span", { class: "meta" }, `in ${owner.name}`));
    list.append(row);
  }
  selectAll(list, boxes);
  const chosen = (): string[] => boxes.filter((b) => b.checked).map((b) => b.value);

  // Profiles here it could be merged into instead.
  const targets = existing.filter((p) => !p.error && p.module === picked.module);
  const mode = el("select", {});
  mode.append(el("option", { value: "" }, "Whole profile, as a profile of its own"));
  for (const t of targets) {
    mode.append(el("option", { value: t.file }, `Merged into ${t.name}: only the lights, screen lines and page slots ticked`));
  }
  const into = (): ProfileSummary | undefined => targets.find((t) => t.file === mode.value);

  const name = el("input", { type: "text", value: picked.name });
  const moves = el("p", { class: "meta" });

  // The pages it brings, each ticked on its own and named as it would come in.
  // One already here unchanged adds nothing, so it has no name to give.
  const pageRows: { plan: PagePlan; box: HTMLInputElement; name: HTMLInputElement }[] = [];
  const pageList = el("div", { class: "checklist" });
  for (const plan of picked.pages) {
    const box = el("input", { type: "checkbox" }) as HTMLInputElement;
    box.checked = true;
    const called = el("input", { type: "text", value: plan.name_after }) as HTMLInputElement;
    const fate =
      plan.fate === "same"
        ? "already here, unchanged"
        : plan.fate === "new_id"
          ? "a different page here has its id, so it comes in as a new page"
          : "new";
    const row = el("label", {}, box, el("span", {}, plan.name));
    if (plan.fate === "same") called.hidden = true;
    row.append(called, el("span", { class: "meta" }, `${fate}${plan.used ? "; in a slot" : ""}`));
    pageList.append(row);
    pageRows.push({ plan, box, name: called });
    box.addEventListener("change", () => sync());
    called.addEventListener("input", () => sync());
  }
  const pagesTaken = (): PageTake[] =>
    pageRows.filter((r) => r.box.checked).map((r) => ({ id: r.plan.id, name: r.name.value.trim() || r.plan.name_after }));
  const failed = el("p", { class: "bad" });
  const make = el("button", { class: "primary" }, "Import");
  const cancel = el("button", {}, "Cancel");

  /** Each profile that would give up aircraft, and whether it would have none left. */
  const takes = (): { from: ProfileSummary; taken: string[]; emptied: boolean }[] => {
    const aircraft = chosen();
    return existing
      .filter((p) => !p.error)
      .map((p) => {
        const taken = p.aircraft.filter((a) => aircraft.includes(a));
        return { from: p, taken, emptied: taken.length === p.aircraft.length };
      })
      .filter((t) => t.taken.length > 0);
  };

  const sync = (): void => {
    const lines = takes().map((t) =>
      t.emptied
        ? `${t.taken.join(", ")} out of ${t.from.name}, which would then be deleted`
        : `${t.taken.join(", ")} out of ${t.from.name}`,
    );
    moves.textContent = lines.length > 0 ? `Moves ${lines.join("; ")}.` : "";
    const merging = into() !== undefined;
    whole.hidden = merging;
    merge.node.hidden = !merging;
    make.textContent = merging ? "Merge..." : "Import";
    const pick = merge.pick();
    const ready = merging
      ? pick.lights.length + pick.lines.length + pick.slots.length > 0
      : chosen().length > 0 && name.value.trim() !== "";
    if (ready) make.removeAttribute("disabled");
    else make.setAttribute("disabled", "");
  };
  const merge = mergeChecklist(picked.parts, () => sync());
  const whole = el(
    "div",
    {},
    el("p", { class: "meta" }, "Which aircraft is it for? These are the ones it was made for."),
    list,
    el("label", { class: "field" }, "Name", name),
    moves,
  );
  if (pageRows.length > 0) {
    whole.append(
      el("p", { class: "meta" }, "Which pages come with it? A slot showing a page left unticked comes in empty."),
      pageList,
    );
  }
  name.addEventListener("input", sync);
  mode.addEventListener("change", () => {
    failed.textContent = "";
    sync();
  });
  for (const b of boxes) b.addEventListener("change", sync);

  const about = [`Reads ${picked.module}.`];
  if (picked.author.trim() !== "") about.push(`Made by ${picked.author}.`);
  about.push(`${picked.bound} of ${picked.total} lamps assigned.`);
  const dialog = el(
    "dialog",
    { class: "picker" },
    el("h2", {}, `Import ${picked.name}`),
    el("p", { class: "meta" }, about.join(" ")),
  );
  // Neither stops the import. They are said here because the user is about
  // to own the profile, and afterwards it looks like their own doing.
  const notes = [...picked.cautions];
  if (picked.flagged > 0) {
    notes.unshift(
      `${picked.flagged} ${picked.flagged === 1 ? "row reads a signal" : "rows read signals"} the DCS-BIOS installed here does not have, so ${picked.flagged === 1 ? "it stays" : "they stay"} off.`,
    );
  }
  if (notes.length > 0) {
    const box = el("div", { class: "cautions" });
    for (const n of notes) box.append(el("div", { class: "caution" }, n));
    dialog.append(box);
  }
  // Merging is offered only where a profile here reads the same module.
  if (targets.length > 0) dialog.append(el("label", { class: "field" }, "Import as", mode));
  dialog.append(whole, merge.node, failed, el("div", { class: "actions" }, cancel, make));
  app.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());
  dialog.showModal();
  sync();
  name.select();

  cancel.addEventListener("click", () => dialog.close());
  make.addEventListener("click", () => {
    void (async () => {
      const target = into();
      if (target) {
        try {
          if (!(await runMerge({ kind: "file", path: picked.path }, picked.name, target, merge.pick()))) return;
          dialog.close();
          showBanner(`Merged ${picked.name} into ${target.name}.`);
          await showProfile(target.file);
        } catch (e) {
          failed.textContent = e instanceof Error ? e.message : String(e);
        }
        return;
      }
      const plan = takes();
      // Taking an aircraft from another profile changes what that one flies,
      // so it is asked, not only listed.
      if (plan.length > 0) {
        const lines = plan.map((t) => `${t.taken.join(", ")} from ${t.from.name}`);
        const take = await confirmAction(
          `Take ${lines.join(" and ")}?

` +
            `${plan.length === 1 ? "That profile" : "Those profiles"} will no longer fly ` +
            `${plan.reduce((n, t) => n + t.taken.length, 0) === 1 ? "it" : "them"}; ${name.value.trim()} will.`,
          "Take",
        );
        if (!take) return;
      }
      // A profile with no aircraft can never fly, so it goes, but only if
      // the user says so. No leaves nothing half done: the import is off.
      const emptied = plan.filter((t) => t.emptied).map((t) => t.from);
      if (emptied.length > 0) {
        const names = emptied.map((p) => p.name).join(" and ");
        const gone = await confirmAction(
          `Delete ${names}?

` +
            `${emptied.length === 1 ? "It" : "They"} would be left with no aircraft. ` +
            `The ${emptied.length === 1 ? "file is" : "files are"} removed and cannot be recovered.

` +
            `Cancel stops the import, and nothing is changed.`,
          "Delete",
        );
        if (!gone) {
          dialog.close();
          return;
        }
      }
      try {
        const file = await importProfile(
          picked.path,
          name.value,
          chosen(),
          emptied.map((p) => p.file),
          pagesTaken(),
        );
        dialog.close();
        await showProfile(file);
      } catch (e) {
        // Kept open, since the usual cause is a name already taken.
        failed.textContent = e instanceof Error ? e.message : String(e);
      }
    })();
  });
}

/** Profiles `row` could take lights and screen lines from: any other on its module. */
function mergeSources(row: ProfileSummary, rows: ProfileSummary[]): ProfileSummary[] {
  return rows.filter((r) => r.file !== row.file && !r.error && r.module === row.module);
}

/**
 * Take some of another profile's setup into this one.
 *
 * For profiles that read one module and are kept apart on purpose, the F-14
 * and F-14BU being the shipped case: a change made in one is wanted in the
 * other, without making it twice. Laid out like Import's merge, which it is,
 * with a profile here as the source in place of a file.
 */
async function showMergeFrom(row: ProfileSummary, rows: ProfileSummary[]): Promise<void> {
  const sources = mergeSources(row, rows);
  const from = el("select", {});
  for (const s of sources) from.append(el("option", { value: s.file }, s.name));
  const holder = el("div", {});
  const failed = el("p", { class: "bad" });
  const make = el("button", { class: "primary" }, "Merge...");
  const cancel = el("button", {}, "Cancel");
  let merge: { node: HTMLElement; pick: () => MergePick } | null = null;

  const sync = (): void => {
    const pick = merge?.pick();
    if (pick && pick.lights.length + pick.lines.length + pick.slots.length > 0) make.removeAttribute("disabled");
    else make.setAttribute("disabled", "");
  };
  const load = async (): Promise<void> => {
    failed.textContent = "";
    merge = null;
    sync();
    try {
      merge = mergeChecklist(await mergeParts(from.value), sync);
      holder.replaceChildren(merge.node);
    } catch (e) {
      holder.replaceChildren();
      failed.textContent = e instanceof Error ? e.message : String(e);
    }
    sync();
  };
  from.addEventListener("change", () => void load());

  const dialog = el(
    "dialog",
    { class: "picker" },
    el("h2", {}, `Merge into ${row.name}`),
    el(
      "p",
      { class: "meta" },
      `Take lights and screen lines from another ${row.module} profile. Nothing is changed until you confirm.`,
    ),
    el("label", { class: "field" }, "Take from", from),
    holder,
    failed,
    el("div", { class: "actions" }, cancel, make),
  );
  app.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());
  dialog.showModal();
  cancel.addEventListener("click", () => dialog.close());
  make.addEventListener("click", () => {
    const source = sources.find((s) => s.file === from.value);
    if (!source || !merge) return;
    const pick = merge.pick();
    void (async () => {
      try {
        if (!(await runMerge({ kind: "profile", file: source.file }, source.name, row, pick))) return;
        dialog.close();
        showBanner(`Merged ${source.name} into ${row.name}.`);
        await showLibrary();
      } catch (e) {
        failed.textContent = e instanceof Error ? e.message : String(e);
      }
    })();
  });
  await load();
}

/** Reset discards the user's work, so it asks first and says exactly what it does. */
async function resetOne(row: ProfileSummary): Promise<void> {
  const ok = await confirmAction(
    `Replace ${row.name} with the profile that shipped with DCS Signal Converter?\n\n` +
      `Any changes you have made to it will be lost. Profiles you created yourself are not affected.`,
    "Reset",
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
 * What deleting `row` would leave behind: the aircraft only it flies, and the
 * profiles that could take all of them.
 *
 * A profile can take an aircraft when it reads the same module and already
 * flies one of the same family, which is how the shipped profiles group them.
 * The F-14 and F-14BU share a module and ship apart, and "No aircraft" rides on
 * FC3 without being one, so neither pair can take the other's aircraft.
 */
function deletePlan(row: ProfileSummary, rows: ProfileSummary[]): { orphans: string[]; homes: ProfileSummary[] } {
  const others = rows.filter((r) => r.file !== row.file && !r.error);
  // An aircraft another profile already claims keeps flying that one.
  const orphans: string[] = [];
  const needed = new Set<string>();
  row.aircraft.forEach((a, i) => {
    if (others.some((o) => o.aircraft.includes(a))) return;
    orphans.push(a);
    needed.add(row.families[i] ?? "");
  });
  const homes = others.filter((o) => o.module === row.module && [...needed].every((f) => o.families.includes(f)));
  return { orphans, homes };
}

/**
 * Delete cannot be undone, so it asks first, and it asks where the aircraft go.
 *
 * Laid out like the confirm dialog, with one choice added when deleting would
 * leave an aircraft with no profile: which profile on the same module takes
 * it. That is the split case, an A-10C profile copied into an A-10C II one,
 * where deleting either half should be able to hand its aircraft back.
 */
async function deleteOne(row: ProfileSummary, rows: ProfileSummary[]): Promise<void> {
  const { orphans, homes } = deletePlan(row, rows);

  // A shipped profile left with aircraft nobody else flies is seeded straight
  // back as shipped, on the very next listing, so it has no "No profile"
  // choice. The row offers Delete only when it has somewhere to send them.
  const mustRehome = row.has_default && orphans.length > 0;

  const target = el("select", {});
  for (const h of homes) target.append(el("option", { value: h.file }, h.name));
  if (!mustRehome) target.append(el("option", { value: "" }, "No profile"));
  const outcome = el("p", { class: "meta" });
  const sync = (): void => {
    const list = orphans.join(", ");
    if (target.value !== "") {
      outcome.textContent = `${target.selectedOptions[0]?.textContent ?? ""} will fly ${list} as well as its own aircraft.`;
    } else {
      outcome.textContent = `${list} will fly with the panels cleared until a profile is made for ${orphans.length === 1 ? "it" : "them"}.`;
    }
  };
  target.addEventListener("change", sync);
  sync();

  const cancel = el("button", {}, "Cancel");
  const go = el("button", { class: "danger" }, "Delete");
  const dialog = el(
    "dialog",
    { class: "picker confirm" },
    el("h2", {}, `Delete ${row.name}?`),
    el("p", { class: "meta" }, "The file is removed and cannot be recovered."),
  );
  if (orphans.length > 0) {
    dialog.append(
      el(
        "label",
        { class: "field" },
        `${orphans.join(", ")} ${orphans.length === 1 ? "has" : "have"} no other profile. Give ${orphans.length === 1 ? "it" : "them"} to`,
        target,
      ),
      outcome,
    );
  }
  dialog.append(el("div", { class: "actions" }, cancel, go));
  app.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());
  dialog.showModal();
  // Cancel takes the focus, as it does in every confirm, so Enter on a
  // misclick deletes nothing.
  cancel.focus();

  cancel.addEventListener("click", () => dialog.close());
  go.addEventListener("click", () => {
    const giveTo = orphans.length > 0 && target.value !== "" ? target.value : null;
    dialog.close();
    void (async () => {
      try {
        await deleteProfile(row.file, giveTo);
        await showLibrary();
      } catch (e) {
        showError("Deleting the profile", e);
      }
    })();
  });
}

/**
 * Copy a profile to another aircraft.
 *
 * The case this exists for is a module that reuses another's DCS-BIOS
 * definitions. The Super Hornet community mod reads the Hornet's, so the
 * Hornet profile drives it unchanged and only the name and the aircraft list
 * differ. Doing that by hand means copying a file and editing two fields.
 *
 * The module is carried over and not offered, because a copy whose signal ids
 * resolve against a different catalogue is not a copy, it is a profile full of
 * signals that do not exist.
 */
async function showCloneProfile(row: ProfileSummary): Promise<void> {
  // Needed to say which aircraft the copy would take from other profiles.
  let existing: ProfileSummary[];
  try {
    existing = await listProfiles();
  } catch (e) {
    showError("Loading profiles", e);
    return;
  }

  const name = el("input", { type: "text", value: `${row.name} copy` });
  // Prefilled with what it came from, because the common case is one aircraft
  // name away from the original and the exact spelling is easy to get wrong.
  // Left as it is, the copy would take every aircraft from the original, which
  // the move note below says and Copy refuses.
  const aircraft = el("input", { type: "text", value: row.aircraft.join(", ") });
  const moves = el("p", { class: "meta" });

  const dialog = el(
    "dialog",
    { class: "picker" },
    el("h2", {}, `Copy ${row.name}`),
    el(
      "p",
      { class: "meta" },
      `Everything assigned in this profile is carried over, still reading ${row.module}. ` +
        `Give the copy a name and say which aircraft DCS reports for it.`,
    ),
    el("label", { class: "field" }, "Name", name),
    el(
      "label",
      { class: "field" },
      "Aircraft",
      aircraft,
      el(
        "span",
        { class: "meta block" },
        "As DCS reports it, not as it is written in the mission editor. " +
          "Separate several with commas.",
      ),
    ),
    moves,
    el(
      "div",
      { class: "actions" },
      el("button", { id: "cancel" }, "Cancel"),
      el("button", { class: "primary", id: "make" }, "Copy"),
    ),
  );
  app.append(dialog);
  dialog.showModal();
  name.select();

  const make = dialog.querySelector("#make") as HTMLButtonElement;
  const typed = (): string[] =>
    aircraft.value
      .split(",")
      .map((a) => a.trim())
      .filter((a) => a !== "");
  const sync = (): void => {
    const { text, emptied } = describeMoves(typed(), existing);
    moves.textContent = text;
    moves.className = emptied.length > 0 ? "bad" : "meta";
    if (emptied.length === 0 && typed().length > 0) make.removeAttribute("disabled");
    else make.setAttribute("disabled", "");
  };
  aircraft.addEventListener("input", sync);
  sync();

  dialog.querySelector("#cancel")?.addEventListener("click", () => dialog.close());
  dialog.querySelector("#make")?.addEventListener("click", () => {
    const names = aircraft.value
      .split(",")
      .map((a) => a.trim())
      .filter((a) => a !== "");
    dialog.close();
    void (async () => {
      try {
        const file = await cloneProfile(row.file, name.value, names);
        await showProfile(file);
      } catch (e) {
        showError("Copying the profile", e);
      }
    })();
  });
}

/**
 * What giving `aircraft` to a new profile would take from existing ones.
 *
 * One aircraft, one profile: an aircraft another profile claims moves to the
 * new one, and the backend refuses a move that leaves a profile claiming
 * nothing. Worked out here too so the dialog can say it before the click.
 */
function describeMoves(
  aircraft: string[],
  existing: ProfileSummary[],
): { text: string; emptied: string[] } {
  const moves: string[] = [];
  const emptied: string[] = [];
  for (const p of existing) {
    if (p.error) continue;
    const taken = p.aircraft.filter((a) => aircraft.includes(a));
    if (taken.length === 0) continue;
    if (taken.length === p.aircraft.length) emptied.push(p.name);
    else moves.push(`${taken.join(", ")} out of ${p.name}`);
  }
  if (emptied.length > 0) {
    return {
      text: `${emptied.join(", ")} would be left with no aircraft. Leave at least one of its aircraft unselected, or edit that profile instead.`,
      emptied,
    };
  }
  return { text: moves.length > 0 ? `Moves ${moves.join("; ")}.` : "", emptied };
}

/**
 * New profiles pick a module, then which of its aircraft they are for.
 *
 * Both lists come from the catalogue built from the user's own DCS-BIOS, so
 * neither can offer something they do not have, and the aircraft names come
 * from it because one module often serves several and the mapping is not
 * guessable. The aircraft step exists because sharing DCS-BIOS outputs does
 * not mean wanting the same lamps: the A-10C and A-10C II read one module.
 */
async function showNewProfile(): Promise<void> {
  if (modules.length === 0) {
    showError(
      "No modules available",
      new Error("The signal catalogue is empty. Build it from your DCS-BIOS install first."),
    );
    return;
  }

  let existing: ProfileSummary[];
  try {
    existing = await listProfiles();
  } catch (e) {
    showError("Loading profiles", e);
    return;
  }
  const claimedBy = new Map<string, ProfileSummary>();
  for (const p of existing) {
    if (p.error) continue;
    for (const a of p.aircraft) if (!claimedBy.has(a)) claimedBy.set(a, p);
  }

  const dialog = el("dialog", { class: "picker" });
  app.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());

  /** Step one: the module. Every one is offered; step two handles claims. */
  const pickModule = (chosen?: string): void => {
    const select = el("select", { id: "module", size: "12" });
    for (const m of modules) {
      const summary = aircraftSummary(m.aircraft);
      const names = m.aircraft.length > 0 ? summary.text : "no runtime aircraft name";
      const claimed = m.aircraft.filter((a) => claimedBy.has(a)).length;
      const note =
        claimed === 0
          ? ""
          : claimed === m.aircraft.length
            ? "  ·  every aircraft has a profile"
            : `  ·  ${claimed} of ${m.aircraft.length} aircraft have a profile`;
      select.append(el("option", { value: m.key, ...summary.title }, `${m.key}  ·  ${names}${note}`));
    }
    if (chosen) select.value = chosen;

    const next = el("button", { class: "primary" }, "Next");
    // Nothing is selected when the list opens, so Next waits for a choice.
    const sync = (): void => {
      if (select.value) next.removeAttribute("disabled");
      else next.setAttribute("disabled", "");
    };
    sync();
    select.addEventListener("change", sync);
    const go = (): void => {
      const m = modules.find((x) => x.key === select.value);
      if (m) pickAircraft(m);
    };
    next.addEventListener("click", go);
    select.addEventListener("dblclick", go);

    const cancel = el("button", {}, "Cancel");
    cancel.addEventListener("click", () => dialog.close());

    dialog.replaceChildren(
      el("h2", {}, "Which module?"),
      el("p", { class: "meta" }, `${modules.length} modules in your DCS-BIOS install.`),
      select,
      el("div", { class: "actions" }, cancel, next),
    );
    select.focus();
  };

  /** Step two: which of the module's aircraft, and what to start from. */
  const pickAircraft = (m: ModuleChoice): void => {
    // A module with no runtime name is offered under its key.
    const names = m.aircraft.length > 0 ? m.aircraft : [m.key];

    const boxes: HTMLInputElement[] = [];
    const list = el("div", { class: "checklist" });
    for (const a of names) {
      const owner = claimedBy.get(a);
      const box = el("input", { type: "checkbox", value: a });
      // Free aircraft start chosen. Claimed ones are there to be taken, not
      // taken by default.
      box.checked = owner === undefined;
      boxes.push(box);
      const row = el("label", {}, box, el("span", {}, a));
      if (owner) row.append(el("span", { class: "meta" }, `in ${owner.name}`));
      list.append(row);
    }
    selectAll(list, boxes);
    const chosen = (): string[] => boxes.filter((b) => b.checked).map((b) => b.value);

    // Anything this could sensibly be copied from: a profile on the same
    // module, or one that claims an aircraft listed here.
    const sources = existing.filter(
      (p) => !p.error && (p.module === m.key || p.aircraft.some((a) => names.includes(a))),
    );
    const from = el("select", {});
    from.append(el("option", { value: "" }, "Blank: every lamp unassigned"));
    for (const p of sources) from.append(el("option", { value: p.file }, `Copy of ${p.name}`));
    let fromTouched = false;
    from.addEventListener("change", () => {
      fromTouched = true;
    });

    const name = el("input", { type: "text" });
    let nameTouched = false;

    const moves = el("p", { class: "meta" });
    const make = el("button", { class: "primary" }, "Create");

    const sync = (): void => {
      const picked = chosen();
      // Named after the one aircraft when there is one, else the module.
      if (!nameTouched) name.value = picked.length === 1 ? (picked[0] ?? m.key) : m.key;
      // Taking a claimed aircraft suggests copying the profile it comes from,
      // until the user picks a starting point themselves.
      if (!fromTouched) {
        const owners = [...new Set(picked.map((a) => claimedBy.get(a)))].filter(
          (p): p is ProfileSummary => p !== undefined,
        );
        from.value = owners.length === 1 ? (owners[0]?.file ?? "") : "";
      }
      const { text, emptied } = describeMoves(picked, existing);
      moves.textContent = text;
      moves.className = emptied.length > 0 ? "bad" : "meta";
      const ok = picked.length > 0 && name.value.trim() !== "" && emptied.length === 0;
      if (ok) make.removeAttribute("disabled");
      else make.setAttribute("disabled", "");
    };
    name.addEventListener("input", () => {
      nameTouched = true;
      sync();
    });
    for (const b of boxes) b.addEventListener("change", sync);
    sync();

    const back = el("button", {}, "Back");
    back.addEventListener("click", () => pickModule(m.key));
    const cancel = el("button", {}, "Cancel");
    cancel.addEventListener("click", () => dialog.close());
    make.addEventListener("click", () => {
      const picked = chosen();
      const source = from.value || null;
      const title = name.value;
      dialog.close();
      void (async () => {
        try {
          const file = await createProfile(m.key, title, picked, source);
          await showProfile(file);
        } catch (e) {
          showError("Creating the profile", e);
        }
      })();
    });

    dialog.replaceChildren(
      el("h2", {}, `New ${m.key} profile`),
      el("p", { class: "meta" }, "Which aircraft is it for? These are the names DCS reports for this module."),
      list,
      el("label", { class: "field" }, "Start from", from),
      el("label", { class: "field" }, "Name", name),
      moves,
      el("div", { class: "actions" }, back, el("div", { class: "spacer" }), cancel, make),
    );
  };

  dialog.showModal();
  pickModule();
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
  /**
   * Every device section's follow chooser, redrawn together. Pointing one
   * unit at another changes what the rest may offer, since a unit that
   * follows cannot be followed.
   */
  followSync: (() => void)[];
  /**
   * Re-run the daemon's own checks over the profile as it stands.
   *
   * Called from `refreshDirty`, so every edit is checked without each call
   * site having to remember to. Debounced, because it crosses into the backend
   * and an edit can be a keystroke.
   */
  recheck: () => void;
  /** The profile's module's pages, and the one open for editing, if any. */
  book: PageBook;
  /** Say something that happened, at the top of the page. */
  tell: (text: string) => void;
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

  // Learn mode reads signals against this profile's module, and warns when DCS
  // is flying something this profile does not cover.
  setLearnContext(profile.module, profile.aircraft);

  // What the other profiles are called, for the rename box: a name already in
  // use is refused, since the list shows nothing else to tell them apart.
  const taken = new Map<string, string>();
  try {
    for (const row of await listProfiles()) {
      if (row.file !== file && !row.error) taken.set(row.name.trim().toLowerCase(), row.name);
    }
  } catch {
    // Only costs the warning while typing; the save is still refused.
  }

  // The shipped copy, so one lamp or one field can be put back without
  // resetting the file. A profile the user made has none, which is not an
  // error.
  const shipped = new Map<string, Binding>();
  try {
    const original = await defaultProfile(file);
    for (const b of original?.bindings ?? []) {
      shipped.set(lampKey(b.device, b.led), b);
    }
  } catch {
    // A missing or unreadable default only costs the revert buttons.
  }

  // The module's pages, which the screens' slots point into. A library that
  // cannot be read leaves every slot showing as empty, with the reason.
  let book: PageBook;
  try {
    book = pageBook(profile.module, file, await openPages(profile.module));
  } catch (e) {
    const why = e instanceof Error ? e.message : String(e);
    book = pageBook(profile.module, file, { pages: [], broken: why, used: [] });
  }

  clear();

  const save = el("button", { class: "primary" }, "Save");
  const state = el("span", { class: "meta" }, "");

  // Outstanding problems, kept between checks so the Save button can consult
  // them without waiting for one. The list starts empty rather than unknown:
  // the first check runs as soon as the rows are built, and until it answers
  // the profile is whatever it was on disk.
  let problems: string[] = [];
  const problemList = el("div", { class: "problems", hidden: "" });
  // Cautions are about a profile that loads but probably does not do what was
  // meant. Shown beside the problems, and never a reason to withhold Save.
  let cautions: string[] = [];
  const cautionList = el("div", { class: "cautions", hidden: "" });
  // One line, only when a row reads something the DCS-BIOS nightly has and
  // the installed one lacks. The rows themselves carry the detail.
  const notice = el("div", { class: "cautions", hidden: "" });
  const drawNotice = (text: string | null): void => {
    notice.hidden = !text;
    notice.textContent = text ?? "";
  };

  /**
   * The header's state line, and whether Save is offered.
   *
   * Save is withheld while anything is outstanding. The daemon skips a profile
   * it will not load, whole, so writing one would darken every lamp in it, and
   * the version already on disk is very likely one that flies. Refusing costs
   * the user the time to finish; allowing it costs them the sortie.
   */
  const refreshSave = (): void => {
    state.textContent = session.dirty ? "unsaved changes" : "";
    if (problems.length > 0) {
      save.setAttribute("disabled", "");
      save.title = `${problems.length} problem${problems.length === 1 ? "" : "s"} to fix first.`;
      return;
    }
    save.title = "";
    if (session.dirty) save.removeAttribute("disabled");
    else save.setAttribute("disabled", "");
  };

  /**
   * What is wrong, in the daemon's own words, above the rows it is about.
   *
   * Shown in full rather than counted. Every one of these names a lamp or a
   * field, which is the only thing that makes it actionable, and there are
   * never many: the window prevents most of them from being made at all.
   */
  const drawProblems = (): void => {
    cautionList.replaceChildren();
    cautionList.hidden = cautions.length === 0;
    if (cautions.length > 0) {
      cautionList.append(
        el("strong", {}, cautions.length === 1 ? "This will load, but check it:" : "These will load, but check them:"),
      );
      for (const caution of cautions) {
        cautionList.append(el("div", { class: "caution" }, caution));
      }
    }

    problemList.replaceChildren();
    if (problems.length === 0) {
      problemList.hidden = true;
      return;
    }
    problemList.hidden = false;
    problemList.append(
      el(
        "strong",
        {},
        problems.length === 1
          ? "This profile will not load until this is fixed:"
          : `This profile will not load until these ${problems.length} are fixed:`,
      ),
    );
    for (const problem of problems) {
      problemList.append(el("div", { class: "problem" }, problem));
    }
  };

  let pending: number | undefined;
  const session: Session = {
    file,
    profile,
    signals,
    shipped,
    baseline: "",
    dirty: false,
    followSync: [],
    book,
    tell: showBanner,
    refreshDirty: () => {
      session.dirty = JSON.stringify(session.profile) !== session.baseline;
      refreshSave();
      session.recheck();
    },
    recheck: () => {
      // Coalesced, so holding a key down is one check rather than one per
      // character. Long enough to skip the middle of a word, short enough that
      // the answer is there by the time the user looks up from the row.
      window.clearTimeout(pending);
      pending = window.setTimeout(() => {
        // Snapshotted before the call: the user keeps typing while it is in
        // flight, and a late answer about an older profile must not be shown
        // as though it were about this one.
        const snapshot = (): string => JSON.stringify([session.profile, book.editing]);
        const asked = snapshot();
        const working = book.editing;
        void checkProfile(session.profile, working?.page ?? null, working?.device ?? null)
          .then((found) => {
            if (snapshot() !== asked) return;
            problems = found.problems;
            cautions = found.cautions;
            drawProblems();
            showFlags(session.profile, pagesChecked(book), found.flags);
            showFieldCautions(session.profile, pagesChecked(book), found.field_cautions);
            showPageProblems(book, found.page_problems);
            drawNotice(found.notice);
            refreshSave();
          })
          .catch((e: unknown) => {
            // A check that cannot run must not read as a profile with nothing
            // wrong, so the failure takes the same place the problems do.
            problems = [`The profile could not be checked: ${e instanceof Error ? e.message : String(e)}`];
            cautions = [];
            drawProblems();
            showFlags(session.profile, pagesChecked(book), []);
            showFieldCautions(session.profile, pagesChecked(book), []);
            showPageProblems(book, []);
            drawNotice(null);
            refreshSave();
          });
      }, 250);
    },
  };
  save.setAttribute("disabled", "");
  // A page open with changes is unsaved work too, though Save does not write it.
  unsavedWork = () => session.dirty || pageUnsaved(book);

  const back = el("button", {}, "← Profiles");
  back.addEventListener("click", () => {
    void (async () => {
      if (unsavedWork() && !(await confirmAction("Leave without saving? Your changes will be lost.", "Leave"))) return;
      await showLibrary();
    })();
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
  const aircraft = aircraftSummary(profile.aircraft);
  app.append(
    el(
      "header",
      {},
      back,
      el(
        "div",
        { class: "grow" },
        profileTitle(session, taken),
        el("span", { class: "meta block", ...aircraft.title }, `${profile.module} · ${aircraft.text}`),
      ),
      state,
      toggle,
      save,
    ),
  );
  app.append(notice, problemList, cautionList);

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

  const sections = devices.map((device) => deviceSection(device, devices, byLamp, session));
  app.append(...sections);

  // Taken after the rows are built, because building them adds a binding for
  // any lamp the file did not already list. Measuring before that would have
  // every profile arrive already dirty.
  session.baseline = JSON.stringify(session.profile);

  // Checked on open as well as on edit. A profile can be invalid without
  // anyone touching it here: the catalogue is rebuilt when DCS-BIOS updates
  // and a signal can leave it, and profiles are shared between people whose
  // panels differ. Saying so on arrival beats saying it on the ramp.
  session.recheck();

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
  // Only panels this profile drives can open, so only they count.
  const live = (): HTMLDetailsElement[] => sections.filter((s) => !s.classList.contains("off"));
  const syncToggle = (): void => {
    const open = live();
    if (open.length > 0 && open.every((s) => s.open)) setLabel(true);
    else if (open.every((s) => !s.open)) setLabel(false);
  };
  for (const s of sections) s.addEventListener("toggle", syncToggle);
  syncToggle();

  toggle.addEventListener("click", () => {
    // Does what the label says. Deciding from the sections instead would make a
    // button reading "Collapse all" expand everything as soon as one section
    // had been closed by hand.
    const expand = !offersCollapse;
    for (const section of live()) section.open = expand;
    syncToggle();
  });
}

/**
 * The profile's name, renamed in place with the pencil, the way a condition
 * is edited: keep or cancel, then Save writes it with everything else.
 *
 * Only the name changes, never the file. The file name is what seeding and
 * Reset match a shipped profile by, so renaming the file would bring the
 * shipped one back beside it; the name is only what the list shows.
 */
function profileTitle(session: Session, taken: Map<string, string>): HTMLElement {
  const title = el("div", { class: "title" });
  const view = (): void => {
    title.replaceChildren(
      el("h1", {}, session.profile.name),
      iconButton("pencil", "✎", "Rename this profile", edit),
    );
  };
  // The name is all the list shows, so two profiles sharing one cannot be
  // told apart. Said while it is typed; the backend refuses it either way.
  const clash = (value: string): string | undefined => taken.get(value.trim().toLowerCase());
  const edit = (): void => {
    const input = el("input", { type: "text", class: "rename", value: session.profile.name });
    const clashing = el("span", { class: "bad" });
    const keep = (): void => {
      const name = input.value.trim();
      if (name === "" || clash(name) !== undefined) return;
      session.profile.name = name;
      session.refreshDirty();
      view();
    };
    const done = iconButton("done", "✓", "Keep this name", keep);
    const sync = (): void => {
      const other = clash(input.value);
      clashing.textContent = other === undefined ? "" : `Another profile is already called ${other}.`;
      if (input.value.trim() === "" || other !== undefined) done.setAttribute("disabled", "");
      else done.removeAttribute("disabled");
    };
    input.addEventListener("input", sync);
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") keep();
      if (e.key === "Escape") view();
    });
    title.replaceChildren(input, done, iconButton("cancel", "✕", "Keep the old name", view), clashing);
    sync();
    input.select();
  };
  view();
  return title;
}

/**
 * A lamp missing from the profile gets a row added to it on first use, so a
 * profile written against fewer devices still edits cleanly.
 */
function bindingFor(device: Device, led: Led, byLamp: Map<string, Binding>, session: Session): Binding {
  const key = lampKey(device.key, led.name);
  const existing = byLamp.get(key);
  if (existing) return existing;
  // A gate starts with its daylight floor. Unassigned it does nothing yet, but
  // the floor is what the user almost always wants once they give it a dimmer
  // to follow, and a floor they have to know to add is one they will not.
  const off = led.governs.length > 0 ? led.max : 0;
  const fresh: Binding = { device: device.key, led: led.name, conditions: [], on: null, off };
  byLamp.set(key, fresh);
  session.profile.bindings.push(fresh);
  return fresh;
}

function deviceSection(
  device: Device,
  all: Device[],
  byLamp: Map<string, Binding>,
  session: Session,
): HTMLDetailsElement {
  const count = el("span", { class: "meta" }, "");
  const nameOf = (key: string): string => all.find((d) => d.key === key)?.display_name ?? key;
  // The unit this one takes its lamps and screen from, if it takes them from
  // anywhere. Its own rows stay in the profile and come back if it stops.
  const following = (): string | undefined => session.profile.follows?.[device.key];
  // What saving now would drive, so any form counts ("same as", "always",
  // "any of"), but a condition still waiting for its signal does not: the
  // profile check rejects it. Those are named instead of silently left out,
  // or an unfinished lamp reads as one the user missed. Refreshed when an edit
  // is settled rather than on every keystroke; see `onCommit`.
  const refreshCount = (): void => {
    const source = following();
    if (source) {
      count.textContent = `set up as ${nameOf(source)}`;
      return;
    }
    let assigned = 0;
    let unfinished = 0;
    for (const l of device.leds) {
      const binding = byLamp.get(lampKey(device.key, l.name));
      if (!binding || isPlaceholder(binding)) continue;
      if (isFinished(binding)) assigned += 1;
      else unfinished += 1;
    }
    count.textContent =
      `${assigned} of ${device.leds.length} lamps assigned` + (unfinished ? ` · ${unfinished} unfinished` : "");
  };

  const rows = el("tbody");
  for (const led of device.leds) {
    rows.append(lampRow(device, all, led, byLamp, session, refreshCount));
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
  // A panel left alone does not open. Its lamps and fields are kept, so ticking
  // it again brings back exactly what was set up, but there is nothing to edit
  // on a panel that will not be driven.
  //
  // A unit that follows another does not open either: what it will do is
  // edited on the one it follows, and its own rows are not in use.
  const shut = (): boolean => !drive.checked || following() !== undefined;
  const applyDriveState = (): void => {
    section.classList.toggle("off", !drive.checked);
    section.classList.toggle("follows", following() !== undefined);
    if (shut()) section.open = false;
  };
  // Stopped before it opens rather than closed after, which flashed the panel
  // open for a frame. The click is cancelled at the summary, which also covers
  // Enter and Space. The drive label is let through, since it is how the panel
  // is turned back on.
  const summary = el(
    "summary",
    {},
    el("span", { class: "name" }, device.display_name),
    count,
  );
  if (device.variants.length > 0) summary.append(followChooser(device, session, nameOf, () => {
    applyDriveState();
    refreshCount();
  }));
  summary.append(el("label", { class: "drive meta" }, drive, " drive this panel"));
  summary.addEventListener("click", (e) => {
    if (shut() && !(e.target as Element).closest("label")) e.preventDefault();
  });
  // Anything that opens it some other way is closed again.
  section.addEventListener("toggle", () => {
    if (section.open && shut()) section.open = false;
  });
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

  section.append(summary, table);
  // Every screen takes its fields from pages rather than from the profile.
  for (const display of device.displays) {
    section.append(
      pageSection(device, display, {
        profile: session.profile,
        signals: session.signals,
        book: session.book,
        profileChanged: session.refreshDirty,
        pageChanged: session.recheck,
        nameOf,
        tell: session.tell,
        fail: showError,
      }),
    );
  }
  return section;
}

/**
 * Which unit a device takes its setup from, for a device sold under more than
 * one name.
 *
 * The MCDU is Captain, Co-Pilot and Observer, and the MFD is L, C and R, each
 * with its own USB id and the same hardware. Pointing one at another means
 * setting it up once. One step deep, so there is always exactly one place to
 * edit: a unit that follows is not offered as a target, and a unit something
 * follows cannot follow in turn.
 *
 * `changed` is this section's own redraw. Every other section's chooser is
 * redrawn too, through `session.followSync`, since what they may offer moved.
 */
function followChooser(
  device: Device,
  session: Session,
  nameOf: (key: string) => string,
  changed: () => void,
): HTMLElement {
  const select = el("select", { class: "test" }) as HTMLSelectElement;
  const sync = (): void => {
    const follows = session.profile.follows ?? {};
    const mine = follows[device.key];
    const followedBy = Object.keys(follows).filter((k) => follows[k] === device.key);
    select.textContent = "";
    select.append(el("option", { value: "" }, "its own setup"));
    for (const key of device.variants) {
      // Kept when it is the current choice, so a file that chains two says
      // so through the problem list rather than by quietly reading as unset.
      if (follows[key] !== undefined && key !== mine) continue;
      select.append(el("option", { value: key }, `the ${nameOf(key)}'s`));
    }
    select.value = mine ?? "";
    select.disabled = followedBy.length > 0 && mine === undefined;
    select.title = select.disabled
      ? `${followedBy.map(nameOf).join(" and ")} ${followedBy.length === 1 ? "takes its" : "take their"} setup from this one, so it keeps its own.`
      : "Take another unit's lamps and screen instead of setting this one up again. Its own are kept for if it stops.";
    changed();
  };
  select.addEventListener("click", (e) => e.stopPropagation());
  select.addEventListener("change", () => {
    const follows = session.profile.follows ?? {};
    if (select.value) follows[device.key] = select.value;
    else delete follows[device.key];
    if (Object.keys(follows).length > 0) session.profile.follows = follows;
    else delete session.profile.follows;
    for (const redraw of session.followSync) redraw();
    session.refreshDirty();
  });
  session.followSync.push(sync);
  sync();
  return el("label", { class: "drive meta" }, "uses ", select);
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
/** Names a lamp but drives nothing yet. Mirrors `Binding::is_placeholder`. */
function isPlaceholder(binding: Binding): boolean {
  return !binding.conditions.length && !binding.any_of?.length && !binding.always && !binding.same_as;
}

/** Assigned, and every condition has its signal, so the profile check accepts it. */
function isFinished(binding: Binding): boolean {
  if (binding.always || binding.same_as) return true;
  const conditions = [...binding.conditions, ...(binding.any_of ?? []).flatMap((b) => b.conditions)];
  return conditions.length > 0 && conditions.every((c) => c.source !== "");
}

function onValueMatters(binding: Binding): boolean {
  if (binding.same_as) return false;
  if (binding.always) return true;
  const groups = binding.any_of?.length ? binding.any_of : [{ conditions: binding.conditions }];
  return groups.some((g) => g.conditions.some((c) => !("scale" in c.on_when)));
}

function lampRow(
  device: Device,
  all: Device[],
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

  /** The brightness when lit, or why there is none to set. */
  const litCell = (): HTMLElement => {
    if (binding.same_as) return el("span", { class: "meta" }, "matched");
    if (!onValueMatters(binding)) {
      // Either nothing is assigned yet, or every test is a scale. A scale
      // ignores `on` and spreads the source across the lamp's own range, so an
      // input here would be a control that quietly does nothing.
      return el("span", { class: "meta" }, binding.conditions.length || binding.any_of?.length ? `0..${led.max}` : "");
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
    return el("label", { class: "field" }, "when lit", input);
  };

  /**
   * The value when the binding resolves to 0: the lamp it follows is dark, or
   * the scaled source is at the bottom, or no condition holds.
   *
   * On a gate this is the daylight floor. Console lighting off means daylight,
   * not lamps off, and SL at 0 hides every indicator on the PTO2. It was only
   * reachable by editing the file, which is how the A-10C shipped without it.
   *
   * Not offered where it cannot apply: an unassigned lamp resolves to nothing,
   * and an always-on one never resolves to 0.
   */
  const atZeroCell = (): HTMLElement | null => {
    if (isPlaceholder(binding) || binding.always) return null;
    const input = el("input", {
      type: "number",
      class: "value",
      min: "0",
      max: String(led.max),
      // Absent in the file when it is 0, which the profile writer leaves out.
      value: String(binding.off ?? 0),
    });
    input.title =
      led.governs.length > 0
        ? `Value when the cockpit lighting is off. At 0 this hides the ${led.governs.length} lamps it governs, so ${led.max} keeps them readable in daylight.`
        : "Value when what this lamp follows is at zero, or none of its conditions hold.";
    input.addEventListener("change", () => {
      const n = Number(input.value);
      binding.off = Number.isFinite(n) ? Math.min(Math.max(Math.round(n), 0), led.max) : 0;
      input.value = String(binding.off);
      session.refreshDirty();
    });
    return el("label", { class: "field" }, "at zero", input);
  };

  const renderOutput = (): void => {
    output.replaceChildren();

    // An indicator acks 255 and lights nothing, which is not "off" and cost a
    // long detour once. There is no brightness to offer, only its on value.
    if (!led.dimmable) {
      output.append(el("span", { class: "meta" }, String(led.on_value)));
      return;
    }
    output.append(litCell());
    const atZero = atZeroCell();
    if (atZero) output.append(atZero);
  };
  renderOutput();

  const driven = el("td", {});
  driven.append(
    bindingEditor({
      binding,
      led,
      signals: session.signals,
      // Only dimmers, and never the lamp itself: an on/off lamp has no level to
      // follow, which is what the mirror copies. This panel's first, then every
      // other panel's, so one backlight can follow another's.
      //
      // A lamp that mirrors something itself is not offered either. Pointing at
      // one would build a chain, and a chain has no value to resolve: the
      // daemon rejects the profile rather than following it. The one already
      // chosen stays in the list whatever it is, so a chain that arrived in the
      // file can still be seen and changed rather than silently reassigned.
      //
      // A panel that takes its setup from another is left out: its own rows
      // are not in use, and its lamps are the ones it follows.
      targets: () => {
        if (!led.dimmable) return [];
        const on = binding.same_as_device ?? device.key;
        // A lamp something else already follows cannot follow in turn, or the
        // two would point round in a loop. Only while it is not matching yet,
        // so one that arrived that way in the file can still be changed.
        const followed = session.profile.bindings.some(
          (b) => b !== binding && b.same_as === led.name && (b.same_as_device ?? b.device) === device.key,
        );
        if (followed && !binding.same_as) return [];
        const panels = [device, ...all.filter((d) => d.key !== device.key)];
        return panels
          .filter((d) => d.key === device.key || session.profile.follows?.[d.key] === undefined)
          .flatMap((d) =>
            d.leds
              .filter(
                (l) =>
                  l.dimmable &&
                  !(d.key === device.key && l.name === led.name) &&
                  ((d.key === on && l.name === binding.same_as) || !byLamp.get(lampKey(d.key, l.name))?.same_as),
              )
              .map((l) => ({ device: d.key, deviceName: d.display_name, led: l })),
          );
      },
      deviceName: (key) => all.find((d) => d.key === key)?.display_name ?? key,
      shipped: session.shipped.get(lampKey(device.key, led.name)),
      onCommit: refreshCount,
      onChange: () => {
        session.refreshDirty();
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

/**
 * A bar along the foot of the window when a different release is out.
 *
 * Checked once per start and left alone after: it sits outside `#app`, so
 * moving between pages does not clear it. Offline, the check finds nothing
 * and nothing is shown.
 */
async function showUpdate(): Promise<void> {
  let update: Update | null;
  try {
    update = await updateCheck();
  } catch {
    return;
  }
  if (!update) return;
  const link = el("a", { href: "#" }, `Get ${update.latest}`);
  link.addEventListener("click", (e) => {
    e.preventDefault();
    openUpdate().catch((err: unknown) => showError("Opening the release page", err));
  });
  document.body.append(
    el(
      "div",
      { class: "update" },
      el("span", {}, `DCS Signal Converter ${update.latest} is out. This is ${update.current}.`),
      link,
    ),
  );
  document.body.classList.add("with-update");
}

/**
 * Closing the window is the one way out that never asked.
 *
 * The back button has always asked, so the way to lose an evening's work was
 * to close the window instead, which is how most people leave an app. Tauri
 * holds the window open for as long as this listener is registered and closes
 * it once the handler returns without objecting, so the question is asked
 * before anything goes.
 */
function guardClose(): void {
  const win = getCurrentWindow();
  // A second click on the close box while the question is up would stack a
  // second copy of it behind the first.
  let asking = false;
  void win.onCloseRequested(async (event) => {
    if (!unsavedWork()) return;
    if (asking) {
      event.preventDefault();
      return;
    }
    asking = true;
    try {
      if (!(await confirmAction("Close without saving? Your changes will be lost.", "Close"))) {
        event.preventDefault();
      }
    } finally {
      asking = false;
    }
  });
}

async function start(): Promise<void> {
  // Before anything is drawn, so a chosen theme does not flash the other one.
  await loadTheme();
  guardClose();
  void showUpdate();
  try {
    [devices, modules] = await Promise.all([listDevices(), listModules()]);
  } catch (e) {
    showError("Loading the hardware inventory", e);
    return;
  }
  await showLibrary();
}

void start();
