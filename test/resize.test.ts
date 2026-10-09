import { beforeAll, expect, test } from "bun:test";
import { compile, formatEvents } from "../src/compile";
import { lengthen, resize, restart, snapCut, snapStart, type Cut } from "../src/edit";
import { applyTokenEdits, loadFig } from "../src/fig";

beforeAll(loadFig);

/** The `nth` slot of `voice`, as an index into `song.slots`. */
function slotIndex(source: string, voice: string, nth: number): number {
  const slots = compile(source).slots;
  return slots.map((s, i) => [s, i] as const).filter(([s]) => s.voice === voice)[nth]![1];
}

/** Resizes the `nth` slot of RH to end `k`/`n` through RH's `at`th slot. */
function cut(source: string, nth: number, at: number, k = 1, n = 1): string {
  const song = compile(source);
  const c: Cut = { slot: slotIndex(source, "RH", at), k, n };
  return applyTokenEdits(source, resize(song, slotIndex(source, "RH", nth), c));
}

const events = (source: string) => formatEvents(compile(source).events);

test("growing over the grid holds the slots it covers", () => {
  expect(cut("RH: 1 2 3 4 |\n", 0, 2)).toBe("RH: 1 - - 4 |\n");
});

test("shrinking gives holds back as rests", () => {
  expect(cut("RH: 1 - - - |\n", 0, 1)).toBe("RH: 1 - . . |\n");
});

test("an end inside a slot splits it", () => {
  // A dotted quarter: the 2 is struck again halfway through beat 2.
  const out = cut("RH: 1 2 3 4 |\n", 0, 1, 1, 2);
  expect(out).toBe("RH: 1 [- 2] 3 4 |\n");
  expect(events(out)).toBe("0 720 RH C4\n720 240 RH D4\n960 480 RH E4\n1440 480 RH F4");
});

test("splits work inside groups", () => {
  expect(cut("RH: 1 [2 3 4] 5 6 |\n", 1, 2, 1, 2)).toBe("RH: 1 [2 [- 3] 4] 5 6 |\n");
});

test("shrinking inside the note's own slot", () => {
  expect(cut("RH: 1 2 |\n", 0, 0, 1, 2)).toBe("RH: [1 .] 2 |\n");
  expect(cut("RH: 1 2 |\n", 0, 0, 3, 4)).toBe("RH: [1 - - .] 2 |\n");
});

test("growing over a held note strikes what is left of it", () => {
  const out = cut("RH: 1 2 - - |\n", 0, 1);
  expect(out).toBe("RH: 1 - 2 - |\n");
  expect(events(out)).toBe("0 960 RH C4\n960 960 RH D4");
});

test("growing over a rest does not let its hold join the note", () => {
  const out = cut("RH: 1 . - 3 |\n", 0, 1);
  expect(out).toBe("RH: 1 - . 3 |\n");
});

test("touched groups are written in their coarsest form", () => {
  // [1 .] grown to the end of its slot is just 1.
  expect(cut("RH: [1 .] 2 |\n", 0, 1)).toBe("RH: 1 2 |\n");
  expect(cut("RH: [1 . . .] 2 |\n", 0, 1)).toBe("RH: [1 .] 2 |\n");
  // A group the edit does not reach keeps its spelling.
  expect(cut("RH: 1 2 [3 -] |\n", 0, 1)).toBe("RH: 1 - [3 -] |\n");
});

test("holds cross barlines and blocks", () => {
  expect(cut("RH: 1 2 | 3 4 |\n", 1, 2)).toBe("RH: 1 2 | - 4 |\n");
  expect(cut("RH: 1 2 |\n\nRH: 3 4 |\n", 1, 2)).toBe("RH: 1 2 |\n\nRH: - 4 |\n");
});

test("other voices and spacing are untouched", () => {
  const src = "RH: 1   2   3   4 |\nLH: I       V      |\n";
  expect(cut(src, 0, 1, 1, 2)).toBe("RH: 1   [- 2]   3   4 |\nLH: I       V      |\n");
});

