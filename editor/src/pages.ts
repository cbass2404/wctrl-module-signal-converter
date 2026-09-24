// Pages: which page each of a screen's slots shows, and the page editor.
//
// A screen takes every field from a page, whatever its glass, and a page belongs to the library
// for its module rather than to the profile open here. So this section is two
// things kept apart. The slots are the profile's, and the profile's Save
// writes them. The page editor opens only when asked, on a page picked or a
// new one, and Save page writes the page to the library, where every profile
// on the module showing it sees the change.

import { deletePage, newPageId, savePage } from "./api";
import { confirmAction } from "./confirm";
import { fieldTable, fontPicker } from "./readout";
import type { Device, DisplayInfo, Page, PageSlots, PageUse, PagesView, Profile, SignalView } from "./types";

/** How many slots a screen has: one per page key the device lists. */
export function slotCount(device: Device): number {
  return Math.max(device.page_keys.length, 1);
}

// A slot's choice in its menu, besides a page's id. Page ids are letters and
// digits, so neither can be taken for one.
const DISABLED = "-disabled";
const BLANK = "-blank";

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

/** The page open for editing, if one is. */
interface Editing {
  /** A copy, so nothing saved changes until Save page. */
  page: Page;
  /** The device whose screen section shows the editor. */
  device: string;
  /** The page as it was opened, to tell an edit from none. */
  baseline: string;
  /** Not in the library yet. */
  fresh: boolean;
}

/** One module's pages, as the window holds them while a profile is open. */
export interface PageBook {
  module: string;
  /** The open profile's file, whose own slots are read live rather than as saved. */
  file: string;
  /** The module's pages as saved. */
  saved: Page[];
  /** Why the module's page file would not load, if it would not. */
  broken: string | null;
  /** Every other saved profile's slots on the module. */
  used: PageUse[];
  editing: Editing | null;
  /** Why the page being edited could not be saved, from the last check. */
  problems: string[];
  /** Every screen section's redraw. */
  sections: (() => void)[];
  /** Where each section shows `problems`, refreshed after every check. */
  problemViews: (() => void)[];
}

/** What a screen section needs from the profile page around it. */
export interface PageContext {
  profile: Profile;
  signals: SignalView[];
  book: PageBook;
  /** The profile's slots changed. */
  profileChanged: () => void;
  /** The page being edited changed. */
  pageChanged: () => void;
  /** A device's name as the window shows it. */
  nameOf: (key: string) => string;
  /** Say something that happened, such as a save. */
  tell: (text: string) => void;
  /** Say something failed. */
  fail: (where: string, e: unknown) => void;
}

export function pageBook(module: string, file: string, view: PagesView): PageBook {
  return {
    module,
    file,
    saved: view.pages,
    broken: view.broken,
    used: view.used.filter((u) => u.file !== file),
    editing: null,
    problems: [],
    sections: [],
    problemViews: [],
  };
}

/** Whether the page open for editing has changes Save page has not written. */
export function pageUnsaved(book: PageBook): boolean {
  const e = book.editing;
  return e !== null && (e.fresh || JSON.stringify(e.page) !== e.baseline);
}

/**
 * The pages a check's findings are placed against: as saved, with the one
 * being edited in place of its saved self. The working copy itself, not a
 * clone, since a mark is found by the field object its row was drawn from.
 */
export function pagesChecked(book: PageBook): Page[] {
  const e = book.editing;
  if (!e) return book.saved;
  return [...book.saved.filter((p) => p.id !== e.page.id), e.page];
}

/** Take in what a save or a delete says the library now holds. */
function update(book: PageBook, view: PagesView): void {
  book.saved = view.pages;
  book.broken = view.broken;
  book.used = view.used.filter((u) => u.file !== book.file);
}

function redrawAll(book: PageBook): void {
  for (const draw of book.sections) draw();
}

const sameName = (a: string, b: string): boolean => a.trim().toLowerCase() === b.trim().toLowerCase();

/** A name on the module no saved page but `except` has: `name`, or `name 2`, `name 3` and so on. */
function freeName(book: PageBook, name: string, except?: string): string {
  const taken = (n: string): boolean => book.saved.some((p) => p.id !== except && sameName(p.name, n));
  const base = name.trim() || "New page";
  if (!taken(base)) return base;
  for (let k = 2; ; k += 1) {
    if (!taken(`${base} ${k}`)) return `${base} ${k}`;
  }
}

