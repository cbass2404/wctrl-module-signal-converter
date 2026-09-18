// The window. Two screens: the profile library, and one profile being edited.
//
// No framework. The editor is a fixed list of lamp rows with a search box on
// each, which is not enough state to be worth a runtime, and a smaller install
// matters for something that ships next to a daemon.

import { displaySection } from "./readout";
import {
  checkProfile,
  cloneProfile,
  createProfile,
  defaultProfile,
  listDevices,
  listModules,
  listProfiles,
  listSignals,
  openProfile,
  resetProfile,
  deleteProfile,
  saveProfile,
} from "./api";
import { bindingEditor } from "./binding";
import { confirmAction } from "./confirm";
import { setLearnContext, stopLearning } from "./learn";
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

/** Failures are shown in the window, never swallowed and never only in a console. */
function showError(where: string, e: unknown): void {
  const message = e instanceof Error ? e.message : String(e);
  app.prepend(el("div", { class: "error" }, `${where}: ${message}`));
}

// ------------------------------------------------------------------- library

async function showLibrary(): Promise<void> {
  // Nothing on the profile list can use the stream, so the socket goes with it.
  stopLearning();

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
      actions.append(copy);
    }
    if (row.has_default) {
      const reset = el("button", { class: "danger" }, "Reset");
      reset.addEventListener("click", () => void resetOne(row));
      actions.append(reset);
    } else {
      // One the user made. A shipped profile would only be seeded back.
      const remove = el("button", { class: "danger" }, "Delete");
      remove.addEventListener("click", () => void deleteOne(row));
      actions.append(remove);
    }

    list.append(
      el("li", {}, el("div", { class: "grow" }, el("strong", {}, row.name), el("br"), meta), actions),
    );
  }
  app.append(list);
}

/** Reset discards the user's work, so it asks first and says exactly what it does. */
async function resetOne(row: ProfileSummary): Promise<void> {
  const ok = await confirmAction(
    `Replace ${row.name} with the profile that shipped with wctrl?\n\n` +
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

/** Delete cannot be undone, so it asks first, the same way Reset does. */
async function deleteOne(row: ProfileSummary): Promise<void> {
  const ok = await confirmAction(
    `Delete ${row.name}?\n\n` +
      `The file is removed and cannot be recovered. Any other profile that lists ` +
      `the same aircraft takes over from it.`,
    "Delete",
  );
  if (!ok) return;
  try {
    await deleteProfile(row.file);
    await showLibrary();
  } catch (e) {
    showError("Deleting the profile", e);
  }
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
   * Re-run the daemon's own checks over the profile as it stands.
   *
   * Called from `refreshDirty`, so every edit is checked without each call
   * site having to remember to. Debounced, because it crosses into the backend
   * and an edit can be a keystroke.
   */
  recheck: () => void;
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
        const asked = JSON.stringify(session.profile);
        void checkProfile(session.profile)
          .then((found) => {
            if (JSON.stringify(session.profile) !== asked) return;
            problems = found.problems;
            cautions = found.cautions;
            drawProblems();
            refreshSave();
          })
          .catch((e: unknown) => {
            // A check that cannot run must not read as a profile with nothing
            // wrong, so the failure takes the same place the problems do.
            problems = [`The profile could not be checked: ${e instanceof Error ? e.message : String(e)}`];
            cautions = [];
            drawProblems();
            refreshSave();
          });
      }, 250);
    },
  };
  save.setAttribute("disabled", "");

  const back = el("button", {}, "← Profiles");
  back.addEventListener("click", () => {
    void (async () => {
      if (session.dirty && !(await confirmAction("Leave without saving? Your changes will be lost.", "Leave"))) return;
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
        el("h1", {}, profile.name),
        el("span", { class: "meta block", ...aircraft.title }, `${profile.module} · ${aircraft.text}`),
      ),
      state,
      toggle,
      save,
    ),
  );
  app.append(problemList, cautionList);

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
  // A panel left alone does not open. Its lamps and fields are kept, so ticking
  // it again brings back exactly what was set up, but there is nothing to edit
  // on a panel that will not be driven.
  const applyDriveState = (): void => {
    section.classList.toggle("off", !drive.checked);
    if (!drive.checked) section.open = false;
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
    el("label", { class: "drive meta" }, drive, " drive this panel"),
  );
  summary.addEventListener("click", (e) => {
    if (!drive.checked && !(e.target as Element).closest("label")) e.preventDefault();
  });
  // Anything that opens it some other way is closed again.
  section.addEventListener("toggle", () => {
    if (section.open && !drive.checked) section.open = false;
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

  if (!session.profile.readouts) session.profile.readouts = [];
  const glass = displaySection(
    device,
    session.profile.readouts,
    session.signals,
    session.refreshDirty,
  );

  section.append(summary, table);
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
/** Names a lamp but drives nothing yet. Mirrors `Binding::is_placeholder`. */
function isPlaceholder(binding: Binding): boolean {
  return !binding.conditions.length && !binding.any_of?.length && !binding.always && !binding.same_as;
}

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
      // follow, which is what the mirror copies.
      //
      // A lamp that mirrors something itself is not offered either. Pointing at
      // one would build a chain, and a chain has no value to resolve: the
      // daemon rejects the profile rather than following it. The one already
      // chosen stays in the list whatever it is, so a chain that arrived in the
      // file can still be seen and changed rather than silently reassigned.
      siblings: led.dimmable
        ? device.leds.filter(
            (l) =>
              l.dimmable &&
              l.name !== led.name &&
              (l.name === binding.same_as || !byLamp.get(lampKey(device.key, l.name))?.same_as),
          )
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
