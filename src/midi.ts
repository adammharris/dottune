// Standard MIDI Files (SMF): a format-1 writer for songs, and a small reader.
//
// Track 0 is the conductor (tempo, time and key signature); each voice gets
// its own track, named after the voice, and its own channel while they last.
// MIDI has 15 melodic channels, so from the 16th voice on, voices share them
// in turn. Ticks are the event list's own: 480 per quarter note.

import { TICKS_PER_QUARTER, type Song } from "./types";

const VELOCITY = 80;
/** Every channel but 10 (index 9), the drum channel. */
const CHANNELS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];
const LETTERS = "CDEFGAB";
const NATURAL = [0, 2, 4, 5, 7, 9, 11];
/** Sharps (+) or flats (−) in each spelled major key. */
const MAJOR_SIG: Record<string, number> = {
  Cb: -7, Gb: -6, Db: -5, Ab: -4, Eb: -3, Bb: -2, F: -1, C: 0,
  G: 1, D: 2, A: 3, E: 4, B: 5, "F#": 6, "C#": 7,
};
const MODE_ROTATION: Record<string, number> = {
  major: 0, ionian: 0, dorian: 1, phrygian: 2, lydian: 3, mixolydian: 4, minor: 5, aeolian: 5, locrian: 6,
};

/** The key signature of a key such as "D dorian": sharps/flats, and whether it is minor. */
export function keySignature(key: string): { sf: number; minor: boolean } {
  const [tonic = "C", mode = "major"] = key.split(" ");
  const rotation = MODE_ROTATION[mode] ?? 0;
  // The major key this mode is a rotation of, spelled from the tonic's letter.
  const letter = (LETTERS.indexOf(tonic[0]!) - rotation + 7) % 7;
  const tonicPc = NATURAL[LETTERS.indexOf(tonic[0]!)]! + (tonic[1] === "#" ? 1 : tonic[1] === "b" ? -1 : 0);
  const parentPc = tonicPc - NATURAL[rotation]!;
  const acc = ((((parentPc - NATURAL[letter]!) % 12) + 18) % 12) - 6;
  const name = LETTERS[letter]! + (acc > 0 ? "#".repeat(acc) : "b".repeat(-acc));
  // A parent key outside the table (e.g. G# major) has no signature; fall back to none.
  return { sf: MAJOR_SIG[name] ?? 0, minor: rotation === 5 };
}

// ── writing ─────────────────────────────────────────────────────────────────

function vlq(n: number): number[] {
  const out = [n & 0x7f];
  while ((n >>= 7)) out.unshift((n & 0x7f) | 0x80);
  return out;
}

const u32 = (n: number) => [(n >>> 24) & 0xff, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff];
const ascii = (s: string) => [...new TextEncoder().encode(s)];

interface TimedEvent {
  tick: number;
  /** Sorts events at the same tick: note-offs before note-ons, end of track last. */
  order: number;
  bytes: number[];
}

function track(events: TimedEvent[]): number[] {
  events.sort((a, b) => a.tick - b.tick || a.order - b.order);
  const body: number[] = [];
  let now = 0;
  for (const e of events) {
    body.push(...vlq(e.tick - now), ...e.bytes);
    now = e.tick;
  }
  return [...ascii("MTrk"), ...u32(body.length), ...body];
}

const meta = (tick: number, type: number, data: number[], order = 0): TimedEvent => ({
  tick,
  order,
  bytes: [0xff, type, ...vlq(data.length), ...data],
});

export function writeMidi(song: Song): Uint8Array<ArrayBuffer> {
  const end = Math.max(song.length, ...song.events.map((e) => e.start + e.duration));
  const timeSig = (tick: number, [num, den]: [number, number]) => meta(tick, 0x58, [num, Math.log2(den), 24, 8]);
  const keySig = (tick: number, key: string) => {
    const { sf, minor } = keySignature(key);
    return meta(tick, 0x59, [sf & 0xff, minor ? 1 : 0]);
  };
  const tempo = (tick: number, bpm: number) => {
    const us = Math.round(60_000_000 / bpm);
    return meta(tick, 0x51, [(us >> 16) & 0xff, (us >> 8) & 0xff, us & 0xff]);
  };

  const conductor = track([
    timeSig(0, song.time),
    keySig(0, song.key),
    tempo(0, song.tempo),
    ...song.changes.flatMap((c) => [
      ...(c.time ? [timeSig(c.tick, c.time)] : []),
      ...(c.key ? [keySig(c.tick, c.key)] : []),
      ...(c.tempo !== undefined ? [tempo(c.tick, c.tempo)] : []),
    ]),
    meta(end, 0x2f, [], 2),
  ]);

  const voices = song.voices.map((voice, i) => {
    const channel = CHANNELS[i % CHANNELS.length]!;
    const events: TimedEvent[] = [meta(0, 0x03, ascii(voice))];
    for (const e of song.events) {
      if (e.voice !== voice) continue;
      events.push({ tick: e.start, order: 1, bytes: [0x90 | channel, e.midi, VELOCITY] });
      events.push({ tick: e.start + e.duration, order: 0, bytes: [0x80 | channel, e.midi, 64] });
    }
    events.push(meta(end, 0x2f, [], 2));
    return track(events);
  });

  const header = [...ascii("MThd"), ...u32(6), 0, 1, 0, voices.length + 1, TICKS_PER_QUARTER >> 8, TICKS_PER_QUARTER & 0xff];
  return new Uint8Array([...header, ...conductor, ...voices.flat()]);
}

