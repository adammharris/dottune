import { beforeAll, expect, test } from "bun:test";
import { compile, formatEvents } from "../src/compile";
import { moveBy, setDegree, split, toggleAccidental, toHold, toRest, type TokenEdit } from "../src/edit";
import { applyMusicalEdit, loadFig } from "../src/fig";
import type { Song } from "../src/types";

beforeAll(loadFig);

/** Applies an operation to the first slot whose text and voice match, through fig. */
function run(source: string, voice: string, nth: number, op: (song: Song, index: number) => TokenEdit[]): string {
  const song = compile(source);
  const index = song.slots.findIndex((s, i) => s.voice === voice && song.slots.filter((t, j) => j <= i && t.voice === voice).length === nth + 1);
  return applyMusicalEdit(source, song, index, op);
}

const pitches = (source: string, voice = "RH") =>
  formatEvents(compile(source).events.filter((e) => e.voice === voice))
    .split("\n")
    .map((l) => l.split(" ").slice(2).join(" "));

test("moving a note up a step keeps the next note's pitch", () => {
  // 1 5 is C4 G3; moving 1 up to 2 would put the 5 at G4 without a mark.
  const src = "RH: 1 5 |\n";
  const out = run(src, "RH", 0, (s, i) => moveBy(s, i, 1));
  expect(out).toBe("RH: 2 5, |\n");
  expect(pitches(out)).toEqual(["RH D4", "RH G3"]);
});

test("moving a note an octave adds a mark and fixes the next", () => {
  const out = run("RH: 1 2 3 |\n", "RH", 1, (s, i) => moveBy(s, i, 7));
  expect(pitches(out)).toEqual(["RH C4", "RH D5", "RH E4"]);
});

test("moving a chord picks the diatonic quality", () => {
  // ii sits a step higher, so the V7 after it needs a `,` to stay at G2.
  expect(run("LH: I V7 |\n", "LH", 0, (s, i) => moveBy(s, i, 1))).toBe("LH: ii V7, |\n");
  expect(run("LH: I V7 |\n", "LH", 1, (s, i) => moveBy(s, i, 1))).toBe("LH: I vi7 |\n");
  expect(run("LH: V |\n", "LH", 0, (s, i) => moveBy(s, i, 2))).toBe("LH: viio |\n");
});

test("moving a stack keeps its shape", () => {
  const out = run("RH: 1+3+5 |\n", "RH", 0, (s, i) => moveBy(s, i, 1));
  expect(out).toBe("RH: 2+4+6 |\n");
});

test("typing a degree replaces a rest", () => {
  const out = run("RH: 1 . 3 |\n", "RH", 1, (s, i) => setDegree(s, i, 4));
  expect(pitches(out)).toEqual(["RH C4", "RH G3", "RH E4"]);
});

test("rest and hold keep later notes in place", () => {
  const src = "RH: 1 5 1 |\n";
  expect(pitches(run(src, "RH", 1, (s, i) => toRest(s, i)))).toEqual(["RH C4", "RH C4"]);
  // Without the 5 (G4) as reference, the 1 would drop to C4; `'` keeps it at C5.
  const held = run("RH: 3 5 1 |\n", "RH", 1, (s, i) => toHold(s, i));
  expect(held).toBe("RH: 3 - 1' |\n");
  expect(pitches(held)).toEqual(["RH E4", "RH C5"]);
});

test("split halves a slot without moving it", () => {
  const out = run("RH: 1' 2 |\n", "RH", 0, (s, i) => split(s, i));
  expect(out).toBe("RH: [1' 1] 2 |\n");
  expect(pitches(out)).toEqual(["RH C5", "RH C5", "RH D5"]);
});

test("accidentals toggle", () => {
  const sharp = run("RH: 4 |\n", "RH", 0, (s, i) => toggleAccidental(s, i, "#"));
  expect(sharp).toBe("RH: #4 |\n");
  expect(run(sharp, "RH", 0, (s, i) => toggleAccidental(s, i, "#"))).toBe("RH: 4 |\n");
});

test("edits inside groups and later blocks reach the right token", () => {
  const src = "RH: 1 [2 3] |\n\nRH: 5 |\n";
  expect(run(src, "RH", 2, (s, i) => setDegree(s, i, 3))).toBe("RH: 1 [2 4] |\n\nRH: 5 |\n");
  expect(run(src, "RH", 3, (s, i) => setDegree(s, i, 0))).toBe("RH: 1 [2 3] |\n\nRH: 1 |\n");
});

test("the next note keeps its pitch across a key change", () => {
  // In G major after `RH: 4`, the 3 is B3 (SPEC 8.26). Moving the 4 up to 5
  // re-anchors on G4 instead of E4, which would lift the 3 to B4 without a mark.
  const src = "RH: 4 |\n\nkey G major\nRH: 3 |\n";
  const out = run(src, "RH", 0, (s, i) => moveBy(s, i, 1));
  expect(out).toBe("RH: 5 |\n\nkey G major\nRH: 3, |\n");
  expect(pitches(out)).toEqual(["RH G4", "RH B3"]);
});

test("chords moved after a key change use the new scale", () => {
  const src = "LH: I |\n\nkey A minor\nLH: i |\n";
  expect(run(src, "LH", 1, (s, i) => moveBy(s, i, 1))).toBe("LH: I |\n\nkey A minor\nLH: iio |\n");
});
