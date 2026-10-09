// Musical edits as token rewrites. Each operation returns the tokens to
// replace, by fig path; the caller applies them with fig's Editor.
//
// A token's pitch depends only on itself, the key, and its voice's octave
// (SPEC §4.2), so an edit rewrites one token and never moves another.

import { CHORD, NOTE, NUMERALS, mod } from "./compile";
import type { SlotInfo, Song } from "./types";

export type Path = (string | number)[];

export interface TokenEdit {
  path: Path;
  text: string;
}

export const slotPath = (s: SlotInfo): Path => ["blocks", s.block, s.voice, s.bar, ...s.path];

/** Whether a slot starts a note, stack, or chord. */
export const sounding = (s: SlotInfo) => s.kind === "note" || s.kind === "stack" || s.kind === "chord";

/** The note, stack, or chord a slot sounds as part of: itself, or the one its holds continue. */
export function onsetOf(song: Song, index: number): number | null {
  const voice = song.slots[index]!.voice;
  for (let i = index; i >= 0 && song.slots[i]!.voice === voice; i--) {
    const s = song.slots[i]!;
    if (sounding(s)) return i;
    if (s.kind !== "hold") return null;
  }
  return null;
}

/** The slot sounding from `start` in `voice`, or -1: for finding a note after an edit that may have rewritten its path. */
export const onsetAt = (song: Song, voice: string, start: number) =>
  song.slots.findIndex((s) => s.voice === voice && Math.abs(s.start - start) <= 1 && sounding(s));
const marksText = (n: number) => (n > 0 ? "'".repeat(n) : ",".repeat(-n));

const countMarks = (s: string) => [...s].reduce((n, c) => n + (c === "'" ? 1 : -1), 0);

/** The diatonic index (0 = the tonic of the voice's octave) of a note, or of a chord's root. */
function indexOf(text: string): number {
  const m = NOTE.exec(text);
  if (m) return Number(m[2]) - 1 + 7 * countMarks(m[3]!);
  const c = CHORD.exec(text)!;
  return NUMERALS.indexOf(c[2]!.toLowerCase()) + 7 * countMarks(c[5]!);
}

/** A degree (1–7) with the marks that put it at diatonic index `idx`. */
const degreeAt = (idx: number) => mod(idx, 7) + 1 + marksText(Math.floor(idx / 7));

/** The token with the octave marks on its first note or chord root replaced. */
function withMarks(text: string, marks: number): string {
  let m = NOTE.exec(text);
  if (m) return m[1]! + m[2]! + marksText(marks);
  m = CHORD.exec(text);
  if (m) {
    const [, acc, numeral, quality, seventh, , bassAcc, bassDeg] = m;
    return acc! + numeral! + quality! + (seventh ?? "") + marksText(marks) + (bassDeg ? `/${bassAcc}${bassDeg}` : "");
  }
  const [first, ...rest] = text.split("+");
  return [withMarks(first!, marks), ...rest].join("+");
}

/** The diatonic triad (or seventh chord) on `degree` in this scale, as a numeral. */
function diatonicChord(scale: number[], degree: number, seventh: boolean): string {
  const above = (k: number) => mod(scale[mod(degree + k, 7)]! - scale[degree]!, 12);
  let numeral = NUMERALS[degree]!;
  let quality = "";
  if (above(2) === 4) {
    numeral = numeral.toUpperCase();
    if (above(4) === 8) quality = "+";
  } else if (above(4) === 6) {
    quality = "o";
  }
  if (!seventh) return numeral + quality;
  if (quality === "o") return numeral + (above(6) === 10 ? "h7" : "o7");
  if (quality === "+") return numeral + "+7";
  return numeral + (above(6) === 11 ? "maj7" : "7");
}

function rewrite(song: Song, index: number, text: string): TokenEdit[] {
  const s = song.slots[index]!;
  return text === s.text ? [] : [{ path: slotPath(s), text }];
}

