import { expect, test } from "bun:test";
import { compile } from "../src/compile";
import { keySignature, readMidi, writeMidi } from "../src/midi";
import { extractExamples, SPEC_PATH } from "../tools/spec-examples";

const examples = extractExamples(await Bun.file(SPEC_PATH).text()).filter((e) => e.expect.kind === "events");

// Every spec example survives tune → MIDI → notes, track by track.
for (const ex of examples) {
  test(`MIDI round trip: ${ex.name}`, () => {
    const song = compile(ex.source);
    const file = readMidi(writeMidi(song));
    expect(file.format).toBe(1);
    expect(file.division).toBe(480);
    expect(file.tracks.map((t) => t.name)).toEqual(["", ...song.voices]);
    for (const voice of song.voices) {
      const track = file.tracks.find((t) => t.name === voice)!;
      const expected = song.events
        .filter((e) => e.voice === voice)
        .map((e) => [e.start, e.duration, e.midi])
        .sort((a, b) => a[0]! - b[0]! || a[2]! - b[2]!);
      expect(track.notes.map((n) => [n.start, n.duration, n.midi])).toEqual(expected);
    }
  });
}

test("conductor track: tempo, time, key, length", () => {
  const song = compile("key A minor\ntempo 100\ntime 6/8\nRH: 1 - - . . . | . |\n");
  const file = readMidi(writeMidi(song));
  expect(file.tempo).toBe(600_000);
  expect(file.time).toEqual([6, 8]);
  expect(file.key).toEqual({ sf: 0, minor: true });
  // Trailing rests still count toward the length.
  expect(file.length).toBe(2880);
});

test("repeated notes do not cut each other off", () => {
  const file = readMidi(writeMidi(compile("RH: 1 1 1 1 |\n")));
  expect(file.tracks[1]!.notes.map((n) => n.duration)).toEqual([480, 480, 480, 480]);
});

test("key signatures", () => {
  expect(keySignature("C major")).toEqual({ sf: 0, minor: false });
  expect(keySignature("Eb major")).toEqual({ sf: -3, minor: false });
  expect(keySignature("F# minor")).toEqual({ sf: 3, minor: true });
  expect(keySignature("D dorian")).toEqual({ sf: 0, minor: false });
  expect(keySignature("G mixolydian")).toEqual({ sf: 0, minor: false });
  expect(keySignature("Bb lydian")).toEqual({ sf: -1, minor: false });
  expect(keySignature("C# phrygian")).toEqual({ sf: 3, minor: false }); // A major
});

test("changes between blocks become conductor events", () => {
  const song = compile("key C major\ntempo 120\nRH: 1 |\n\nkey E minor\ntempo 90\ntime 3/4\nRH: 1 |\n");
  const file = readMidi(writeMidi(song));
  expect(file.tempos).toEqual([
    { tick: 0, usPerQuarter: 500_000 },
    { tick: 1920, usPerQuarter: 666_667 },
  ]);
  expect(file.times).toEqual([
    { tick: 0, time: [4, 4] },
    { tick: 1920, time: [3, 4] },
  ]);
  expect(file.keys).toEqual([
    { tick: 0, sf: 0, minor: false },
    { tick: 1920, sf: 1, minor: true },
  ]);
  expect(file.length).toBe(1920 + 1440);
});
