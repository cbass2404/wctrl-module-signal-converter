// Typed wrappers over the Rust commands. One place that knows command names, so
// a rename is a compile error here rather than an empty screen at runtime.

import { invoke } from "@tauri-apps/api/core";

import type { Device, ModuleChoice, Profile, ProfileSummary, SignalView } from "./types";

export const listDevices = () => invoke<Device[]>("devices");
export const listModules = () => invoke<ModuleChoice[]>("modules");
export const listProfiles = () => invoke<ProfileSummary[]>("profiles");
export const listSignals = (module: string) => invoke<SignalView[]>("signals", { module });

export const openProfile = (file: string) => invoke<Profile>("open_profile", { file });
export const defaultProfile = (file: string) => invoke<Profile | null>("default_profile", { file });
export const createProfile = (module: string) => invoke<string>("create_profile", { module });
export const saveProfile = (file: string, profile: Profile) =>
  invoke<void>("save_profile", { file, profile });
export const resetProfile = (file: string) => invoke<void>("reset_profile", { file });
