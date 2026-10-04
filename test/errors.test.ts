import { expect, test } from "bun:test";
import { compile } from "../src/compile";
import { readMidi, writeMidi } from "../src/midi";
import { TuneError } from "../src/types";

function errorsOf(source: string): TuneError[] {
  try {
    compile(source);
  } catch (e) {
    if (e instanceof TuneError) return e.errors;
    throw e;
  }
  throw new Error("expected an error");
}

/** Each error as `line: text at fault`. */
const faults = (source: string) => errorsOf(source).map((e) => `${e.line}: ${e.span ? source.slice(...e.span) : "-"}`);

test("errors point at the text at fault", () => {
  expect(faults("RH: 1 2 x 4 |")).toEqual(["1: x"]);
  expect(faults("RH: 1 [2 3 4 |")).toEqual(["1: ["]);
  expect(faults("RH: 1 2] |")).toEqual(["1: ]"]);
  expect(faults("RH: 1 [] |")).toEqual(["1: []"]);
  expect(faults("RH: 1 || 2 |")).toEqual(["1: |"]);
  expect(faults("RH: 1 |\nRH: 2 |")).toEqual(["2: RH"]);
  expect(faults("key H major\nRH: 1 |")).toEqual(["1: key H major"]);
  expect(faults("  tempo 2  // slow\nRH: 1 |")).toEqual(["1: tempo 2"]);
  expect(faults("RH: 1 Ih |")).toEqual(["1: Ih"]);
  expect(faults("RH: 1 |\n\ntempo 90")).toEqual(["3: tempo 90"]);
});

test("every error is reported, in order, and the first is thrown", () => {
  const source = "tempo 0\nRH: 1 x | 2 |\nLH: I [ |\n\nfoo 3\nRH: y |";
  expect(faults(source)).toEqual(["1: tempo 0", "2: x", "3: [", "5: foo 3", "6: y"]);
  try {
    compile(source);
  } catch (e) {
    expect((e as TuneError).line).toBe(1);
  }
});

test("a bad bar does not hide errors in the rest of its line", () => {
  expect(faults("RH: 1 x 3 | 4 [5 | y |")).toEqual(["1: x", "1: [", "1: y"]);
  expect(faults("RH: 1 || 2 | z |")).toEqual(["1: |", "1: z"]);
});

test("an error in one voice line does not hide one in the next block", () => {
  expect(faults("RH: 1 || 2 |\n\nRH: 9 |").map((f) => f.split(":")[0])).toEqual(["1", "3"]);
});

test("pitches must be in MIDI's range", () => {
  expect(faults("RH: 1 5'''''' |")).toEqual(["1: 5''''''"]);
  expect(faults("octave LH -1\nLH: 1, |")).toEqual(["2: 1,"]);
  expect(faults("octave LH -1\nLH: I/3 |")).toEqual(["2: I/3"]);
  expect(compile("octave RH 9\nRH: 5 |").events[0]!.midi).toBe(127);
  expect(compile("octave RH -1\nRH: 1 |").events[0]!.midi).toBe(0);
});

test("tempo and time limits", () => {
  expect(compile("tempo 4\nRH: 1 |").tempo).toBe(4);
  expect(compile("tempo 1000\nRH: 1 |").tempo).toBe(1000);
  expect(faults("tempo 1001\nRH: 1 |")).toHaveLength(1);
  expect(compile("time 64/64\nRH: 1 |").time).toEqual([64, 64]);
  expect(faults("time 65/4\nRH: 1 |")).toHaveLength(1);
  expect(faults("time 3/128\nRH: 1 |")).toHaveLength(1);
  expect(faults("time 0/4\nRH: 1 |")).toHaveLength(1);
});

test("every MIDI value survives a round trip at the limits", () => {
  const file = readMidi(writeMidi(compile("tempo 4\ntime 64/64\noctave RH 9\nRH: 5 |")));
  expect(file.tempo).toBe(15_000_000);
  expect(file.time).toEqual([64, 64]);
  expect(file.tracks[1]!.notes[0]!.midi).toBe(127);
});

test("voices past the fifteenth share channels in turn, never the drum channel", () => {
  const source = Array.from({ length: 17 }, (_, i) => `V${i}: 1 |`).join("\n");
  const channels = readMidi(writeMidi(compile(source))).tracks.slice(1).map((t) => t.notes[0]!.channel);
  expect(channels).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 0, 1]);
});
