// The Settings dialog, behind the gear on the Profiles page.
//
// What belongs to the PC rather than to any profile: the window's theme and
// the key held to swap MCDU pages, both kept in settings.json, and the two
// things on this page that are not about one profile, Import and Manage
// Converter, moved here to keep the header short. Laid out like converter.ts.

import { settingsRead, settingsSave } from "./api";
import type { PageModifier, Settings, Theme } from "./types";

/** What the dialog closed on: nothing, or one of the dialogs it hands on to. */
export type SettingsExit = null | "import" | "converter";

/** Draw the window in a theme. System leaves it to Windows, as the stylesheet always did. */
export function applyTheme(theme: Theme): void {
  if (theme === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.dataset.theme = theme;
}

/** Read the settings and draw the window in their theme, before the first screen. */
export async function loadTheme(): Promise<void> {
  try {
    applyTheme((await settingsRead()).theme);
  } catch {
    // The window follows Windows, as it did before there was a choice.
  }
}

/** A labelled dropdown, as the pickers lay one out. */
function choice<T extends string>(label: string, options: [T, string][], value: T): [HTMLLabelElement, HTMLSelectElement] {
  const field = document.createElement("label");
  field.className = "field";
  field.append(label);
  const select = document.createElement("select");
  for (const [v, text] of options) {
    const option = document.createElement("option");
    option.value = v;
    option.textContent = text;
    select.append(option);
  }
  select.value = value;
  field.append(select);
  return [field, select];
}

function note(text: string): HTMLParagraphElement {
  const p = document.createElement("p");
  p.className = "meta";
  p.textContent = text;
  return p;
}

/**
 * Open the dialog. Resolves once it is closed, with the dialog to open next
 * if Import or Manage Converter was pressed.
 *
 * A change is saved as it is made, so there is no Save to forget: the theme
 * shows at once, and a running converter picks up the modifier within about a
 * second, as it picks up a saved profile.
 */
export function showSettings(): Promise<SettingsExit> {
  return new Promise((resolve) => {
    void settingsRead().then(
      (view) => open(view, view.problem, resolve),
      (e) => open({ page_modifier: "ctrl", theme: "system" }, String(e), resolve),
    );
  });
}

function open(current: Settings, problem: string | null | undefined, resolve: (exit: SettingsExit) => void): void {
  const settings: Settings = { page_modifier: current.page_modifier, theme: current.theme };
  const dialog = document.createElement("dialog");
  dialog.className = "picker confirm settings";

  const h2 = document.createElement("h2");
  h2.textContent = "Settings";
  dialog.append(h2);

  const state = note("");
  if (problem) {
    state.textContent = `The settings file could not be read, so these are the defaults. Changing one writes a new file. ${problem}`;
    state.classList.add("bad");
  }
  dialog.append(state);

  const save = (): void => {
    state.textContent = "";
    state.classList.remove("bad");
    void settingsSave(settings).catch((e: unknown) => {
      state.textContent = e instanceof Error ? e.message : String(e);
      state.classList.add("bad");
    });
  };

  const [themeField, theme] = choice<Theme>(
    "Appearance",
    [
      ["system", "Follow Windows"],
      ["light", "Light"],
      ["dark", "Dark"],
    ],
    settings.theme,
  );
  theme.addEventListener("change", () => {
    settings.theme = theme.value as Theme;
    applyTheme(settings.theme);
    save();
  });
  dialog.append(themeField);

  const [modifierField, modifier] = choice<PageModifier>(
    "MCDU page keys",
    [
      ["ctrl", "Ctrl + line select key"],
      ["shift", "Shift + line select key"],
      ["alt", "Alt + line select key"],
    ],
    settings.page_modifier,
  );
  modifier.addEventListener("change", () => {
    settings.page_modifier = modifier.value as PageModifier;
    save();
  });
  dialog.append(modifierField);
  dialog.append(
    note(
      "Hold it alone: with a second modifier held too, the press is left to DCS and nothing swaps. DCS still sees every press, so leave the combination you pick unbound in DCS, or one press will swap the page and do whatever it is bound to.",
    ),
  );

  const importButton = document.createElement("button");
  importButton.textContent = "Import profile...";
  const converterButton = document.createElement("button");
  converterButton.textContent = "Manage Converter...";
  const close = document.createElement("button");
  close.className = "primary";
  close.textContent = "Close";

  // The two hand-offs to the left, Close alone on the right where every
  // dialog's action sits.
  const actions = document.createElement("div");
  actions.className = "actions";
  actions.append(importButton, converterButton, close);
  dialog.append(actions);

  let done = false;
  const finish = (exit: SettingsExit): void => {
    if (done) return;
    done = true;
    dialog.close();
    dialog.remove();
    resolve(exit);
  };

  close.addEventListener("click", () => finish(null));
  importButton.addEventListener("click", () => finish("import"));
  converterButton.addEventListener("click", () => finish("converter"));
  dialog.addEventListener("close", () => finish(null));
  dialog.addEventListener("click", (e) => {
    if (e.target !== dialog) return;
    const r = dialog.getBoundingClientRect();
    const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
    if (!inside) finish(null);
  });

  document.body.append(dialog);
  dialog.showModal();
  close.focus();
}
