// The structure bar: bars, blocks, and voices, for wherever the selected note
// is. Each button (and its key on the roll) runs one of src/structure's edits
// and then selects what the edit made or moved.

import {
  addVoice,
  blockBars,
  deleteBar,
  deleteBlock,
  deleteVoice,
  duplicateBar,
  duplicateBlock,
  insertBar,
  insertBlock,
  joinBlocks,
  moveBar,
  moveBlock,
  moveVoice,
  renameVoice,
  splitBlock,
} from "../src/structure";
import type { Song } from "../src/types";
import { $, type App } from "./app";

export interface StructureBar {
  /** Shows where the bar's edits apply and which of them can. */
  sync(): void;
  /** Runs the edit a key on the roll stands for; true if `e` was one. */
  key(e: KeyboardEvent): boolean;
}

/** Where the selection is: its voice, block, and bar. */
interface Here {
  voice: string;
  block: number;
  bar: number;
  path: number[];
}

/** One edit: what it does, and where to select afterwards (null: nothing). */
interface Action {
  run(source: string, song: Song, here: Here): string;
  then?(here: Here, song: Song): Partial<Here> | null;
  /** Whether it can run here. */
  can?(here: Here, song: Song): boolean;
  /** Whether it needs no selection, only a tune. */
  free?: boolean;
}

const nBars = (song: Song, b: number) => blockBars(song.blocks[b]!);

const ACTIONS: Record<string, Action> = {
  "bar-before": { run: (s, g, h) => insertBar(s, g, h.block, h.bar), then: () => ({ path: [0] }) },
  "bar-after": { run: (s, g, h) => insertBar(s, g, h.block, h.bar + 1), then: (h) => ({ bar: h.bar + 1, path: [0] }) },
  "bar-dup": { run: (s, g, h) => duplicateBar(s, g, h.block, h.bar), then: (h) => ({ bar: h.bar + 1 }) },
  "bar-left": { run: (s, g, h) => moveBar(s, g, h.block, h.bar, -1), then: (h) => ({ bar: h.bar - 1 }), can: (h) => h.bar > 0 },
  "bar-right": {
    run: (s, g, h) => moveBar(s, g, h.block, h.bar, 1),
    then: (h) => ({ bar: h.bar + 1 }),
    can: (h, g) => h.bar < nBars(g, h.block) - 1,
  },
  "bar-del": {
    run: (s, g, h) => deleteBar(s, g, h.block, h.bar),
    then: (h, g) => (nBars(g, h.block) > 1 ? { bar: Math.min(h.bar, nBars(g, h.block) - 2), path: [0] } : null),
  },
  "block-new": {
    run: (s, g, h) => insertBlock(s, g, h.block),
    then: (h, g) => ({ block: g.blocks.length ? h.block + 1 : 0, bar: 0, path: [0] }),
    free: true,
  },
  "block-dup": { run: (s, g, h) => duplicateBlock(s, g, h.block), then: (h) => ({ block: h.block + 1 }) },
  "block-up": { run: (s, g, h) => moveBlock(s, g, h.block, -1), then: (h) => ({ block: h.block - 1 }), can: (h) => h.block > 0 },
  "block-down": {
    run: (s, g, h) => moveBlock(s, g, h.block, 1),
    then: (h) => ({ block: h.block + 1 }),
    can: (h, g) => h.block < g.blocks.length - 1,
  },
  "block-split": { run: (s, g, h) => splitBlock(s, g, h.block, h.bar), then: (h) => ({ block: h.block + 1, bar: 0 }), can: (h) => h.bar > 0 },
  "block-join": { run: (s, g, h) => joinBlocks(s, g, h.block), can: (h, g) => h.block < g.blocks.length - 1 },
  "block-del": { run: (s, g, h) => deleteBlock(s, g, h.block), then: () => null },
  "voice-up": { run: (s, g, h) => moveVoice(s, g, h.voice, -1), can: (h, g) => g.voices.indexOf(h.voice) > 0 },
  "voice-down": { run: (s, g, h) => moveVoice(s, g, h.voice, 1), can: (h, g) => g.voices.indexOf(h.voice) < g.voices.length - 1 },
  "voice-del": { run: (s, g, h) => deleteVoice(s, g, h.voice, h.block), then: () => null },
};

/** Keys on the roll, with ⌥: `code` (and ⇧) → action. */
const KEYS: [code: string, shift: boolean, action: string][] = [
  ["KeyB", false, "bar-after"],
  ["KeyB", true, "bar-before"],
  ["KeyD", false, "bar-dup"],
  ["ArrowLeft", false, "bar-left"],
  ["ArrowRight", false, "bar-right"],
  ["Backspace", false, "bar-del"],
  ["KeyN", false, "block-new"],
  ["KeyD", true, "block-dup"],
  ["ArrowUp", false, "block-up"],
  ["ArrowDown", false, "block-down"],
  ["KeyX", false, "block-split"],
  ["KeyJ", false, "block-join"],
  ["Backspace", true, "block-del"],
  ["ArrowUp", true, "voice-up"],
  ["ArrowDown", true, "voice-down"],
];

