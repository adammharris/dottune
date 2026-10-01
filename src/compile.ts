import { TICKS_PER_QUARTER, TuneError, type BlockInfo, type Change, type NoteEvent, type SlotInfo, type SlotKind, type Song } from "./types";

// ---------------------------------------------------------------------------
// Exact time: positions are fractions until output (SPEC §3.3).

interface Q {
  n: number;
  d: number;
}

function gcd(a: number, b: number): number {
  a = Math.abs(a);
  b = Math.abs(b);
  while (b) [a, b] = [b, a % b];
  return a;
}

function q(n: number, d = 1): Q {
  if (d < 0) [n, d] = [-n, -d];
  const g = gcd(n, d) || 1;
  return { n: n / g, d: d / g };
}

const add = (a: Q, b: Q) => q(a.n * b.d + b.n * a.d, a.d * b.d);
const mul = (a: Q, k: number) => q(a.n * k, a.d);
const div = (a: Q, k: number) => q(a.n, a.d * k);
const ticks = (a: Q) => Math.round(a.n / a.d);

// ---------------------------------------------------------------------------
// Theory

const IONIAN = [0, 2, 4, 5, 7, 9, 11];
const MODES: Record<string, number> = {
  major: 0,
  ionian: 0,
  dorian: 1,
  phrygian: 2,
  lydian: 3,
  mixolydian: 4,
  minor: 5,
  aeolian: 5,
  locrian: 6,
};
const LETTERS: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };
const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

function modeScale(rotation: number): number[] {
  return IONIAN.map((_, i) => (IONIAN[(i + rotation) % 7]! - IONIAN[rotation]! + 12) % 12);
}

export const mod = (a: number, m: number) => ((a % m) + m) % m;

export function pitchName(midi: number): string {
  return `${NAMES[mod(midi, 12)]}${Math.floor(midi / 12) - 1}`;
}

// ---------------------------------------------------------------------------
// Parsing

const DIRECTIVES = new Set(["key", "tempo", "time", "octave"]);
const VOICE_NAME = "[A-Za-z][A-Za-z0-9_]*";

interface Header {
  key: string;
  tonicPc: number;
  scale: number[];
  tempo: number;
  time: [number, number];
  octaves: Map<string, number>;
}

type Item = { kind: "token"; text: string } | { kind: "group"; items: Item[] };

interface VoiceLine {
  name: string;
  line: number;
  bars: Item[][];
}

function stripComment(line: string): string {
  const i = line.indexOf("//");
  return i === -1 ? line : line.slice(0, i);
}

