// Typed wrappers over the Rust commands. One place that knows command names, so
// a rename is a compile error here rather than an empty screen at runtime.

import { invoke } from "@tauri-apps/api/core";

import type {
  CatalogueStatus,
  CellDraw,
  CellInk,
  ConverterState,
  Device,
  ExportPage,
  Findings,
  FontGlyphs,
  ImportPreview,
  LearnReport,
  MergeParts,
  MergePick,
  MergeReport,
  MergeSource,
  ModuleChoice,
  Page,
  PageTake,
  PagesView,
  Profile,
  ProfileSummary,
  RuleCell,
  Settings,
  SettingsView,
  SignalView,
  Update,
} from "./types";

/** Whether the catalogue matched DCS-BIOS at startup, or was rebuilt, or why not. */
export const catalogueStatus = () => invoke<CatalogueStatus>("catalogue_status");
export const listDevices = () => invoke<Device[]>("devices");
export const connectedDevices = () => invoke<string[]>("connected_devices");
export const listModules = () => invoke<ModuleChoice[]>("modules");
export const listProfiles = () => invoke<ProfileSummary[]>("profiles");
export const listSignals = (module: string) => invoke<SignalView[]>("signals", { module });
/**
 * What a divider of this many cells will draw, one cell at a time.
 *
 * Asked of the backend rather than worked out here, so the preview cannot
 * drift from the rule the panel is actually sent. Per cell rather than as one
 * string, because a label is drawn in its own colour and the preview has to
 * know which cells are it.
 */
export const dividerRule = (cells: number, label: string) =>
  invoke<RuleCell[]>("divider_rule", { cells, label });

/**
 * One font's glyphs, for drawing a line the way the panel will draw it.
 *
 * Asked for per font rather than sent with the displays, because four fonts of
 * bitmaps is a great deal of data to hand over for a screen nobody may open.
 * Cached by the caller, since a font never changes while the window is up.
 */
export const fontGlyphs = (display: string, font: string) =>
  invoke<FontGlyphs>("font_glyphs", { display, font });

/**
 * What each of these cells would light, drawing these values.
 *
 * For glass that draws from a glyph table rather than from a font: the UFC's
 * segments and the DED's pixels. Asked of the backend rather than worked out
 * here for the same reason a divider is, and more so: which glyph a value
 * lands on depends on the cell it is drawn in, and no two of those cells agree.
 */
export const cellInk = (display: string, cells: CellDraw[]) =>
  invoke<CellInk[]>("cell_ink", { display, cells });

// The converter daemon. Nothing here is needed to edit a profile: a running
// daemon picks up a saved one on its own. See editor/src/converter.ts.
export const converterState = () => invoke<ConverterState>("converter_state");
/** Stops a running converter, waits for it to clear the panels, starts a fresh one. */
export const converterRestart = () => invoke<string>("converter_restart");
/** Ends it without asking, for one that will not answer. Clears no panels. */
export const converterKill = () => invoke<string>("converter_kill");

// The PC's own settings. A running converter picks up a saved change itself.
export const settingsRead = () => invoke<SettingsView>("settings_read");
export const settingsSave = (settings: Settings) => invoke<void>("settings_save", { settings });

export const openProfile = (file: string) => invoke<Profile>("open_profile", { file });
export const defaultProfile = (file: string) => invoke<Profile | null>("default_profile", { file });
/**
 * A new profile for some of a module's aircraft, blank or copied from `from`.
 * Chosen aircraft another profile claims move to the new one.
 */
export const createProfile = (module: string, name: string, aircraft: string[], from: string | null) =>
  invoke<string>("create_profile", { module, name, aircraft, from });
/** Write the profile. Its pages are saved on their own, by `savePage`. */
export const saveProfile = (file: string, profile: Profile) =>
  invoke<void>("save_profile", { file, profile });
