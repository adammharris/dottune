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

const sounding = (s: SlotInfo) => s.kind === "note" || s.kind === "stack" || s.kind === "chord";
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