/** This device's slots, as the profile holds them, or every one empty. */
function slotsOf(profile: Profile, device: Device): PageSlots {
  return profile.screens?.[device.key] ?? { slots: Array<null>(slotCount(device)).fill(null) };
}

/**
 * Put a device's slots back on the profile the way the backend writes them:
 * `start` on a filled slot, before `slots`, and no entry at all with every
 * slot empty. The order matters because the unsaved marker compares text, and
 * a key that moved would read as an edit that was never made.
 */
function storeSlots(profile: Profile, device: string, s: PageSlots): void {
  const filled = s.slots.map((slot, i) => (slot ? i + 1 : 0)).filter((i) => i > 0);
  const screens = profile.screens ?? {};
  if (filled.length === 0) {
    delete screens[device];
  } else {
    const start = s.start !== undefined && filled.includes(s.start) ? s.start : filled[0];
    screens[device] = { start, slots: s.slots };
  }
  if (Object.keys(screens).length > 0) profile.screens = screens;
  else delete profile.screens;
}

/** Where a page is shown, this profile's slots read as they stand. */
function shownAt(ctx: PageContext, id: string): string[] {
  const here: string[] = [];
  for (const [device, s] of Object.entries(ctx.profile.screens ?? {})) {
    if (ctx.profile.follows?.[device]) continue;
    s.slots.forEach((slot, i) => {
      if (slot?.page === id) here.push(`${ctx.nameOf(device)} slot ${i + 1} here`);
    });
  }
  const there = ctx.book.used
    .filter((u) => u.page === id)
    .map((u) => `${u.profile}, ${ctx.nameOf(u.device)} slot ${u.slot}`);
  return [...here, ...there];
}

/**
 * One screen's pages: its slots and which one starts, and the page
 * editor when a page has been opened on this screen.
 */
