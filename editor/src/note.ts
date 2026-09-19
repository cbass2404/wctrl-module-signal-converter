// A line about why a row is the way it is.
//
// Both a lamp and a display field carry one, and in both it was write only:
// the shipped profiles explain why a lamp is deliberately left unassigned and
// which cells a CDU's lines land on, and none of it reached the window. The
// same question asked of two kinds of row, so one control rather than two.

/** Anything in a profile that carries a note. A binding and a readout both do. */
interface Annotated {
  note?: string;
}

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

/**
 * The note box for one row.
 *
 * `what` is the word for the row in the placeholder, "lamp" or "field".
 *
 * Closed to a button until there is something to read, because a device can
 * carry eighteen lamps and a textarea on each would push the ones with
 * conditions off the screen. Opening it writes nothing: an empty note is not
 * a note, and storing one would mark the profile unsaved for a stray click.
 */
export function noteEditor(row: Annotated, what: string, onChange: () => void): HTMLElement {
  const wrap = el("div", { class: "note-box" });
  let open = (row.note ?? "") !== "";

  const draw = (): void => {
    wrap.textContent = "";
    if (!open) {
      const add = el("button", { class: "add small", type: "button" }, "Add a note");
      add.addEventListener("click", () => {
        open = true;
        draw();
        wrap.querySelector("textarea")?.focus();
      });
      wrap.append(add);
      return;
    }

    const box = el("textarea", {
      class: "note",
      rows: "2",
      placeholder: `why this ${what} is set this way`,
    });
    box.value = row.note ?? "";
    box.addEventListener("input", () => {
      // Trimmed into the profile and never back into the box: rewriting what
      // is being typed would move the cursor out from under the user. Emptied
      // it leaves rather than stays as "", so a row nobody annotated reads the
      // same in the file as it always did. The box stays open, since an empty
      // one is mid-edit and closing it under the cursor would be worse.
      const text = box.value.trim();
      if (text === "") delete row.note;
      else row.note = text;
      onChange();
    });
    wrap.append(
      el("label", { class: "meta" }, "note"),
      box,
      el(
        "span",
        { class: "meta block" },
        "Nothing reads this but the next person to open the profile, which is " +
          "usually you. The shipped profiles use it for the reasoning a row " +
          "cannot show on its own: why a lamp is left unassigned, or which " +
          "cells a display's lines were chosen to land on.",
      ),
    );
  };

  draw();
  return wrap;
}