/** Moves a note, stack, or chord by `steps` scale steps (7 = an octave). */
export function moveBy(song: Song, index: number, steps: number): TokenEdit[] {
  const s = song.slots[index]!;
  if (!sounding(s) || !s.editable) return [];
  const octaveOnly = steps % 7 === 0;
  let text: string;

  if (s.kind === "note") {
    const acc = octaveOnly ? NOTE.exec(s.text)![1]! : "";
    text = acc + degreeAt(indexOf(s.text) + steps);
  } else if (s.kind === "chord") {
    const target = indexOf(s.text) + steps;
    const seventh = CHORD.exec(s.text)![4] !== undefined;
    const scale = song.blocks[s.block]?.scale ?? song.scale;
    text = octaveOnly
      ? withMarks(s.text, Math.floor(target / 7))
      : diatonicChord(scale, mod(target, 7), seventh) + marksText(Math.floor(target / 7));
  } else {
    // A stack keeps its shape: every member moves by the same number of steps.
    text = s.text
      .split("+")
      .map((member) => (octaveOnly ? NOTE.exec(member)![1]! : "") + degreeAt(indexOf(member) + steps))
      .join("+");
  }
  return rewrite(song, index, text);
}

/** Replaces the token with a plain scale degree (0-based), keeping a note or chord's octave marks. */
export function setDegree(song: Song, index: number, degree: number): TokenEdit[] {
  const s = song.slots[index]!;
  if (!s.editable) return [];
  const octave = sounding(s) ? Math.floor(indexOf(s.text.split("+")[0]!) / 7) : 0;
  return rewrite(song, index, degreeAt(degree + 7 * octave));
}

export function toRest(song: Song, index: number): TokenEdit[] {
  const s = song.slots[index]!;
  return s.editable ? rewrite(song, index, ".") : [];
}

export function toHold(song: Song, index: number): TokenEdit[] {
  const s = song.slots[index]!;
  return s.editable ? rewrite(song, index, "-") : [];
}

/** Splits a slot into two equal halves, both sounding what it sounded. */
export function split(song: Song, index: number): TokenEdit[] {
  const s = song.slots[index]!;
  if (!s.editable) return [];
  return [{ path: slotPath(s), text: `[${s.text} ${s.text}]` }];
}

/** Toggles a sharp or flat on a single note. */
export function toggleAccidental(song: Song, index: number, acc: "#" | "b"): TokenEdit[] {
  const s = song.slots[index]!;
  const m = s.kind === "note" && s.editable ? NOTE.exec(s.text) : null;
  if (!m) return [];
  return [{ path: slotPath(s), text: (m[1] === acc ? "" : acc) + m[2] + m[3] }];
}

// ── resizing ────────────────────────────────────────────────────────────────
//
// A note lasts its own slot plus the holds after it, so its length is changed
// by rewriting slots, never by moving barlines: the slots it grows over become
// `-`, the holds it gives up become `.`, and where the new end falls inside a
// slot, that slot is split. Groups the edit touches are then written in their
// coarsest form, so `[- -]` reads `-` and `[1 - . .]` reads `[1 .]`.

/** Where a resized note ends: `k`/`n` of the way through slot `slot` (0 < n, 0 ≤ k ≤ n). */
export interface Cut {
  slot: number;
  k: number;
  n: number;
}

/** A bar as written: a token, or a group of nodes. */
type Node = string | Node[];

const serialize = (n: Node): string => (typeof n === "string" ? n : `[${n.map(serialize).join(" ")}]`);

/** The index of the last hold continuing the note at `index` (itself if none). */
function holdEnd(song: Song, index: number): number {
  const voice = song.slots[index]!.voice;
  let e = index;
  for (let t = song.slots[e + 1]; t && t.voice === voice && t.kind === "hold"; t = song.slots[++e + 1]);
  return e;
}

/** Whether slot `m` is an editable slot in `voice`. */
const editableIn = (song: Song, voice: string, m: number) => song.slots[m]?.voice === voice && song.slots[m]!.editable;