export function pageSection(device: Device, display: DisplayInfo, ctx: PageContext): HTMLElement {
  const box = el("div", { class: "display pages" });
  const book = ctx.book;

  const draw = (): void => {
    const s = slotsOf(ctx.profile, device);
    const editingHere = book.editing?.device === device.key ? book.editing : null;
    const busy = book.editing !== null;

    // The head names the screen and, on a text grid where the aircraft has
    // no font of its own, offers the profile's, since every page here is
    // drawn in it.
    const head = el("div", { class: "display-head" }, el("span", { class: "name" }, `${display.key} pages`));
    const start = s.start !== undefined ? s.slots[s.start - 1] : null;
    const startName = !start
      ? null
      : start.page === null
        ? "a blank screen"
        : (book.saved.find((p) => p.id === start.page)?.name ?? "a missing page");
    head.append(
      el("span", { class: "meta" }, startName ? `starts on slot ${s.start}, ${startName}` : "every slot disabled, so the screen is blank"),
    );
    const picker = fontPicker(display, ctx.profile, () => {
      draw();
      ctx.profileChanged();
    });
    if (picker) head.append(picker);

    if (book.broken) {
      box.replaceChildren(
        head,
        el(
          "div",
          { class: "problems" },
          `The page file for ${book.module} would not load, so every slot on it is empty and its pages cannot be edited until it is fixed: ${book.broken}`,
        ),
      );
      return;
    }

    // One slot per page key. A page picked here is shown on this screen by
    // this profile; which page starts is the one ticked. Disabled and blank
    // differ in what the slot's key does on the panel: nothing, or take the
    // screen dark.
    // Only pages drawn on this screen: a UFC page means nothing on the MCDU.
    const here = book.saved
      .filter((p) => p.display === display.key)
      .sort((a, b) => a.name.localeCompare(b.name));
    const body = el("tbody");
    const count = slotCount(device);
    for (let i = 0; i < count; i += 1) {
      const slot = s.slots[i] ?? null;
      const choose = el("select", { class: "test" }) as HTMLSelectElement;
      choose.append(el("option", { value: DISABLED }, "Disabled"), el("option", { value: BLANK }, "Blank"));
      for (const p of here) choose.append(el("option", { value: p.id }, p.name));
      // A slot already pointing somewhere this screen cannot show is kept on
      // its own line, so the file's mistake is visible rather than hidden.
      const missing = slot?.page ?? null;
      if (missing !== null && !here.some((p) => p.id === missing)) {
        const other = book.saved.find((p) => p.id === missing);
        choose.append(
          el(
            "option",
            { value: missing },
            other ? `${other.name}, a page for the ${other.display}` : `a page not in the library (${missing})`,
          ),
        );
      }
      choose.value = slot === null ? DISABLED : (slot.page ?? BLANK);
      choose.addEventListener("change", () => {
        const next = slotsOf(ctx.profile, device);
        const slots = [...next.slots];
        while (slots.length < count) slots.push(null);
        slots[i] =
          choose.value === DISABLED
            ? null
            : { page: choose.value === BLANK ? null : choose.value, key: null };
        storeSlots(ctx.profile, device.key, { start: next.start, slots });
        ctx.profileChanged();
        redrawAll(book);
      });

      const tick = el("input", { type: "radio", name: `start-${device.key}` }) as HTMLInputElement;
      tick.checked = slot !== null && s.start === i + 1;
      tick.disabled = slot === null;
      tick.title = "Show this slot when a mission starts";
      tick.addEventListener("change", () => {
        storeSlots(ctx.profile, device.key, { start: i + 1, slots: slotsOf(ctx.profile, device).slots });
        ctx.profileChanged();
        redrawAll(book);
      });

      body.append(
        el(
          "tr",
          {},
          // Named with the key that brings it up on the panel, from the
          // device's own list; a device with none has one slot and no key.
          el(
            "td",
            {},
            el("span", { class: "region-name" }, `Slot ${i + 1}`),
            el("div", { class: "meta" }, device.page_keys[i] ?? "no key"),
          ),
          el("td", {}, choose),
          el("td", { class: "num" }, el("label", { class: "meta" }, tick, " start")),
        ),
      );
    }
    const slots = el(
      "table",
      { class: "readouts" },
      el("thead", {}, el("tr", {}, el("th", {}, "Slot"), el("th", {}, "Page"), el("th", { class: "num" }, ""))),
      body,
    );

    // Opening a page, or making one. Held while a page is open anywhere, so
    // an edit cannot be dropped by opening another over it.
    const pick = el("select", { class: "test" }) as HTMLSelectElement;
    for (const p of here) pick.append(el("option", { value: p.id }, p.name));
    const startPage = start?.page ?? null;
    if (startPage !== null && here.some((p) => p.id === startPage)) pick.value = startPage;
    const open = el("button", { class: "add" }, "Edit page");
    open.addEventListener("click", () => {
      const page = book.saved.find((p) => p.id === pick.value);
      if (page) edit(structuredClone(page), false);
    });
    const make = el("button", { class: "add" }, "+ New page");
    make.addEventListener("click", () => {
      void (async () => {
        try {
          const id = await newPageId([]);
          edit({ id, name: freeName(book, "New page"), display: display.key, fields: [] }, true);
        } catch (e) {
          ctx.fail("Making a page", e);
        }
      })();
    });
    if (here.length === 0) {
      pick.disabled = true;
      open.setAttribute("disabled", "");
    }
    if (busy) {
      for (const b of [pick, open, make]) b.setAttribute("disabled", "");
    }
    const actions = el("div", { class: "chain-add" }, pick, open, make);

    box.replaceChildren(head, slots, actions);
    if (editingHere) box.append(editor(editingHere));
  };

  /** Open `page` in the editor on this screen. */
  const edit = (page: Page, fresh: boolean): void => {
    // The field rows compare a field's device with its neighbours', so each
    // is put on this one. The page keeps none; Save page takes it off again.
    for (const f of page.fields) f.device = device.key;
    book.editing = { page, device: device.key, baseline: JSON.stringify(page), fresh };
    book.problems = [];
    redrawAll(book);
    ctx.pageChanged();
  };

  /**
   * Take `page` as saved and keep it open, so the editor stays where it was;
   * only Close shuts it.
   */
  const saved = (page: Page): void => {
    book.editing = { page, device: device.key, baseline: JSON.stringify(page), fresh: false };
    redrawAll(book);
    ctx.pageChanged();
  };

  const close = (): void => {
    book.editing = null;
    book.problems = [];
    book.problemViews = [];
    redrawAll(book);
    ctx.pageChanged();
  };

  /** The page editor: its name, what saving it touches, and its fields. */
  const editor = (e: Editing): HTMLElement => {
    const page = e.page;
    const wrap = el("div", { class: "page-edit" });

    const name = el("input", { type: "text", class: "rename", value: page.name }) as HTMLInputElement;
    const clash = el("span", { class: "bad" });
    const save = el("button", { class: "primary" }, "Save page");
    const saveAs = el("button", {}, "Save as new page");
    const remove = el("button", { class: "danger" }, "Delete page");
    const shut = el("button", {}, "Close");
    const state = el("span", { class: "meta" });
    const problems = el("div", { class: "problems", hidden: "" });

    const nameTaken = (n: string): boolean => book.saved.some((p) => p.id !== page.id && sameName(p.name, n));
    const refresh = (): void => {
      const n = page.name.trim();
      clash.textContent = n === "" ? "A page needs a name." : nameTaken(n) ? `Another page on ${book.module} is called ${n}.` : "";
      const blocked = clash.textContent !== "" || book.problems.length > 0;
      if (blocked || !pageUnsaved(book)) save.setAttribute("disabled", "");
      else save.removeAttribute("disabled");
      state.textContent = e.fresh ? "new, not saved yet" : pageUnsaved(book) ? "unsaved changes" : "";
      problems.replaceChildren();
      problems.hidden = book.problems.length === 0;
      if (book.problems.length > 0) {
        problems.append(el("strong", {}, "This page cannot be saved until this is fixed:"));
        for (const p of book.problems) problems.append(el("div", { class: "problem" }, p));
      }
    };
    book.problemViews = [refresh];

    name.addEventListener("input", () => {
      page.name = name.value;
      refresh();
      ctx.pageChanged();
    });

    const where = shownAt(ctx, page.id);
    const reach = e.fresh
      ? "Not in any slot until it is saved and picked for one."
      : where.length === 0
        ? "Not shown in any slot."
        : `Saving changes it everywhere it is shown: ${where.join("; ")}.`;

    save.addEventListener("click", () => {
      void (async () => {
        try {
          page.name = page.name.trim();
          update(book, await savePage(ctx.profile, page, device.key));
          ctx.tell(`Saved page ${page.name}.`);
          saved(page);
        } catch (err) {
          ctx.fail("Saving the page", err);
        }
      })();
    });

    // A copy under a new id, leaving the page it came from as saved. The
    // name typed is kept if it is free, which is the usual way to name it.
    // The editor carries on with the copy.
    saveAs.addEventListener("click", () => {
      void (async () => {
        try {
          const id = await newPageId([]);
          const copy: Page = { ...structuredClone(page), id, name: freeName(book, page.name) };
          update(book, await savePage(ctx.profile, copy, device.key));
          ctx.tell(`Saved as a new page, ${copy.name}. Pick it in a slot to show it.`);
          saved(copy);
        } catch (err) {
          ctx.fail("Saving the page", err);
        }
      })();
    });

    // Deleting empties every slot showing the page, here and in every other
    // profile on the module, so the question lists them first.
    remove.addEventListener("click", () => {
      void (async () => {
        const places = shownAt(ctx, page.id);
        const question =
          `Delete the page ${page.name}?` +
          (places.length > 0 ? `\n\nThese slots show it and will be emptied: ${places.join("; ")}.` : "") +
          "\n\nThis cannot be undone.";
        if (!(await confirmAction(question, "Delete"))) return;
        try {
          const [view, touched] = await deletePage(book.module, page.id, book.file);
          update(book, view);
          for (const [dev, s] of Object.entries(ctx.profile.screens ?? {})) {
            const slots = s.slots.map((slot) => (slot?.page === page.id ? null : slot));
            storeSlots(ctx.profile, dev, { start: s.start, slots });
          }
          ctx.profileChanged();
          ctx.tell(
            touched.length > 0
              ? `Deleted page ${page.name}, and emptied its slots in ${touched.join(", ")}.`
              : `Deleted page ${page.name}.`,
          );
          close();
        } catch (err) {
          ctx.fail("Deleting the page", err);
        }
      })();
    });
    if (e.fresh) remove.setAttribute("disabled", "");

    shut.addEventListener("click", () => {
      void (async () => {
        if (pageUnsaved(book) && !(await confirmAction(`Close ${page.name} without saving? The changes to it will be lost.`, "Close"))) return;
        close();
      })();
    });

    const { table } = fieldTable(device, display, ctx.profile, page.fields, false, ctx.signals, () => {
      refresh();
      ctx.pageChanged();
    }, []);

    wrap.append(
      el(
        "div",
        { class: "display-head" },
        el("label", { class: "meta" }, "Page ", name),
        state,
      ),
      el("div", { class: "meta block" }, reach),
      clash,
      problems,
      table,
      el("div", { class: "chain-add page-actions" }, save, saveAs, remove, shut),
    );
    refresh();
    return wrap;
  };

  book.sections.push(draw);
  draw();
  return box;
}

/** Show the latest check's page problems in the open editor. */
export function showPageProblems(book: PageBook, problems: string[]): void {
  book.problems = problems;
  for (const view of book.problemViews) view();
}
