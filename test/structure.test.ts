import { beforeAll, expect, test } from "bun:test";
import { compile, formatSong } from "../src/compile";
import { applyEdits, loadFig } from "../src/fig";
import {
  addVoice,
  clearDirective,
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
  setDirective,
  splitBlock,
} from "../src/structure";
import type { Song } from "../src/types";

beforeAll(loadFig);

const run = (source: string, op: (source: string, song: Song) => string) => op(source, compile(source));

/** The tune's events from block `b` on, shifted to start at 0. */
const from = (source: string, b: number) => {
  const song = compile(source);
  const t = song.blocks[b]!.start;
  return song.events.filter((e) => e.start >= t).map((e) => `${e.start - t} ${e.duration} ${e.voice} ${e.midi}`);
};

// ── the language ────────────────────────────────────────────────────────────

test("fig edits a voice's bars and a bar's tokens item by item", () => {
  const src = "RH: 1 2 3 4 | 5 | // tune\nLH: I |\n";
  expect(applyEdits(src, (ed) => ed.insertItemRaw(["blocks", 0, "RH"], 1, "6 7"))).toBe("RH: 1 2 3 4 | 6 7 | 5 | // tune\nLH: I |\n");
  expect(applyEdits(src, (ed) => ed.appendValueRaw(["blocks", 0, "RH"], "7"))).toBe("RH: 1 2 3 4 | 5 | 7 | // tune\nLH: I |\n");
  expect(applyEdits(src, (ed) => ed.deleteItem(["blocks", 0, "RH"], 0))).toBe("RH: 5 | // tune\nLH: I |\n");
  expect(applyEdits(src, (ed) => ed.moveItem(["blocks", 0, "RH"], 0, 1))).toBe("RH: 5 | 1 2 3 4 | // tune\nLH: I |\n");
  expect(applyEdits(src, (ed) => ed.insertItemRaw(["blocks", 0, "RH", 0], 1, "5"))).toBe("RH: 1 5 2 3 4 | 5 | // tune\nLH: I |\n");
  expect(applyEdits(src, (ed) => ed.deleteItem(["blocks", 0, "RH", 0], 3))).toBe("RH: 1 2 3 | 5 | // tune\nLH: I |\n");
  expect(applyEdits("RH: 1 [2 3] |\n", (ed) => ed.insertItemRaw(["blocks", 0, "RH", 0, 1], 1, "4"))).toBe("RH: 1 [2 4 3] |\n");
});

test("fig edits the blocks item by item", () => {
  const src = "RH: 1 |\n\nRH: 2 |\n\nRH: 3 |\n";
  expect(applyEdits(src, (ed) => ed.deleteItem(["blocks"], 1))).toBe("RH: 1 |\n\nRH: 3 |\n");
  expect(applyEdits(src, (ed) => ed.moveItem(["blocks"], 2, 0))).toBe("RH: 3 |\n\nRH: 1 |\n\nRH: 2 |\n");
  expect(applyEdits(src, (ed) => ed.appendValueRaw(["blocks"], "LH: I |"))).toBe("RH: 1 |\n\nRH: 2 |\n\nRH: 3 |\n\nLH: I |\n");
});

// ── bars ────────────────────────────────────────────────────────────────────

test("inserting a bar writes rests in every voice that reaches it", () => {
  const src = "RH: 1 2 3 4 | 5 |\nLH: I |\n";
  expect(run(src, (s, g) => insertBar(s, g, 0, 1))).toBe("RH: 1 2 3 4 | . . . . | 5 |\nLH: I |\n");
  expect(run(src, (s, g) => insertBar(s, g, 0, 0))).toBe("RH: . . . . | 1 2 3 4 | 5 |\nLH: . . . . | I |\n");
  // At the end, only the longest voices need it; the rest are padded.
  expect(run(src, (s, g) => insertBar(s, g, 0, 2))).toBe("RH: 1 2 3 4 | 5 | . . . . |\nLH: I |\n");
  expect(run("time 3/4\nRH: 1 |\n", (s, g) => insertBar(s, g, 0, 1))).toBe("time 3/4\nRH: 1 | . . . |\n");
});

test("deleting a bar takes it from every voice", () => {
  const src = "RH: 1 | 2 | 3 |\nLH: I | V |\n";
  expect(run(src, (s, g) => deleteBar(s, g, 0, 1))).toBe("RH: 1 | 3 |\nLH: I |\n");
  expect(run(src, (s, g) => deleteBar(s, g, 0, 2))).toBe("RH: 1 | 2 |\nLH: I | V |\n");
  // A voice whose only bar goes loses its line.
  expect(run("RH: 1 | 2 |\nLH: I |\n", (s, g) => deleteBar(s, g, 0, 0))).toBe("RH: 2 |\n");
  // A block whose only bar goes is deleted.
  expect(run("RH: 1 |\n\nRH: 2 |\n", (s, g) => deleteBar(s, g, 1, 0))).toBe("RH: 1 |\n");
});