test("chords and stacks resize like notes", () => {
  expect(cut("RH: 1+3+5 . |\n", 0, 1)).toBe("RH: 1+3+5 - |\n");
});

test("rests, holds, and padding cannot be resized or grown into", () => {
  const src = "RH: . - |\nLH: I | I |\n";
  const song = compile(src);
  expect(resize(song, slotIndex(src, "RH", 0), { slot: slotIndex(src, "RH", 1), k: 1, n: 1 })).toEqual([]);
  const padded = "RH: 1 |\nLH: I | I |\n";
  expect(lengthen(compile(padded), slotIndex(padded, "RH", 0), 1)).toEqual([]);
});

test("snapping picks the nearest point in the slot under the cursor", () => {
  const src = "RH: 1 2 3 4 |\n";
  const song = compile(src);
  const i = slotIndex(src, "RH", 0);
  // 1.4 beats in, in halves of a beat: the half beat at 1.5.
  expect(snapCut(song, i, 480 * 1.4, () => 2)).toEqual({ slot: slotIndex(src, "RH", 1), k: 1, n: 2 });
  // Past the end: the end of the last slot.
  expect(snapCut(song, i, 99999, () => 1)).toEqual({ slot: slotIndex(src, "RH", 3), k: 1, n: 1 });
  // Before the note: never shorter than its first part.
  expect(snapCut(song, i, -5, () => 4)).toEqual({ slot: i, k: 1, n: 4 });
});

test("the keyboard grows by a slot and shrinks by a hold, then by halves", () => {
  const src = "RH: 1 2 3 4 |\n";
  const at = (s: string) => slotIndex(s, "RH", 0);
  const grown = applyTokenEdits(src, lengthen(compile(src), at(src), 1));
  expect(grown).toBe("RH: 1 - 3 4 |\n");
  const back = applyTokenEdits(grown, lengthen(compile(grown), at(grown), -1));
  expect(back).toBe("RH: 1 . 3 4 |\n");
  expect(applyTokenEdits(back, lengthen(compile(back), at(back), -1))).toBe("RH: [1 .] . 3 4 |\n");
});

/** Moves the start of the `nth` slot of RH to `k`/`n` through RH's `at`th slot. */
function start(source: string, nth: number, at: number, k = 0, n = 1): string {
  const song = compile(source);
  return applyTokenEdits(source, restart(song, slotIndex(source, "RH", nth), { slot: slotIndex(source, "RH", at), k, n }));
}

test("starting earlier takes over the slots before, cutting short what was there", () => {
  expect(start("RH: 1 2 3 4 |\n", 2, 1)).toBe("RH: 1 3 - 4 |\n");
  expect(start("RH: 1 - 3 4 |\n", 2, 1, 1, 2)).toBe("RH: 1 [- 3] - 4 |\n");
  expect(start("RH: 1 2 | 3 4 |\n", 2, 1)).toBe("RH: 1 3 | - 4 |\n");
});

test("starting later leaves a rest and keeps the end", () => {
  expect(start("RH: 1 - - 4 |\n", 0, 1)).toBe("RH: . 1 - 4 |\n");
  expect(start("RH: 1 2 |\n", 0, 0, 1, 2)).toBe("RH: [. 1] 2 |\n");
  const out = start("RH: 1 - - 4 |\n", 0, 2, 1, 2);
  expect(out).toBe("RH: . . [. 1] 4 |\n");
  expect(events(out)).toBe("1200 240 RH C4\n1440 480 RH F4");
});

test("a start never reaches the note's end or crosses padding", () => {
  const song = compile("RH: 1 2 |\n");
  const i = slotIndex("RH: 1 2 |\n", "RH", 0);
  expect(snapStart(song, i, 9999, () => 2)).toEqual({ slot: i, k: 1, n: 2 });
  expect(restart(song, i, { slot: i, k: 1, n: 1 })).toEqual([]);
  const padded = "LH: I | I |\nRH: . |\n\nRH: 1 |\n";
  expect(snapStart(compile(padded), slotIndex(padded, "RH", 2), 0, () => 1)).toEqual({ slot: slotIndex(padded, "RH", 2), k: 0, n: 1 });
});
