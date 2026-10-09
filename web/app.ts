// What the parts of the page share: the elements everyone touches, the song
// as last compiled, the selection, and the few actions that change them. Each
// part gets this one object, so none needs another's internals.

import type { SlotInfo, Song, TuneError } from "../src/types";
import type { History } from "./history";
import type { Player } from "./player";
import type { PianoRoll } from "./roll";

export interface App {
  readonly source: HTMLTextAreaElement;
  readonly canvas: HTMLCanvasElement;
  readonly roll: PianoRoll;
  readonly player: Player;
  readonly history: History;

  /** Last song that compiled; kept playing while the source has errors. */
  song: Song | null;
  /** The text `song` was compiled from; slot spans are only valid while it matches the source. */
  compiled: string;
  /** Every error in the source as of the last compile; empty when it compiled. */
  errors: TuneError[];
  /** The text `errors` were found in; their spans are only valid while it matches the source. */
  errorSource: string;
  figReady: boolean;
  /** The selected slot, as an index into `song.slots`. */
  selected: number | null;

  /** Compiles the source and shows the result everywhere. */
  recompile(): void;
  select(index: number | null): void;
  /** Selects the slot written at `key` in the current song, if there still is one. */
  selectKey(key: SlotKey): void;
  /** Runs an edit and records it for undo; a refusal (fig rolled it back) is reported, not thrown. */
  tryEdit(fn: () => string): boolean;
  setStatus(text: string, kind?: "html" | "error" | "text"): void;
  /** Puts the selection, or the song's summary, back in the status bar. */
  describeSelection(): void;
}

/** Identifies a slot by where it is written, so selection survives a recompile. */
export type SlotKey = string;
export const keyOf = (s: Pick<SlotInfo, "voice" | "block" | "bar" | "path">): SlotKey => `${s.voice}|${s.block}|${s.bar}|${s.path.join(".")}`;

export const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export const escapeHtml = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
