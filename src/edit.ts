// Musical edits as token rewrites. Each operation returns the tokens to
// replace, by fig path; the caller applies them with fig's Editor.
//
// Notes are placed relative to the note before them (SPEC §4.2), so changing
// one note's pitch can move the next note's octave. `keepNextPitch` compares
// the song before and after an edit and rewrites the next sounding token's
// octave marks so that it keeps the pitch it had — which holds across key and
// octave changes too, since it compares pitches rather than predicting them.

import { CHORD, NOTE, NUMERALS, mod, nearest } from "./compile";
import type { SlotInfo, Song } from "./types";

export type Path = (string | number)[];

export interface TokenEdit {
  path: Path;
  text: string;
}

export const slotPath = (s: SlotInfo): Path => ["blocks", s.block, s.voice, s.bar, ...s.path];

const sounding = (s: SlotInfo) => s.kind === "note" || s.kind === "stack" || s.kind === "chord";
const marksText = (n: number) => (n > 0 ? "'".repeat(n) : ",".repeat(-n));

/** Octave marks that put `degree` at diatonic index `target`, given the reference. */
const marksFor = (ref: number | null, degree: number, target: number) => (target - nearest(ref, degree)) / 7;

const countMarks = (s: string) => [...s].reduce((n, c) => n + (c === "'" ? 1 : -1), 0);

/** The octave marks on a token's first note or chord root. */
function marksOf(text: string): number {
  const m = NOTE.exec(text.split("+")[0]!);
  if (m) return countMarks(m[3]!);
  return countMarks(CHORD.exec(text)![5]!);
}

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

const slotKey = (s: SlotInfo) => `${s.voice}|${s.block}|${s.bar}|${s.path.join(".")}`;

/**
 * After an edit to slot `index`, the octave-mark fix that keeps the next
 * sounding token in its voice at the pitch it had. `after` is the edited song.
 */
export function keepNextPitch(before: Song, index: number, after: Song): TokenEdit[] {
  const voice = before.slots[index]!.voice;
  for (let i = index + 1; i < before.slots.length && before.slots[i]!.voice === voice; i++) {
    const next = before.slots[i]!;
    if (!sounding(next)) continue;
    if (!next.editable) return [];
    const twin = after.slots.find((t) => slotKey(t) === slotKey(next));
    if (!twin?.midi.length) return [];
    const shift = next.midi[0]! - twin.midi[0]!;
    if (shift === 0 || shift % 12 !== 0) return [];
    return [{ path: slotPath(next), text: withMarks(next.text, marksOf(next.text) + shift / 12) }];
  }
  return [];
}

function rewrite(song: Song, index: number, text: string): TokenEdit[] {
  const s = song.slots[index]!;
  return text === s.text ? [] : [{ path: slotPath(s), text }];
}

/** Moves a note, stack, or chord by `steps` scale steps (7 = an octave). */
export function moveBy(song: Song, index: number, steps: number): TokenEdit[] {
  const s = song.slots[index]!;
  if (!sounding(s) || !s.editable) return [];
  const target = s.ref! + steps;
  const degree = mod(target, 7);
  const marks = marksFor(s.refBefore, degree, target);
  const octaveOnly = steps % 7 === 0;
  let text: string;

  if (s.kind === "note") {
    const acc = octaveOnly ? NOTE.exec(s.text)![1]! : "";
    text = acc + (degree + 1) + marksText(marks);
  } else if (s.kind === "chord") {
    const seventh = CHORD.exec(s.text)![4] !== undefined;
    const scale = song.blocks[s.block]?.scale ?? song.scale;
    text = octaveOnly ? withMarks(s.text, marks) : diatonicChord(scale, degree, seventh) + marksText(marks);
  } else {
    // A stack keeps its shape: every member moves by the same number of steps.
    const members = s.text.split("+").map((m) => NOTE.exec(m)!);
    text = members
      .map(([, acc, deg, mk], i) => {
        const d = mod(Number(deg) - 1 + steps, 7) + 1;
        return (octaveOnly ? acc : "") + d + (i === 0 ? marksText(marks) : mk);
      })
      .join("+");
  }
  return rewrite(song, index, text);
}

/** Replaces the token with a plain scale degree (0-based), placed by the nearest-note rule. */
export function setDegree(song: Song, index: number, degree: number): TokenEdit[] {
  const s = song.slots[index]!;
  if (!s.editable) return [];
  return rewrite(song, index, String(degree + 1));
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
  const second = sounding(s) ? withMarks(s.text, 0) : s.text;
  return [{ path: slotPath(s), text: `[${s.text} ${second}]` }];
}

/** Toggles a sharp or flat on a single note. */
export function toggleAccidental(song: Song, index: number, acc: "#" | "b"): TokenEdit[] {
  const s = song.slots[index]!;
  const m = s.kind === "note" && s.editable ? NOTE.exec(s.text) : null;
  if (!m) return [];
  return [{ path: slotPath(s), text: (m[1] === acc ? "" : acc) + m[2] + m[3] }];
}
