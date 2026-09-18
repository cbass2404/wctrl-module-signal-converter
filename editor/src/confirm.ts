// Asking before something that cannot be undone, inside the window.
//
// `window.confirm` showed nothing in this webview and answered yes, so Reset
// replaced a profile unasked. The dialog plugin's native box did ask, but it
// opened outside the window in the OS style. A <dialog> stays over the page it
// is about and looks like the rest of the editor.

/**
 * Ask a yes-or-no question about a destructive action.
 *
 * `message` may hold paragraphs separated by a blank line. `ok` names the
 * action on its button ("Reset", "Delete"), because a button reading OK makes
 * the user go back and reread the question. Cancel takes the focus, so Enter
 * on a dialog opened by a misclick does nothing harmful. Escape and a click
 * outside the box both cancel.
 */
export function confirmAction(message: string, ok: string): Promise<boolean> {
  return new Promise((resolve) => {
    // Laid out like the editor's other dialogs: the question as the heading,
    // what it means under it, and the buttons at the bottom right.
    const dialog = document.createElement("dialog");
    dialog.className = "picker confirm";

    const [question = "", ...detail] = message.split("\n\n");
    const h2 = document.createElement("h2");
    h2.textContent = question;
    dialog.append(h2);
    for (const para of detail) {
      const p = document.createElement("p");
      p.className = "meta";
      p.textContent = para;
      dialog.append(p);
    }

    const cancel = document.createElement("button");
    cancel.textContent = "Cancel";
    const go = document.createElement("button");
    go.className = "danger";
    go.textContent = ok;
    const actions = document.createElement("div");
    actions.className = "actions";
    actions.append(cancel, go);
    dialog.append(actions);

    let answered = false;
    const answer = (yes: boolean): void => {
      if (answered) return;
      answered = true;
      dialog.close();
      dialog.remove();
      resolve(yes);
    };
    cancel.addEventListener("click", () => answer(false));
    go.addEventListener("click", () => answer(true));
    // Escape fires `cancel` and then closes; `close` covers any other way out.
    dialog.addEventListener("close", () => answer(false));
    // A click on the backdrop lands on the dialog element itself, outside its
    // content box. Anywhere inside the box is a click on a child.
    dialog.addEventListener("click", (e) => {
      if (e.target !== dialog) return;
      const r = dialog.getBoundingClientRect();
      const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      if (!inside) answer(false);
    });

    document.body.append(dialog);
    dialog.showModal();
    cancel.focus();
  });
}
