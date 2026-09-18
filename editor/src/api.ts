// Typed wrappers over the Rust commands. One place that knows command names, so
// a rename is a compile error here rather than an empty screen at runtime.

import { invoke } from "@tauri-apps/api/core";

import type {
  Device,
  LearnReport,
  ModuleChoice,
  Profile,
  ProfileSummary,
  SignalView,
} from "./types";

export const listDevices = () => invoke<Device[]>("devices");
export const listModules = () => invoke<ModuleChoice[]>("modules");
export const listProfiles = () => invoke<ProfileSummary[]>("profiles");
export const listSignals = (module: string) => invoke<SignalView[]>("signals", { module });

export const openProfile = (file: string) => invoke<Profile>("open_profile", { file });
export const defaultProfile = (file: string) => invoke<Profile | null>("default_profile", { file });
export const createProfile = (module: string) => invoke<string>("create_profile", { module });
export const saveProfile = (file: string, profile: Profile) =>
  invoke<void>("save_profile", { file, profile });
/**
 * Everything the daemon would refuse this profile for, in its own words. An
 * empty list means it will load. Run after each edit, not only on save.
 */
export const checkProfile = (profile: Profile) => invoke<string[]>("check_profile", { profile });
export const resetProfile = (file: string) => invoke<void>("reset_profile", { file });
export const cloneProfile = (file: string, name: string, aircraft: string[]) =>
  invoke<string>("clone_profile", { file, name, aircraft });

// Learn mode. The only commands that touch the DCS-BIOS stream, and the only
// ones that leave anything running in the backend between calls.
export const learnStart = (module: string) => invoke<void>("learn_start", { module });
export const learnPoll = () => invoke<LearnReport>("learn_poll");
export const learnAgain = () => invoke<void>("learn_again");
export const learnStop = () => invoke<void>("learn_stop");