test("duplicating a bar copies it after itself", () => {
  const src = "RH: 1 [2 3] | 5 |\nLH: I |\n";
  expect(run(src, (s, g) => duplicateBar(s, g, 0, 0))).toBe("RH: 1 [2 3] | 1 [2 3] | 5 |\nLH: I | I |\n");
  expect(run(src, (s, g) => duplicateBar(s, g, 0, 1))).toBe("RH: 1 [2 3] | 5 | 5 |\nLH: I |\n");
});

test("moving a bar swaps it with its neighbour", () => {
  const src = "RH: 1 | 2 | 3 |\nLH: I | V |\n";
  expect(run(src, (s, g) => moveBar(s, g, 0, 0, 1))).toBe("RH: 2 | 1 | 3 |\nLH: V | I |\n");
  // LH has no bar 3: it rests there, so the bar it moves out of becomes a rest.
  expect(run(src, (s, g) => moveBar(s, g, 0, 1, 1))).toBe("RH: 1 | 3 | 2 |\nLH: I | . | V |\n");
  expect(run(src, (s, g) => moveBar(s, g, 0, 2, -1))).toBe("RH: 1 | 3 | 2 |\nLH: I | . | V |\n");
  expect(run(src, (s, g) => moveBar(s, g, 0, 0, -1))).toBe(src);
});

// ── blocks ──────────────────────────────────────────────────────────────────

test("a new block has the voices of the one before it", () => {
  expect(run("RH: 1 |\nLH: I |\n\nRH: 5 |\n", (s, g) => insertBlock(s, g, 0))).toBe("RH: 1 |\nLH: I |\n\nRH: . . . . |\nLH: . . . . |\n\nRH: 5 |\n");
  expect(run("key G major\n", (s, g) => insertBlock(s, g, 0))).toBe("key G major\n\nRH: . . . . |\n");
  expect(run("", (s, g) => insertBlock(s, g, 0))).toBe("RH: . . . . |\n");
});

test("deleting a block keeps the next one's settings", () => {
  expect(run("RH: 1 |\n\nRH: 2 |\n\nRH: 3 |\n", (s, g) => deleteBlock(s, g, 1))).toBe("RH: 1 |\n\nRH: 3 |\n");
  expect(run("RH: 1 |\n\nRH: 2 |\n", (s, g) => deleteBlock(s, g, 0))).toBe("RH: 2 |\n");
  // The first block goes: the second's change now sits in the header.
  const src = "key C major\nRH: 1 |\n\ntempo 90\nRH: 2 |\n";
  expect(run(src, (s, g) => deleteBlock(s, g, 0))).toBe("key C major\ntempo 90\nRH: 2 |\n");
  // A block with a change goes: the next block keeps what the change set.
  const changed = "RH: 1 |\n\ntempo 90\nkey G major\nRH: 2 |\n\nRH: 3 |\n";
  const out = run(changed, (s, g) => deleteBlock(s, g, 1));
  expect(from(out, 1)).toEqual(from(changed, 2));
  expect(compile(out).blocks[1]!.tempo).toBe(90);
});

test("moving a block keeps every block's settings", () => {
  const src = "key C major\nRH: 1 |\n\ntempo 90\nRH: 2 |\n\nRH: 3 |\n";
  const out = run(src, (s, g) => moveBlock(s, g, 1, -1));
  const before = compile(src);
  const after = compile(out);
  expect(after.blocks.map((b) => b.tempo)).toEqual([90, 120, 90]);
  expect(from(out, 0).slice(0, 1)).toEqual(from(src, 1).slice(0, 1));
  expect(after.events.map((e) => e.midi)).toEqual([before.events[1]!.midi, before.events[0]!.midi, before.events[2]!.midi]);
  expect(run(src, (s, g) => moveBlock(s, g, 2, 1))).toBe(src);
});

test("duplicating a block copies its change", () => {
  const src = "RH: 1 |\n\ntempo 90 // slower\nRH: 2 | 3 |\n\nRH: 4 |\n";
  const out = run(src, (s, g) => duplicateBlock(s, g, 1));
  expect(out).toBe("RH: 1 |\n\ntempo 90 // slower\nRH: 2 | 3 |\n\ntempo 90 // slower\nRH: 2 | 3 |\n\nRH: 4 |\n");
});

test("splitting and joining a block leave the music alone", () => {
  const src = "key D minor\nRH: 1 | 2 | 3 | // melody\nLH: i | V |\n";
  const split = run(src, (s, g) => splitBlock(s, g, 0, 1));
  expect(split).toBe("key D minor\nRH: 1 | // melody\nLH: i |\n\nRH: 2 | 3 |\nLH: V |\n");
  expect(formatSong(compile(split))).toBe(formatSong(compile(src)));
  const twoBars = run(src, (s, g) => splitBlock(s, g, 0, 2));
  expect(twoBars).toBe("key D minor\nRH: 1 | 2 | // melody\nLH: i | V |\n\nRH: 3 |\n");

  const joined = run(split, (s, g) => joinBlocks(s, g, 0));
  expect(joined).toBe("key D minor\nRH: 1 | 2 | 3 | // melody\nLH: i | V |\n");
  // A voice missing from either block rests where it was missing.
  const gappy = "RH: 1 | 2 |\n\nRH: 3 |\nLH: I |\n";
  const out = run(gappy, (s, g) => joinBlocks(s, g, 0));
  expect(out).toBe("RH: 1 | 2 | 3 |\nLH: . | . | I |\n");
  expect(formatSong(compile(out))).toBe(formatSong(compile(gappy)));
  expect(() => run("RH: 1 |\n\ntempo 90\nRH: 2 |\n", (s, g) => joinBlocks(s, g, 0))).toThrow(/starts with a change/);
});