/** The cut nearest `tick` for the note at `index`: one of `parts(slot)` equal points through the slot under `tick`. */
export function snapCut(song: Song, index: number, tick: number, parts: (s: SlotInfo) => number): Cut | null {
  const s = song.slots[index]!;
  if (!sounding(s) || !s.editable) return null;
  let m = index;
  while (song.slots[m]!.end <= tick && editableIn(song, s.voice, m + 1)) m++;
  const slot = song.slots[m]!;
  const n = Math.max(1, Math.round(parts(slot)));
  const k = Math.min(n, Math.max(m === index ? 1 : 0, Math.round(((tick - slot.start) / (slot.end - slot.start)) * n)));
  return { slot: m, k, n };
}

/** Where the note at `index` ends now, as a cut. */
export function currentCut(song: Song, index: number): Cut {
  return { slot: holdEnd(song, index), k: 1, n: 1 };
}

/** Grows the note at `index` over the next slot, or shrinks it by its last hold (by half if it has none). */
export function lengthen(song: Song, index: number, dir: 1 | -1): TokenEdit[] {
  const s = song.slots[index]!;
  const e = holdEnd(song, index);
  if (dir === 1) return editableIn(song, s.voice, e + 1) ? resize(song, index, { slot: e + 1, k: 1, n: 1 }) : [];
  return resize(song, index, e > index ? { slot: e, k: 0, n: 1 } : { slot: index, k: 1, n: 2 });
}

/**
 * Makes the note at `index` end at `cut`. Growing over another note cuts it
 * short: what is left of it, if anything, is struck again where this one ends.
 */
export function resize(song: Song, index: number, cut: Cut): TokenEdit[] {
  const s = song.slots[index]!;
  const { slot: j, k, n } = cut;
  if (!sounding(s) || !s.editable || j < index || (j === index && k === 0)) return [];
  for (let m = index; m <= j; m++) if (!editableIn(song, s.voice, m)) return [];

  const e = holdEnd(song, index);
  const rep = new Map<number, Node>();
  /** The token sounding where the note now ends: the last one it grew over. */
  let onset: string | null = null;
  const fill = (text: string, count: number) => Array<string>(count).fill(text);

  for (let m = index; m <= Math.max(e, j) + 1 && editableIn(song, s.voice, m); m++) {
    const t = song.slots[m]!;
    const held = m <= e;
    if (m < j || (m === j && k === n)) {
      if (!held) {
        if (t.kind !== "hold") onset = t.text;
        rep.set(m, "-");
      }
      continue;
    }
    const struck = t.kind === "hold" ? onset : t.text;
    const rest = (count: number): string[] => (held ? fill(".", count) : [struck ?? "-", ...fill("-", count - 1)]);
    if (m > j || k === 0) {
      if (held) rep.set(m, ".");
      else {
        if (t.kind === "hold" && onset !== null) rep.set(m, onset);
        break;
      }
      continue;
    }
    const head = m === index ? [t.text, ...fill("-", k - 1)] : fill("-", k);
    rep.set(m, [...head, ...rest(n - k)]);
    if (!held) break;
  }
  return barEdits(song, rep);
}

/** The tick a cut falls at. */
export const cutTick = (song: Song, cut: Cut): number => {
  const s = song.slots[cut.slot]!;
  return s.start + ((s.end - s.start) * cut.k) / cut.n;
};

/**
 * The cut nearest `tick` for a new start of the note at `index`: anywhere from
 * the voice's first editable slot before it to just short of where it ends.
 */
export function snapStart(song: Song, index: number, tick: number, parts: (s: SlotInfo) => number): Cut | null {
  const s = song.slots[index]!;
  if (!sounding(s) || !s.editable) return null;
  const e = holdEnd(song, index);
  let m = index;
  while (song.slots[m]!.start > tick && editableIn(song, s.voice, m - 1)) m--;
  while (song.slots[m]!.end <= tick && m < e) m++;
  const slot = song.slots[m]!;
  const n = Math.max(1, Math.round(parts(slot)));
  const k = Math.min(m === e ? n - 1 : n, Math.max(0, Math.round(((tick - slot.start) / (slot.end - slot.start)) * n)));
  return { slot: m, k, n };
}

/**
 * Makes the note at `index` start at `cut`, keeping where it ends. Starting
 * earlier cuts short whatever sounds before; starting later leaves a rest.
 */