// ── reading ─────────────────────────────────────────────────────────────────

export interface MidiNote {
  start: number;
  duration: number;
  midi: number;
  velocity: number;
  channel: number;
}

export interface MidiTrack {
  name: string;
  notes: MidiNote[];
}

export interface MidiFile {
  format: number;
  division: number;
  /** Microseconds per quarter note, from the first tempo event. */
  tempo?: number;
  time?: [number, number];
  key?: { sf: number; minor: boolean };
  /** Every tempo, time, and key signature event, in file order. */
  tempos: { tick: number; usPerQuarter: number }[];
  times: { tick: number; time: [number, number] }[];
  keys: { tick: number; sf: number; minor: boolean }[];
  length: number;
  tracks: MidiTrack[];
}

/** Reads notes and the tempo, time, and key signature events. Ticks are the file's own. */
export function readMidi(bytes: Uint8Array): MidiFile {
  let p = 0;
  const str = (n: number) => String.fromCharCode(...bytes.subarray(p, (p += n)));
  const u16 = () => (bytes[p++]! << 8) | bytes[p++]!;
  const u32r = () => ((bytes[p++]! << 24) | (bytes[p++]! << 16) | (bytes[p++]! << 8) | bytes[p++]!) >>> 0;
  const vlqr = () => {
    let n = 0;
    let b: number;
    do {
      b = bytes[p++]!;
      n = (n << 7) | (b & 0x7f);
    } while (b & 0x80);
    return n;
  };

  if (str(4) !== "MThd") throw new Error("not a MIDI file");
  const headerLen = u32r();
  const format = u16();
  const ntrks = u16();
  const division = u16();
  p += headerLen - 6;
  if (division & 0x8000) throw new Error("SMPTE time division is not supported");

  const file: MidiFile = { format, division, length: 0, tracks: [], tempos: [], times: [], keys: [] };
  for (let t = 0; t < ntrks && p < bytes.length; t++) {
    if (str(4) !== "MTrk") throw new Error(`track ${t}: missing MTrk`);
    const len = u32r();
    const end = p + len;
    const out: MidiTrack = { name: "", notes: [] };
    const open = new Map<number, { start: number; velocity: number }[]>();
    let tick = 0;
    let running = 0;
    while (p < end) {
      tick += vlqr();
      let status = bytes[p]!;
      if (status & 0x80) p++;
      else status = running;
      if (status === 0xff) {
        const type = bytes[p++]!;
        const len = vlqr();
        const data = bytes.subarray(p, (p += len));
        if (type === 0x03 && !out.name) out.name = new TextDecoder().decode(data);
        if (type === 0x51) file.tempos.push({ tick, usPerQuarter: (data[0]! << 16) | (data[1]! << 8) | data[2]! });
        if (type === 0x58) file.times.push({ tick, time: [data[0]!, 2 ** data[1]!] });
        if (type === 0x59) file.keys.push({ tick, sf: (data[0]! << 24) >> 24, minor: data[1] === 1 });
        if (type === 0x2f) file.length = Math.max(file.length, tick);
      } else if (status === 0xf0 || status === 0xf7) {
        p += vlqr();
      } else {
        running = status;
        const kind = status & 0xf0;
        const channel = status & 0x0f;
        const a = bytes[p++]!;
        const b = kind === 0xc0 || kind === 0xd0 ? 0 : bytes[p++]!;
        const id = (channel << 8) | a;
        if (kind === 0x90 && b > 0) {
          if (!open.has(id)) open.set(id, []);
          open.get(id)!.push({ start: tick, velocity: b });
        } else if (kind === 0x80 || (kind === 0x90 && b === 0)) {
          const on = open.get(id)?.shift();
          if (on) out.notes.push({ start: on.start, duration: tick - on.start, midi: a, velocity: on.velocity, channel });
        }
      }
    }
    p = end;
    file.tempo ??= file.tempos[0]?.usPerQuarter;
    file.time ??= file.times[0]?.time;
    if (!file.key && file.keys[0]) file.key = { sf: file.keys[0].sf, minor: file.keys[0].minor };
    out.notes.sort((x, y) => x.start - y.start || x.midi - y.midi);
    file.tracks.push(out);
  }
  return file;
}
