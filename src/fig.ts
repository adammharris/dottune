// Edits through fig: the tune language registered with @diaryx/fig's Editor,
// so every change is a minimal splice that fig reparses (and rolls back if the
// result does not parse).

import { Editor, init, registerLanguage, type Format } from "@diaryx/fig";
import { tune } from "../tools/fig-tune.mjs";
import type { TokenEdit } from "./edit";
import type { Song } from "./types";

let format: Format | null = null;

export async function loadFig(): Promise<void> {
  if (format !== null) return;
  await init();
  format = registerLanguage(tune);
}

/** Runs `fn` against an Editor over `source`; returns the edited source. Throws if any edit is refused. */
export function applyEdits(source: string, fn: (ed: Editor) => void): string {
  if (format === null) throw new Error("fig is not loaded");
  const ed = Editor.open(source, format);
  try {
    fn(ed);
    return ed.source();
  } finally {
    ed.dispose();
  }
}

export function applyTokenEdits(source: string, edits: TokenEdit[]): string {
  return applyEdits(source, (ed) => {
    for (const e of edits) ed.replaceValueRaw(e.path, e.text);
  });
}

/** Applies a musical edit to one slot. */
export function applyMusicalEdit(
  source: string,
  song: Song,
  index: number,
  op: (song: Song, index: number) => TokenEdit[],
): string {
  const edits = op(song, index);
  return edits.length === 0 ? source : applyTokenEdits(source, edits);
}
