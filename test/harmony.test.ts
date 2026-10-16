import { beforeAll, expect, test } from "bun:test";
import { compile, formatEvents } from "../src/compile";
import {
  addThird,
  cycleChord,
  cycleInversion,
  cycleQuality,
  deleteSlot,
  divideBar,
  insertSlot,
  joinGroup,
  setPitch,
  split,
  toggleAccidental,
  type TokenEdit,
} from "../src/edit";
import { applyMusicalEdit, loadFig } from "../src/fig";
import type { Song } from "../src/types";

beforeAll(loadFig);

/** Applies an operation to the `nth` slot of `voice`, through fig. */
function run(source: string, voice: string, nth: number, op: (song: Song, index: number) => TokenEdit[]): string {
  const song = compile(source);
  const index = song.slots.map((s, i) => [s, i] as const).filter(([s]) => s.voice === voice)[nth]![1];
  return applyMusicalEdit(source, song, index, op);
}

/** Steps an operation `times` times on the same slot. */
function steps(source: string, voice: string, nth: number, op: (song: Song, index: number) => TokenEdit[], times: number): string[] {
  const out: string[] = [];
  for (let i = 0; i < times; i++) out.push((source = run(source, voice, nth, op)));
  return out.map((s) => s.trim());
}

// ── harmony ─────────────────────────────────────────────────────────────────

test("c turns a note into its triad, then its seventh chord, then back", () => {
  expect(steps("LH: 5, |\n", "LH", 0, cycleChord, 3)).toEqual(["LH: V, |", "LH: V7, |", "LH: 5, |"]);
  expect(steps("LH: 7 |\n", "LH", 0, cycleChord, 2)).toEqual(["LH: viio |", "LH: viih7 |"]);
  expect(steps("LH: 4 |\n", "LH", 0, cycleChord, 2)).toEqual(["LH: IV |", "LH: IVmaj7 |"]);
  // In minor, the diatonic qualities follow the scale.
  expect(run("key A minor\nLH: 1 |\n", "LH", 0, cycleChord)).toBe("key A minor\nLH: i |\n");
  // A stack becomes the triad on its first member.
  expect(run("RH: 2+4+6 |\n", "RH", 0, cycleChord)).toBe("RH: ii |\n");
});

test("q steps a chord through the qualities", () => {
  expect(steps("LH: I |\n", "LH", 0, cycleQuality, 4)).toEqual(["LH: i |", "LH: io |", "LH: I+ |", "LH: I |"]);
  expect(steps("LH: V7, |\n", "LH", 0, cycleQuality, 5)).toEqual(["LH: Vmaj7, |", "LH: v7, |", "LH: vh7, |", "LH: vo7, |", "LH: V7, |"]);
  const pitches = formatEvents(compile("LH: io |\n").events).split("\n").map((l) => l.split(" ").at(-1));
  expect(pitches).toEqual(["C3", "D#3", "F#3"]);
});

test("v steps a chord through its inversions", () => {
  expect(steps("LH: I |\n", "LH", 0, cycleInversion, 3)).toEqual(["LH: I/3 |", "LH: I/5 |", "LH: I |"]);
  expect(steps("LH: V7 |\n", "LH", 0, cycleInversion, 4)).toEqual(["LH: V7/7 |", "LH: V7/2 |", "LH: V7/4 |", "LH: V7 |"]);
  // A chord borrowed from outside the scale spells its bass to match.
  expect(run("LH: II |\n", "LH", 0, cycleInversion)).toBe("LH: II/#4 |\n");
  expect(run("key A minor\nLH: V |\n", "LH", 0, cycleInversion)).toBe("key A minor\nLH: V/#7 |\n");
});

test("+ stacks a third on top", () => {
  expect(steps("RH: 1 |\n", "RH", 0, addThird, 3)).toEqual(["RH: 1+3 |", "RH: 1+3+5 |", "RH: 1+3+5+7 |"]);
  expect(run("RH: 6+1' |\n", "RH", 0, addThird)).toBe("RH: 6+1'+3' |\n");
  expect(run("LH: I |\n", "LH", 0, addThird)).toBe("LH: I |\n");
});

