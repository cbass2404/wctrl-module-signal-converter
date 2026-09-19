// Managing the converter daemon, from a dialog the Profiles page opens.
//
// Deliberately behind one neutral button rather than offered as a bare
// "Restart". Nothing here is needed to edit a profile, and a restart drops the
// panels for a moment, so the reasons to press it are listed before the buttons
// that do it. Laid out like confirm.ts, which is the same shape of thing.

import { converterKill, converterRestart, converterState } from "./api";

/** Why someone would actually need this, in the order they come up. */
const REASONS = [
  "The converter stopped while DCS stayed running. The DCS hook starts one when a mission begins and never learns that it has gone, so it will not come back until DCS is restarted.",
  "A panel was plugged in after it started. Devices are found once, at startup.",
  "DCS-BIOS was updated and the signal catalogue was rebuilt underneath it.",
];

/**
 * Open the dialog. Resolves once it is closed, with the line to show in a
 * banner, or null if nothing was done.
 *
 * The state is read when it opens rather than held, because the hook or a
 * mission start can change it at any moment and a stale "running" would send
 * someone to Kill something that is already gone.
 */
export function manageConverter(): Promise<string | null> {
  return new Promise((resolve) => {
    const dialog = document.createElement("dialog");
    dialog.className = "picker confirm converter";

    const h2 = document.createElement("h2");
    h2.textContent = "Manage Converter";
    dialog.append(h2);

    const state = document.createElement("p");
    state.className = "meta";
    state.textContent = "Checking...";
    dialog.append(state);

    const lead = document.createElement("p");
    lead.className = "meta";
    lead.textContent = "Saving a profile does not need this: a running converter picks up a saved profile within about a second. Restart it when one of these is true.";
    dialog.append(lead);

    const why = document.createElement("ul");
    why.className = "meta reasons";
    for (const reason of REASONS) {
      const li = document.createElement("li");
      li.textContent = reason;
      why.append(li);
    }
    dialog.append(why);

    const danger = document.createElement("p");
    danger.className = "meta";
    danger.textContent = "Kill ends it without asking, for one that will not answer. It cannot clear the panels: the lamps latch, and a killed process runs none of its shutdown, so whatever is lit stays lit. Start it again and stop it properly to clear them.";
    dialog.append(danger);

    const cancel = document.createElement("button");
    cancel.textContent = "Cancel";
    const kill = document.createElement("button");
    kill.className = "danger";
    kill.textContent = "Kill";
    const restart = document.createElement("button");
    restart.className = "primary";
    restart.textContent = "Restart";

    // Kill is pushed to the far left by the stylesheet, away from Restart. It
    // is the one button here that cannot be undone and cannot clear the panels.
    const actions = document.createElement("div");
    actions.className = "actions";
    actions.append(kill, cancel, restart);
    dialog.append(actions);

    let done = false;
    const finish = (text: string | null): void => {
      if (done) return;
      done = true;
      dialog.close();
      dialog.remove();
      resolve(text);
    };

    // Both actions take a moment: a stop waits for the daemon to clear every
    // lamp it lit. Disabling the row says so and stops a second press landing
    // on a half-finished one.
    const working = (text: string): void => {
      state.textContent = text;
      for (const b of [cancel, kill, restart]) b.disabled = true;
    };
    const failed = (e: unknown): void => {
      state.textContent = e instanceof Error ? e.message : String(e);
      state.classList.add("bad");
      for (const b of [cancel, kill, restart]) b.disabled = false;
    };

    void converterState().then(
      (s) => {
        state.textContent = s.running
          ? `Running${s.pid === null ? "" : `, process ${s.pid}`}.`
          : "Not running.";
        // Nothing to end, and nothing to start from a checkout with no daemon
        // built beside the editor.
        kill.disabled = !s.running;
        restart.disabled = !s.can_start;
        if (!s.can_start) {
          state.textContent += " No daemon is installed beside the editor, so there is none to start.";
        }
      },
      () => {
        state.textContent = "Could not tell whether it is running.";
      },
    );

    cancel.addEventListener("click", () => finish(null));
    restart.addEventListener("click", () => {
      working("Stopping and starting...");
      void converterRestart().then(finish, failed);
    });
    kill.addEventListener("click", () => {
      working("Ending it...");
      void converterKill().then(finish, failed);
    });

    dialog.addEventListener("close", () => finish(null));
    // A click on the backdrop lands on the dialog element itself, outside its
    // content box. Anywhere inside the box is a click on a child.
    dialog.addEventListener("click", (e) => {
      if (e.target !== dialog) return;
      const r = dialog.getBoundingClientRect();
      const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      if (!inside) finish(null);
    });

    document.body.append(dialog);
    dialog.showModal();
    cancel.focus();
  });
}