/**
 * Everything the daemon would refuse this profile for, in its own words, and
 * everything it would caution about. No problems means it will load. Run after
 * each edit, not only on save. `working` is the page open for editing on
 * `device`, if one is: its rows are marked, and why it could not be saved is
 * said apart from the profile's own problems.
 */
export const checkProfile = (profile: Profile, working: Page | null, device: string | null) =>
  invoke<Findings>("check_profile", { profile, working, device });
/** One module's pages, and where the saved profiles use them. */
export const openPages = (module: string) => invoke<PagesView>("open_pages", { module });
/** An id no page has, nor any of `avoid`, the new pages not saved yet. */
export const newPageId = (avoid: string[]) => invoke<string>("new_page_id", { avoid });
/**
 * Save one page to the library, checked as `device` of `profile` would draw
 * it. Returns the module's pages as they now are.
 */
export const savePage = (profile: Profile, page: Page, device: string) =>
  invoke<PagesView>("save_page", { profile, page, device });
/**
 * Delete a page from the library, emptying its slots in every other saved
 * profile on the module, which it names. The open profile's slots are the
 * window's to empty.
 */
export const deletePage = (module: string, id: string, current: string) =>
  invoke<[PagesView, string[]]>("delete_page", { module, id, current });
export const resetProfile = (file: string) => invoke<void>("reset_profile", { file });
/**
 * Delete a profile, first giving its aircraft to `giveTo` if one is named. It
 * must read the same module. A shipped profile whose aircraft would go nowhere
 * is refused, since it would be seeded straight back.
 */
export const deleteProfile = (file: string, giveTo: string | null) =>
  invoke<void>("delete_profile", { file, giveTo });
export const cloneProfile = (file: string, name: string, aircraft: string[]) =>
  invoke<string>("clone_profile", { file, name, aircraft });

// Sharing. The backend runs the file dialogs; the window is allowed none.
/**
 * Where the profile was saved, with the pages its slots show and those in
 * `also`, or null if the dialog was cancelled.
 */
export const exportProfile = (file: string, also: string[]) =>
  invoke<string | null>("export_profile", { file, also });
/** Every page on a profile's module, for choosing which go with an export. */
export const exportPages = (file: string) => invoke<ExportPage[]>("export_pages", { file });
/** Asks for a file and checks it. Null if the dialog was cancelled; refused if it would not load. */
export const importPick = () => invoke<ImportPreview | null>("import_pick");
/**
 * Write the picked profile under `name` for `aircraft`, which must be some of
 * those it came with. Aircraft other profiles fly move to it. A profile left
 * with none is deleted only if `remove` names it, which the user confirms first.
 */
export const importProfile = (path: string, name: string, aircraft: string[], remove: string[], pages: PageTake[]) =>
  invoke<string>("import_profile", { path, name, aircraft, delete: remove, pages });

/** What the profile in `file` could give another on its module. */
export const mergeParts = (file: string) => invoke<MergeParts>("merge_parts", { file });
/**
 * Take the picked lamps and lines from `from` into the profile `into`. With
 * `write` false nothing is saved, and the report says what it would do.
 * Refused if the result would not load.
 */
export const mergeProfile = (from: MergeSource, into: string, pick: MergePick, write: boolean) =>
  invoke<MergeReport>("merge_profile", { from, into, pick, write });

// Learn mode. The only commands that touch the DCS-BIOS stream, and the only
// ones that leave anything running in the backend between calls.
export const learnStart = (module: string) => invoke<void>("learn_start", { module });
export const learnPoll = () => invoke<LearnReport>("learn_poll");
export const learnAgain = () => invoke<void>("learn_again");
export const learnStop = () => invoke<void>("learn_stop");

// Whether a different release is out. Null when this is the newest, and also
// when the check could not be made, offline most often: no answer, no banner.
export const updateCheck = () => invoke<Update | null>("update_check");
/** Opens the release the check found; the backend holds its address. */
export const openUpdate = () => invoke<void>("open_update");
