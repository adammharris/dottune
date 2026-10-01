export const TICKS_PER_QUARTER = 480;

export interface NoteEvent {
  /** Start, in ticks. */
  start: number;
  /** Length, in ticks. */
  duration: number;
  voice: string;
  /** MIDI note number; C4 = 60. */
  midi: number;
  /** Index into `Song.slots` of the token that started this note. */
  slot: number;
}

export type SlotKind = "note" | "stack" | "chord" | "rest" | "hold";

/** One token as written, where it sits, and how it was placed. */
export interface SlotInfo {
  voice: string;
  /** Position in the source: block index, bar index, then indices into nested groups. */
  block: number;
  bar: number;
  path: number[];
  /** False for the rests that pad a short voice; they have no text to edit. */
  editable: boolean;
  text: string;
  /** Character offsets of `text` in the source; null for padding rests. */
  span: [number, number] | null;
  kind: SlotKind;
  start: number;
  end: number;
  midi: number[];
}

/** A song. `key`, `scale`, `octaves`, `tempo`, and `time` are the settings at the start. */
export interface Song {
  key: string;
  /** Semitones above the tonic for each scale degree. */
  scale: number[];
  /** Voices with an explicit `octave` directive. */
  octaves: Record<string, number>;
  tempo: number;
  time: [number, number];
  /** Total length in ticks, including trailing rests. */
  length: number;
  /** Voice names in order of first appearance. */
  voices: string[];
  events: NoteEvent[];
  /** Every token of every voice, grouped by voice in `voices` order, in time order. */
  slots: SlotInfo[];
  blocks: BlockInfo[];
  /** Key, tempo, and time changes between blocks, in time order (SPEC §2.1). */
  changes: Change[];
}

/** A block, where it plays, and the settings in force for it. */
export interface BlockInfo {
  start: number;
  end: number;
  /** First voice line, 1-based. */
  line: number;
  key: string;
  scale: number[];
  tempo: number;
  time: [number, number];
  /** Explicit `octave` settings in force. */
  octaves: Record<string, number>;
  /** Directives the change before this block set (empty for the first block, whose settings are the header's). */
  directives: string[];
}

export interface Change {
  tick: number;
  key?: string;
  tempo?: number;
  time?: [number, number];
}

export class TuneError extends Error {
  constructor(
    readonly line: number,
    message: string,
  ) {
    super(`line ${line}: ${message}`);
    this.name = "TuneError";
  }
}
