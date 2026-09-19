// Typed wrappers over the Rust commands. One place that knows command names, so
// a rename is a compile error here rather than an empty screen at runtime.

import { invoke } from "@tauri-apps/api/core";

import type {
  CatalogueStatus,
  ConverterState,
  Device,
  Findings,
  ImportPreview,
  LearnReport,
  ModuleChoice,
  Profile,
  ProfileSummary,
  SignalView,
  Update,
} from "./types";

/** Whether the catalogue matched DCS-BIOS at startup, or was rebuilt, or why not. */
export const catalogueStatus = () => invoke<CatalogueStatus>("catalogue_status");
export const listDevices = () => invoke<Device[]>("devices");
export const listModules = () => invoke<ModuleChoice[]>("modules");
export const listProfiles = () => invoke<ProfileSummary[]>("profiles");
export const listSignals = (module: string) => invoke<SignalView[]>("signals", { module });
/**
 * What a divider of this many cells will draw.
 *
 * Asked of the backend rather than worked out here, so the preview cannot
 * drift from the rule the panel is actually sent.
 */
export const dividerRule = (cells: number) => invoke<string>("divider_rule", { cells });

// The converter daemon. Nothing here is needed to edit a profile: a running
// daemon picks up a saved one on its own. See editor/src/converter.ts.
export const converterState = () => invoke<ConverterState>("converter_state");
/** Stops a running converter, waits for it to clear the panels, starts a fresh one. */
export const converterRestart = () => invoke<string>("converter_restart");
/** Ends it without asking, for one that will not answer. Clears no panels. */
export const converterKill = () => invoke<string>("converter_kill");

export const openProfile = (file: string) => invoke<Profile>("open_profile", { file });
export const defaultProfile = (file: string) => invoke<Profile | null>("default_profile", { file });
/**
 * A new profile for some of a module's aircraft, blank or copied from `from`.
 * Chosen aircraft another profile claims move to the new one.
 */
export const createProfile = (module: string, name: string, aircraft: string[], from: string | null) =>
  invoke<string>("create_profile", { module, name, aircraft, from });
export const saveProfile = (file: string, profile: Profile) =>
  invoke<void>("save_profile", { file, profile });
/**
 * Everything the daemon would refuse this profile for, in its own words, and
 * everything it would caution about. No problems means it will load. Run after
 * each edit, not only on save.
 */
export const checkProfile = (profile: Profile) => invoke<Findings>("check_profile", { profile });
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
/** Where the profile was saved, or null if the dialog was cancelled. */
export const exportProfile = (file: string) => invoke<string | null>("export_profile", { file });
/** Asks for a file and checks it. Null if the dialog was cancelled; refused if it would not load. */
export const importPick = () => invoke<ImportPreview | null>("import_pick");
/**
 * Write the picked profile under `name` for `aircraft`, which must be some of
 * those it came with. Aircraft other profiles fly move to it. A profile left
 * with none is deleted only if `remove` names it, which the user confirms first.
 */
export const importProfile = (path: string, name: string, aircraft: string[], remove: string[]) =>
  invoke<string>("import_profile", { path, name, aircraft, delete: remove });

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