test("accidentals toggle on a chord's root", () => {
  expect(run("LH: VII |\n", "LH", 0, (s, i) => toggleAccidental(s, i, "b"))).toBe("LH: bVII |\n");
  expect(run("LH: bVII |\n", "LH", 0, (s, i) => toggleAccidental(s, i, "b"))).toBe("LH: VII |\n");
});

test("a pitch is written as the degree that sounds it", () => {
  expect(run("RH: . |\n", "RH", 0, (s, i) => setPitch(s, i, 67))).toBe("RH: 5 |\n");
  expect(run("RH: 1 |\n", "RH", 0, (s, i) => setPitch(s, i, 61))).toBe("RH: #1 |\n");
  expect(run("RH: - |\n", "RH", 0, (s, i) => setPitch(s, i, 72))).toBe("RH: 1' |\n");
  expect(run("RH: . |\n", "RH", 0, (s, i) => setPitch(s, i, 59))).toBe("RH: 7, |\n");
  expect(run("key Eb major\nLH: . |\n", "LH", 0, (s, i) => setPitch(s, i, 46))).toBe("key Eb major\nLH: 5, |\n");
  expect(run("octave RH 5\nRH: . |\n", "RH", 0, (s, i) => setPitch(s, i, 72))).toBe("octave RH 5\nRH: 1 |\n");
});

// ── rhythm ──────────────────────────────────────────────────────────────────

test("split makes thirds as well as halves", () => {
  expect(run("RH: 1 2 |\n", "RH", 0, (s, i) => split(s, i, 3))).toBe("RH: [1 1 1] 2 |\n");
});

test("inserting a slot re-divides its group or bar", () => {
  expect(run("RH: 1 2 3 |\n", "RH", 0, insertSlot)).toBe("RH: 1 . 2 3 |\n");
  expect(run("RH: 1 [2 3] |\n", "RH", 2, insertSlot)).toBe("RH: 1 [2 3 .] |\n");
});

test("deleting a slot re-divides what is left", () => {
  expect(run("RH: 1 2 3 |\n", "RH", 1, deleteSlot)).toBe("RH: 1 3 |\n");
  expect(run("RH: 1 [2 3] |\n", "RH", 1, deleteSlot)).toBe("RH: 1 3 |\n");
  expect(run("RH: 1 [2 3 4] |\n", "RH", 1, deleteSlot)).toBe("RH: 1 [3 4] |\n");
  expect(run("RH: 1 |\n", "RH", 0, deleteSlot)).toBe("RH: . |\n");
});

test("joining a group makes it one slot again", () => {
  expect(run("RH: 1 [2 3] |\n", "RH", 2, joinGroup)).toBe("RH: 1 2 |\n");
  expect(run("RH: [1 [2 3]] 4 |\n", "RH", 2, joinGroup)).toBe("RH: [1 2] 4 |\n");
  expect(run("RH: 1 2 |\n", "RH", 0, joinGroup)).toBe("RH: 1 2 |\n");
});

test("dividing a bar keeps what fits the new grid", () => {
  expect(run("RH: 1 2 |\n", "RH", 0, (s, i) => divideBar(s, i, 4))).toBe("RH: 1 - 2 - |\n");
  expect(run("RH: 1 [2 3] 4 . |\n", "RH", 0, (s, i) => divideBar(s, i, 8))).toBe("RH: 1 - 2 3 4 - . . |\n");
  expect(run("RH: 1 - 2 - |\n", "RH", 0, (s, i) => divideBar(s, i, 2))).toBe("RH: 1 2 |\n");
  // Each slot takes the first note starting within it; one that fits no slot alone is lost.
  expect(run("RH: 1 2 3 |\n", "RH", 0, (s, i) => divideBar(s, i, 2))).toBe("RH: 1 3 |\n");
  // A note held over the barline keeps holding.
  expect(run("RH: 1 | - 2 |\n", "RH", 1, (s, i) => divideBar(s, i, 4))).toBe("RH: 1 | - - 2 - |\n");
});