export function setupStructureBar(app: App): StructureBar {
  const bar = $<HTMLElement>("structure");
  const buttons = [...bar.querySelectorAll<HTMLButtonElement>("button[data-act]")];
  const barLabel = $<HTMLElement>("bar-label");
  const blockLabel = $<HTMLElement>("block-label");
  const voiceLabel = $<HTMLElement>("voice-label");

  function here(): Here | null {
    const { song, selected } = app;
    if (!song || selected === null || app.compiled !== app.source.value) return null;
    const s = song.slots[selected]!;
    return { voice: s.voice, block: s.block, bar: s.bar, path: s.path };
  }

  /** Where the free actions apply without a selection: the last block. */
  function anywhere(song: Song): Here {
    const block = Math.max(0, song.blocks.length - 1);
    return { voice: song.voices[0] ?? "RH", block, bar: 0, path: [0] };
  }

  /** Selects the slot at `h`, or the first in its bar. */
  function selectAt(h: Here): void {
    const song = app.song;
    if (!song) return;
    const same = (s: Song["slots"][number]) => s.voice === h.voice && s.block === h.block && s.bar === h.bar;
    let i = song.slots.findIndex((s) => same(s) && s.editable && s.path.join(".") === h.path.join("."));
    if (i === -1) i = song.slots.findIndex((s) => same(s) && s.editable);
    if (i === -1) i = song.slots.findIndex((s) => same(s));
    app.select(i === -1 ? null : i);
  }

  function run(name: string): void {
    const action = ACTIONS[name]!;
    const song = app.song;
    if (!song || app.compiled !== app.source.value) return;
    const h = here() ?? (action.free ? anywhere(song) : null);
    if (!h || (action.can && !action.can(h, song))) return;
    edit((s, g) => action.run(s, g, h), action.then ? action.then(h, song) : {}, h);
  }

  /** Runs an edit and selects `then` (relative to `h`) in the result. */
  function edit(op: (source: string, song: Song) => string, then: Partial<Here> | null, h: Here | null): void {
    const song = app.song!;
    if (!app.tryEdit(() => op(app.source.value, song))) return;
    if (then === null || !h) app.select(null);
    else selectAt({ ...h, ...then });
  }

  // The roll keeps the keyboard, so arrows and edit keys carry on after a click here.
  for (const b of buttons)
    b.addEventListener("click", () => {
      run(b.dataset.act!);
      app.canvas.focus();
    });

  $<HTMLButtonElement>("voice-add").addEventListener("click", () => {
    const song = app.song;
    if (!song) return;
    const h = here() ?? anywhere(song);
    const name = prompt(`Name of the new voice in block ${h.block + 1}:`, nextVoiceName(song))?.trim();
    if (name) edit((s, g) => addVoice(s, g, h.block, name), { voice: name, bar: 0, path: [0] }, h);
    app.canvas.focus();
  });

  $<HTMLButtonElement>("voice-rename").addEventListener("click", () => {
    const h = here();
    if (!h) return;
    const name = prompt(`Rename ${h.voice} everywhere to:`, h.voice)?.trim();
    if (name && name !== h.voice) edit((s, g) => renameVoice(s, g, h.voice, name), { voice: name }, h);
    app.canvas.focus();
  });

  function sync(): void {
    const song = app.song;
    const h = here();
    barLabel.textContent = h ? `Bar ${h.bar + 1}` : "Bar";
    blockLabel.textContent = h && song && song.blocks.length > 1 ? `Block ${h.block + 1}` : "Block";
    voiceLabel.textContent = h ? h.voice : "Voice";
    for (const b of buttons) {
      const action = ACTIONS[b.dataset.act!]!;
      const at = h ?? (song && action.free ? anywhere(song) : null);
      b.disabled = !song || !at || !app.figReady || (action.can !== undefined && !action.can(at, song));
    }
    $<HTMLButtonElement>("voice-add").disabled = !song || !app.figReady;
    $<HTMLButtonElement>("voice-rename").disabled = !h || !app.figReady;
  }

  return {
    sync,
    key(e) {
      if (!e.altKey || e.metaKey || e.ctrlKey) return false;
      const hit = KEYS.find(([code, shift]) => code === e.code && shift === e.shiftKey);
      if (!hit) return false;
      run(hit[2]);
      return true;
    },
  };
}

/** A name for a new voice: LH or RH if free, else the first of V2, V3… that is. */
function nextVoiceName(song: Song): string {
  for (const v of ["RH", "LH"]) if (!song.voices.includes(v)) return v;
  for (let i = 2; ; i++) if (!song.voices.includes(`V${i}`)) return `V${i}`;
}