/** Applies a directive to `h`; for `octave`, returns the voice it names. */
function parseDirective(text: string, line: number, h: Header): string | null {
  const word = text.split(/\s+/)[0]!;
  let m: RegExpExecArray | null;
  switch (word) {
    case "key":
      m = /^key\s+([A-G])([#b]?)(?:\s+([A-Za-z]+))?$/.exec(text);
      if (!m) throw new TuneError(line, `invalid key: "${text}"`);
      const mode = (m[3] ?? "major").toLowerCase();
      if (!(mode in MODES)) throw new TuneError(line, `unknown mode "${m[3]}"`);
      h.tonicPc = LETTERS[m[1]!]! + (m[2] === "#" ? 1 : m[2] === "b" ? -1 : 0);
      h.scale = modeScale(MODES[mode]!);
      h.key = `${m[1]}${m[2]} ${mode}`;
      return null;
    case "tempo":
      m = /^tempo\s+(\d+(?:\.\d+)?)$/.exec(text);
      if (!m || Number(m[1]) <= 0) throw new TuneError(line, `invalid tempo: "${text}"`);
      h.tempo = Number(m[1]);
      return null;
    case "time":
      m = /^time\s+(\d+)\s*\/\s*(\d+)$/.exec(text);
      if (!m || Number(m[1]) <= 0 || Number(m[2]) <= 0)
        throw new TuneError(line, `invalid time signature: "${text}"`);
      h.time = [Number(m[1]), Number(m[2])];
      return null;
    case "octave":
      m = new RegExp(`^octave\\s+(${VOICE_NAME})\\s+(-?\\d+)$`).exec(text);
      if (!m) throw new TuneError(line, `expected "octave <voice> <n>"`);
      h.octaves.set(m[1]!, Number(m[2]));
      return m[1]!;
    default:
      throw new TuneError(line, `unknown directive "${word}"`);
  }
}

function parseBar(text: string, line: number): Item[] {
  const words = text.replace(/\[/g, " [ ").replace(/\]/g, " ] ").trim().split(/\s+/);
  const stack: Item[][] = [[]];
  for (const w of words) {
    if (w === "[") {
      stack.push([]);
    } else if (w === "]") {
      if (stack.length === 1) throw new TuneError(line, `unbalanced "]"`);
      const items = stack.pop()!;
      if (items.length === 0) throw new TuneError(line, `empty group "[]"`);
      stack.at(-1)!.push({ kind: "group", items });
    } else {
      stack.at(-1)!.push({ kind: "token", text: w });
    }
  }
  if (stack.length !== 1) throw new TuneError(line, `unbalanced "["`);
  return stack[0]!;
}

function parseBars(text: string, line: number): Item[][] {
  const parts = text.split("|");
  if (parts.length > 1 && parts.at(-1)!.trim() === "") parts.pop();
  return parts.map((p) => {
    if (p.trim() === "") throw new TuneError(line, "empty bar");
    return parseBar(p, line);
  });
}

// ---------------------------------------------------------------------------
// Pitch

interface VoiceState {
  /** Diatonic index of the reference note, relative to the home tonic. */
  ref: number | null;
  tonicMidi: number;
}

export const NOTE = /^([#b]?)([1-7])([',]*)$/;
export const CHORD = /^([#b]?)(VII|VI|V|IV|III|II|I|vii|vi|v|iv|iii|ii|i)([oh+]?)(maj7|7)?([',]*)(?:\/([#b]?)([1-7]))?$/;
export const NUMERALS = ["i", "ii", "iii", "iv", "v", "vi", "vii"];

/** The octave a voice's first note is placed in, absent an `octave` directive (SPEC §4.3). */
export const defaultOctave = (voice: string) => (voice === "LH" ? 3 : 4);

const accidental = (s: string | undefined) => (s === "#" ? 1 : s === "b" ? -1 : 0);
const octaveMarks = (s: string) => [...s].reduce((n, c) => n + (c === "'" ? 1 : -1), 0);

export function nearest(ref: number | null, degree: number): number {
  if (ref === null) return degree;
  const d = mod(degree - mod(ref, 7), 7);
  return ref + (d <= 3 ? d : d - 7);
}

function midiAt(idx: number, acc: number, st: VoiceState, h: Header): number {
  return st.tonicMidi + 12 * Math.floor(idx / 7) + h.scale[mod(idx, 7)]! + acc;
}

/** Returns the MIDI notes a sounding token produces, updating the voice reference. */
function sound(text: string, line: number, st: VoiceState, h: Header): number[] {
  let m = NOTE.exec(text);
  if (m) {
    const idx = nearest(st.ref, Number(m[2]) - 1) + 7 * octaveMarks(m[3]!);
    st.ref = idx;
    return [midiAt(idx, accidental(m[1]), st, h)];
  }

  m = CHORD.exec(text);
  if (m) {
    const [, acc, numeral, quality, seventh, marks, bassAcc, bassDeg] = m;
    const upper = numeral === numeral!.toUpperCase();
    if (quality === "h" && seventh !== "7") throw new TuneError(line, `"h" must be followed by "7" in "${text}"`);
    const triad =
      quality === "o" || quality === "h" ? [0, 3, 6] : quality === "+" ? [0, 4, 8] : upper ? [0, 4, 7] : [0, 3, 7];
    if (seventh === "maj7") triad.push(11);
    else if (seventh === "7") triad.push(quality === "o" ? 9 : 10);

    const rootIdx = nearest(st.ref, NUMERALS.indexOf(numeral!.toLowerCase())) + 7 * octaveMarks(marks!);
    st.ref = rootIdx;
    const root = midiAt(rootIdx, accidental(acc), st, h);
    const notes = triad.map((i) => root + i);
    if (bassDeg) {
      const below = mod(rootIdx - (Number(bassDeg) - 1), 7) || 7;
      notes.unshift(midiAt(rootIdx - below, accidental(bassAcc), st, h));
    }
    return notes;
  }

  if (text.includes("+")) {
    const members = text.split("+");
    const notes: number[] = [];
    let prev: number | null = null;
    for (const member of members) {
      const mm = NOTE.exec(member);
      if (!mm) throw new TuneError(line, `invalid stack "${text}"`);
      const degree = Number(mm[2]) - 1;
      let idx: number;
      if (prev === null) {
        idx = nearest(st.ref, degree);
      } else {
        idx = prev + (mod(degree - mod(prev, 7), 7) || 7);
      }
      idx += 7 * octaveMarks(mm[3]!);
      if (prev === null) st.ref = idx;
      prev = idx;
      notes.push(midiAt(idx, accidental(mm[1]), st, h));
    }
    return notes;
  }

  throw new TuneError(line, `unknown token "${text}"`);
}

// ---------------------------------------------------------------------------
// Compile

interface Slot {
  start: Q;
  end: Q;
  text: string;
  line: number;
  block: number;
  bar: number;
  path: number[];
  editable: boolean;
}

function kindOf(text: string): SlotKind {
  if (text === "-") return "hold";
  if (text === ".") return "rest";
  if (NOTE.test(text)) return "note";
  if (CHORD.test(text)) return "chord";
  return "stack";
}

interface Block {
  voices: VoiceLine[];
  /** The settings in force for this block. */
  settings: Header;
  /** Which directives the change before this block set, and which voices it reset (SPEC §2.1). */
  directives: Set<string>;
  resets: Set<string>;
  /** First voice line. */
  line: number;
}

const snapshot = (h: Header): Header => ({ ...h, time: [...h.time], octaves: new Map(h.octaves) });
const sameKey = (a: Header, b: Header) => a.tonicPc === b.tonicPc && a.scale.join() === b.scale.join();

/** The diatonic index, in a new key, nearest a pitch; ties go to the lower (SPEC §2.1). */
function reanchor(pitch: number, tonicMidi: number, scale: number[]): number {
  const base = Math.floor((pitch - tonicMidi) / 12) * 7;
  let best = base;
  let bestDist = Infinity;
  for (let idx = base - 7; idx <= base + 14; idx++) {
    const dist = Math.abs(tonicMidi + 12 * Math.floor(idx / 7) + scale[mod(idx, 7)]! - pitch);
    if (dist < bestDist) [best, bestDist] = [idx, dist];
  }
  return best;
}

export function compile(source: string): Song {
  const cur: Header = {
    key: "C major",
    tonicPc: 0,
    scale: modeScale(0),
    tempo: 120,
    time: [4, 4],
    octaves: new Map(),
  };
  const blocks: Block[] = [];
  let block: Block | null = null;
  /** Directives read since the last block: the change the next block starts with. */
  let pending = { directives: new Set<string>(), resets: new Set<string>(), firstLine: 0 };

  const lines = source.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const lineNo = i + 1;
    const raw = lines[i]!;
    if (raw.trim() === "") {
      block = null;
      continue;
    }
    const text = stripComment(raw).trim();
    if (text === "") continue;

    const vm = new RegExp(`^(${VOICE_NAME})\\s*:(.*)$`).exec(text);
    if (vm) {
      const name = vm[1]!;
      if (DIRECTIVES.has(name)) throw new TuneError(lineNo, `"${name}" is a directive and cannot name a voice`);
      if (!block) {
        block = { voices: [], settings: snapshot(cur), directives: pending.directives, resets: pending.resets, line: lineNo };
        blocks.push(block);
        pending = { directives: new Set(), resets: new Set(), firstLine: 0 };
      }
      if (block.voices.some((v) => v.name === name))
        throw new TuneError(lineNo, `voice "${name}" appears twice in one block`);
      if (vm[2]!.trim() === "") throw new TuneError(lineNo, "empty bar");
      block.voices.push({ name, line: lineNo, bars: parseBars(vm[2]!, lineNo) });
      continue;
    }

    const word = text.split(/\s+/)[0]!;
    if (block) {
      if (DIRECTIVES.has(word))
        throw new TuneError(lineNo, `"${word}" is inside a block; changes go between blocks, after a blank line`);
      throw new TuneError(lineNo, `expected "Name: bars…", got "${text}"`);
    }
    const reset = parseDirective(text, lineNo, cur);
    pending.directives.add(word);
    if (reset) pending.resets.add(reset);
    pending.firstLine ||= lineNo;
  }
  if (blocks.length > 0 && pending.firstLine)
    throw new TuneError(pending.firstLine, "a change must be followed by a block");

  const voices: string[] = [];
  for (const b of blocks) for (const v of b.voices) if (!voices.includes(v.name)) voices.push(v.name);

  // Lay every voice out on the timeline (SPEC §1.1, §3).
  const slots = new Map<string, Slot[]>(voices.map((v) => [v, []]));
  const blockInfos: BlockInfo[] = [];
  const changes: Change[] = [];
  let blockStart = q(0);

  type Origin = Pick<Slot, "line" | "block" | "bar" | "path">;
  const layout = (items: Item[], start: Q, len: Q, at: Origin, out: Slot[]) => {
    const each = div(len, items.length);
    items.forEach((item, j) => {
      const s = add(start, mul(each, j));
      const here = { ...at, path: [...at.path, j] };
      if (item.kind === "token") out.push({ start: s, end: add(s, each), text: item.text, editable: true, ...here });
      else layout(item.items, s, each, here, out);
    });
  };

  for (const [blockIndex, b] of blocks.entries()) {
    const h = b.settings;
    const barLen = q(h.time[0] * 4 * TICKS_PER_QUARTER, h.time[1]);
    const nBars = Math.max(...b.voices.map((v) => v.bars.length));
    for (const name of voices) {
      const vl = b.voices.find((v) => v.name === name);
      const out = slots.get(name)!;
      for (let k = 0; k < nBars; k++) {
        const start = add(blockStart, mul(barLen, k));
        const bar = vl?.bars[k];
        if (bar) layout(bar, start, barLen, { line: vl.line, block: blockIndex, bar: k, path: [] }, out);
        else
          out.push({ start, end: add(start, barLen), text: ".", line: vl?.line ?? 0, block: blockIndex, bar: k, path: [], editable: false });
      }
    }
    const blockEnd = add(blockStart, mul(barLen, nBars));

    const prev = blocks[blockIndex - 1]?.settings;
    if (prev) {
      const change: Change = { tick: ticks(blockStart) };
      if (prev.time.join("/") !== h.time.join("/")) change.time = h.time;
      if (!sameKey(prev, h)) change.key = h.key;
      if (prev.tempo !== h.tempo) change.tempo = h.tempo;
      if (change.time || change.key || change.tempo !== undefined) changes.push(change);
    }
    blockInfos.push({
      start: ticks(blockStart),
      end: ticks(blockEnd),
      line: b.line,
      key: h.key,
      scale: h.scale,
      tempo: h.tempo,
      time: h.time,
      octaves: Object.fromEntries(h.octaves),
      directives: blockIndex === 0 ? [] : [...b.directives],
    });
    blockStart = blockEnd;
  }

  // Turn slots into events.
  const events: NoteEvent[] = [];
  const slotInfos: SlotInfo[] = [];
  for (const name of voices) {
    const st: VoiceState = { ref: null, tonicMidi: 0 };
    let h = cur;
    let blockIndex = -1;
    let sounding: { start: Q; end: Q; notes: number[]; slot: number } | null = null;

    const flush = () => {
      if (!sounding) return;
      const start = ticks(sounding.start);
      const duration = ticks(sounding.end) - start;
      for (const midi of sounding.notes) events.push({ start, duration, voice: name, midi, slot: sounding.slot });
      sounding = null;
    };

    /** Entering a block: apply its change to this voice (SPEC §2.1). */
    const enter = (b: Block) => {
      const next = b.settings;
      const tonicMidi = 12 * ((next.octaves.get(name) ?? defaultOctave(name)) + 1) + next.tonicPc;
      if (b.resets.has(name)) st.ref = null;
      else if (st.ref !== null && !sameKey(h, next)) st.ref = reanchor(midiAt(st.ref, 0, st, h), tonicMidi, next.scale);
      st.tonicMidi = tonicMidi;
      h = next;
    };

    for (const slot of slots.get(name)!) {
      if (slot.block !== blockIndex) enter(blocks[(blockIndex = slot.block)]!);
      const refBefore = st.ref;
      let notes: number[] = [];
      if (slot.text === "-") {
        if (sounding) sounding.end = slot.end;
      } else if (slot.text === ".") {
        flush();
      } else {
        notes = sound(slot.text, slot.line, st, h);
        flush();
        sounding = { start: slot.start, end: slot.end, notes, slot: slotInfos.length };
      }
      slotInfos.push({
        voice: name,
        block: slot.block,
        bar: slot.bar,
        path: slot.path,
        editable: slot.editable,
        text: slot.text,
        kind: kindOf(slot.text),
        start: ticks(slot.start),
        end: ticks(slot.end),
        refBefore,
        ref: st.ref,
        midi: notes,
      });
    }
    flush();
  }

  events.sort(
    (a, b) => a.start - b.start || voices.indexOf(a.voice) - voices.indexOf(b.voice) || a.midi - b.midi,
  );

  const first = blocks[0]?.settings ?? cur;
  return {
    key: first.key,
    scale: first.scale,
    octaves: Object.fromEntries(first.octaves),
    tempo: first.tempo,
    time: first.time,
    length: ticks(blockStart),
    voices,
    events,
    slots: slotInfos,
    blocks: blockInfos,
    changes,
  };
}

/** The settings in force at a tick. */
export function settingsAt(song: Song, tick: number): BlockInfo | undefined {
  let found = song.blocks[0];
  for (const b of song.blocks) if (b.start <= tick) found = b;
  return found;
}

/** The text form of a song: changes and note events (SPEC §6). */
export function formatSong(song: Song): string {
  const lines: [number, number, string][] = [];
  for (const c of song.changes) {
    if (c.time) lines.push([c.tick, 0, `${c.tick} time ${c.time.join("/")}`]);
    if (c.key) lines.push([c.tick, 1, `${c.tick} key ${c.key}`]);
    if (c.tempo !== undefined) lines.push([c.tick, 2, `${c.tick} tempo ${c.tempo}`]);
  }
  song.events.forEach((e, i) => lines.push([e.start, 3 + i, `${e.start} ${e.duration} ${e.voice} ${pitchName(e.midi)}`]));
  return lines
    .sort((a, b) => a[0] - b[0] || a[1] - b[1])
    .map((l) => l[2])
    .join("\n");
}

/** The text form of an event list (SPEC §6). */
export function formatEvents(events: NoteEvent[]): string {
  return events.map((e) => `${e.start} ${e.duration} ${e.voice} ${pitchName(e.midi)}`).join("\n");
}