export function restart(song: Song, index: number, cut: Cut): TokenEdit[] {
  const s = song.slots[index]!;
  let { slot: j, k, n } = cut;
  if (k === n) [j, k, n] = [j + 1, 0, 1];
  const e = holdEnd(song, index);
  if (!sounding(s) || !s.editable || j > e || (j === index && k === 0)) return [];
  for (let m = Math.min(j, index); m <= Math.max(j, index); m++) if (!editableIn(song, s.voice, m)) return [];

  const fill = (text: string, count: number) => Array<string>(count).fill(text);
  const rep = new Map<number, Node>();
  const tail = [s.text, ...fill("-", n - k - 1)];
  if (j < index) {
    for (let m = j + 1; m <= index; m++) rep.set(m, "-");
    const t = song.slots[j]!;
    rep.set(j, k === 0 ? s.text : [t.text, ...fill("-", k - 1), ...tail]);
  } else {
    for (let m = index; m < j; m++) rep.set(m, ".");
    rep.set(j, k === 0 ? s.text : [...fill(".", k), ...tail]);
  }
  return barEdits(song, rep);
}

/** Token edits for slot replacements, with each group they touch reduced. */
function barEdits(song: Song, rep: Map<number, Node>): TokenEdit[] {
  const bars = new Map<string, number[]>();
  for (const m of rep.keys()) {
    const t = song.slots[m]!;
    const key = `${t.voice}|${t.block}|${t.bar}`;
    if (!bars.has(key)) bars.set(key, []);
  }
  song.slots.forEach((t, m) => bars.get(`${t.voice}|${t.block}|${t.bar}`)?.push(m));

  const edits: TokenEdit[] = [];
  for (const members of bars.values()) {
    const first = song.slots[members[0]!]!;
    const old: Node[] = [];
    const next: Node[] = [];
    const touched = new Set<string>();
    for (const m of members) {
      const t = song.slots[m]!;
      place(old, t.path, t.text);
      place(next, t.path, rep.get(m) ?? t.text);
      if (rep.has(m)) for (let d = 1; d <= t.path.length; d++) touched.add(t.path.slice(0, d).join("."));
    }
    const reduced = next.map((child, i) => reduceTouched(child, [i], touched));
    diff(old, reduced, ["blocks", first.block, first.voice, first.bar], edits);
  }
  return edits;
}

function place(root: Node[], path: number[], value: Node): void {
  let at = root;
  for (const i of path.slice(0, -1)) at = (at[i] ??= []) as Node[];
  at[path.at(-1)!] = value;
}

/** Reduces, bottom up, every group on the way to a replaced slot. */
function reduceTouched(node: Node, path: number[], touched: Set<string>): Node {
  if (!touched.has(path.join("."))) return node;
  if (typeof node === "string") return node;
  return reduce(node.map((child, i) => reduceTouched(child, [...path, i], touched)));
}

/**
 * A group in its coarsest equivalent form: split into as few equal runs as
 * possible where each run is one token held (`1 -` → `1`) or a rest (`. .` → `.`).
 */
function reduce(group: Node[]): Node {
  if (!group.every((c) => typeof c === "string")) return group;
  const tokens = group as string[];
  const n = tokens.length;
  for (let m = 1; m < n; m++) {
    if (n % m !== 0) continue;
    const r = n / m;
    const runs: string[] = [];
    for (let i = 0; i < n; i += r) {
      const run = tokens.slice(i, i + r);
      const tail = run.slice(1);
      if (tail.every((t) => t === "-")) runs.push(run[0]!);
      else if (run[0] === "." && tail.every((t) => t === "." || t === "-")) runs.push(".");
      else break;
    }
    if (runs.length === m) return m === 1 ? runs[0]! : runs;
  }
  return group;
}

/** Edits turning `old` into `next`, each at the outermost node whose shape changed. */
function diff(old: Node, next: Node, path: Path, out: TokenEdit[]): void {
  if (typeof old !== "string" && typeof next !== "string" && old.length === next.length) {
    old.forEach((child, i) => diff(child, next[i]!, [...path, i], out));
  } else if (serialize(old) !== serialize(next)) {
    out.push({ path, text: serialize(next) });
  }
}