// ── voices ──────────────────────────────────────────────────────────────────

test("adding a voice writes a line of rests", () => {
  expect(run("RH: 1 | 2 |\n", (s, g) => addVoice(s, g, 0, "Bass"))).toBe("RH: 1 | 2 |\nBass: . | . |\n");
  expect(run("RH: 1 |\n\nRH: 2 |\n", (s, g) => addVoice(s, g, 0, "LH"))).toBe("RH: 1 |\nLH: . |\n\nRH: 2 |\n");
  expect(() => run("RH: 1 |\n", (s, g) => addVoice(s, g, 0, "RH"))).toThrow(/already/);
  expect(() => run("RH: 1 |\n", (s, g) => addVoice(s, g, 0, "tempo"))).toThrow(/directive/);
  expect(() => run("RH: 1 |\n", (s, g) => addVoice(s, g, 0, "2x"))).toThrow(/not a voice name/);
});

test("deleting a voice, from one block or all", () => {
  const src = "RH: 1 |\nLH: I |\n\nLH: V |\n\nRH: 2 |\nLH: I |\n";
  expect(run(src, (s, g) => deleteVoice(s, g, "LH", 0))).toBe("RH: 1 |\n\nLH: V |\n\nRH: 2 |\nLH: I |\n");
  expect(run(src, (s, g) => deleteVoice(s, g, "LH", null))).toBe("RH: 1 |\n\nRH: 2 |\n");
});

test("renaming a voice renames its lines and octaves", () => {
  const src = "octave LH 2\nRH: 1 |\nLH: I |\n\noctave LH 3\nLH: V |\n";
  expect(run(src, (s, g) => renameVoice(s, g, "LH", "Bass"))).toBe("octave Bass 2\nRH: 1 |\nBass: I |\n\noctave Bass 3\nBass: V |\n");
  expect(() => run(src, (s, g) => renameVoice(s, g, "LH", "RH"))).toThrow(/already/);
});

test("moving a voice swaps its lines", () => {
  const src = "RH: 1 |\nLH: I |\n\ntempo 90\nRH: 2 |\nLH: V |\n";
  const out = run(src, (s, g) => moveVoice(s, g, "LH", -1));
  expect(out).toBe("LH: I |\nRH: 1 |\n\ntempo 90\nLH: V |\nRH: 2 |\n");
  expect(compile(out).voices).toEqual(["LH", "RH"]);
  expect(() => run("RH: 1 |\n\nLH: I |\n", (s, g) => moveVoice(s, g, "LH", -1))).toThrow(/first appears/);
});

// ── directives ──────────────────────────────────────────────────────────────

test("a block's directives are listed, the header's for the first", () => {
  const song = compile("tempo 90\noctave LH 2\nRH: 1 |\n\nkey G major\noctave RH 5\nRH: 2 |\n");
  expect(song.blocks.map((b) => b.directives)).toEqual([["tempo", "octave LH"], ["key", "octave RH"]]);
});

test("clearing a directive hands the block back the setting before it", () => {
  const src = "tempo 90\nRH: 1 |\n\ntempo 80\noctave RH 5\noctave LH 2\nRH: 2 |\n";
  expect(run(src, (s, g) => clearDirective(s, g, 1, "tempo"))).toBe("tempo 90\nRH: 1 |\n\noctave RH 5\noctave LH 2\nRH: 2 |\n");
  expect(run(src, (s, g) => clearDirective(s, g, 1, "octave RH"))).toBe("tempo 90\nRH: 1 |\n\ntempo 80\noctave LH 2\nRH: 2 |\n");
  expect(run(src, (s, g) => clearDirective(s, g, 0, "tempo"))).toBe("RH: 1 |\n\ntempo 80\noctave RH 5\noctave LH 2\nRH: 2 |\n");
  expect(run("RH: 1 |\n\noctave RH 5\nRH: 2 |\n", (s, g) => clearDirective(s, g, 1, "octave RH"))).toBe("RH: 1 |\n\nRH: 2 |\n");
});

test("setting a directive edits it in place or writes a line", () => {
  expect(run("tempo 90\nRH: 1 |\n", (s, g) => setDirective(s, g, 0, ["tempo"], 100))).toBe("tempo 100\nRH: 1 |\n");
  expect(run("RH: 1 |\n\nRH: 2 |\n", (s, g) => setDirective(s, g, 1, ["key"], "A minor"))).toBe("RH: 1 |\n\nkey A minor\nRH: 2 |\n");
});
