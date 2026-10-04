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

/** A token, with its character offsets in the source, or a `[ ]` group. */
type Item = { kind: "token"; text: string; from: number; to: number } | { kind: "group"; items: Item[] };

interface VoiceLine {
  name: string;
  line: number;
  bars: Item[][];
}

function stripComment(line: string): string {
  const i = line.indexOf("//");
  return i === -1 ? line : line.slice(0, i);
}

/** Tempo limits: below 4 bpm a quarter note is too long for MIDI's tempo field. */
const MIN_TEMPO = 4;
const MAX_TEMPO = 1000;
/** The longest bar numerator, and the denominators allowed (powers of two, as MIDI requires). */
const MAX_BEATS = 64;
const DENOMINATORS = [1, 2, 4, 8, 16, 32, 64];
/** MIDI's pitch range: C-1 to G9. */
const LOWEST = 0;
const HIGHEST = 127;

/** Applies a directive to `h`; `span` is where `text` sits in the source. */
function parseDirective(text: string, line: number, h: Header, span: [number, number]): void {
  const word = text.split(/\s+/)[0]!;
  const fail = (reason: string) => new TuneError(line, reason, span);
  let m: RegExpExecArray | null;
  switch (word) {
    case "key":
      m = /^key\s+([A-G])([#b]?)(?:\s+([A-Za-z]+))?$/.exec(text);
      if (!m) throw fail(`invalid key: "${text}"`);
      const mode = (m[3] ?? "major").toLowerCase();
      if (!(mode in MODES)) throw fail(`unknown mode "${m[3]}"`);
      h.tonicPc = LETTERS[m[1]!]! + (m[2] === "#" ? 1 : m[2] === "b" ? -1 : 0);
      h.scale = modeScale(MODES[mode]!);
      h.key = `${m[1]}${m[2]} ${mode}`;
      return;
    case "tempo":
      m = /^tempo\s+(\d+(?:\.\d+)?)$/.exec(text);
      if (!m) throw fail(`invalid tempo: "${text}"`);
      if (Number(m[1]) < MIN_TEMPO || Number(m[1]) > MAX_TEMPO)
        throw fail(`tempo must be from ${MIN_TEMPO} to ${MAX_TEMPO}, got ${m[1]}`);
      h.tempo = Number(m[1]);
      return;
    case "time":
      m = /^time\s+(\d+)\s*\/\s*(\d+)$/.exec(text);
      if (!m) throw fail(`invalid time signature: "${text}"`);
      if (Number(m[1]) < 1 || Number(m[1]) > MAX_BEATS)
        throw fail(`a bar must have from 1 to ${MAX_BEATS} beats, got ${m[1]}`);
      if (!DENOMINATORS.includes(Number(m[2])))
        throw fail(`the beat must be one of ${DENOMINATORS.join(", ")}, got ${m[2]}`);
      h.time = [Number(m[1]), Number(m[2])];
      return;
    case "octave":
      m = new RegExp(`^octave\\s+(${VOICE_NAME})\\s+(-?\\d+)$`).exec(text);
      if (!m) throw fail(`expected "octave <voice> <n>"`);
      if (DIRECTIVES.has(m[1]!)) throw fail(`"${m[1]}" is a directive and cannot name a voice`);
      h.octaves.set(m[1]!, Number(m[2]));
      return;
    default:
      throw fail(`unknown directive "${word}"`);
  }
}

/** Parses one bar; `offset` is where `text` starts in the source. */
function parseBar(text: string, line: number, offset: number): Item[] {
  const stack: Item[][] = [[]];
  /** Where each open `[` is. */
  const opens: number[] = [];
  for (const m of text.matchAll(/\[|\]|[^\s[\]]+/g)) {
    const w = m[0];
    const at = offset + m.index;
    if (w === "[") {
      stack.push([]);
      opens.push(at);
    } else if (w === "]") {
      if (stack.length === 1) throw new TuneError(line, `unbalanced "]"`, [at, at + 1]);
      const items = stack.pop()!;
      const open = opens.pop()!;
      if (items.length === 0) throw new TuneError(line, `empty group "[]"`, [open, at + 1]);
      stack.at(-1)!.push({ kind: "group", items });
    } else {
      stack.at(-1)!.push({ kind: "token", text: w, from: at, to: at + w.length });
    }
  }
  if (stack.length !== 1) throw new TuneError(line, `unbalanced "["`, [opens.at(-1)!, opens.at(-1)! + 1]);
  return stack[0]!;
}

/**
 * Parses a voice line's bars. A bar that fails to parse is reported to
 * `onError` and read as a rest, so the bars after it are still checked.
 */
function parseBars(text: string, line: number, offset: number, onError: (e: TuneError) => void): Item[][] {
  const parts = text.split("|");
  if (parts.length > 1 && parts.at(-1)!.trim() === "") parts.pop();
  const end = offset + text.length;
  return parts.map((p) => {
    let bar: Item[];
    try {
      if (p.trim() === "") {
        // An empty bar is marked by the `|` that closes it, or the one before it at the end of the line.
        const at = offset + p.length < end ? offset + p.length : offset - 1;
        throw new TuneError(line, "empty bar", [at, at + 1]);
      }
      bar = parseBar(p, line, offset);
    } catch (e) {
      if (!(e instanceof TuneError)) throw e;
      onError(e);
      bar = [{ kind: "token", text: ".", from: offset, to: offset }];
    }
    offset += p.length + 1;
    return bar;
  });
}

// ---------------------------------------------------------------------------
// Pitch

/** Where a voice's notes are placed: the tonic of its octave, and the key's scale (SPEC §4.2). */
interface Place {
  tonicMidi: number;
  scale: number[];
}

export const NOTE = /^([#b]?)([1-7])([',]*)$/;
export const CHORD = /^([#b]?)(VII|VI|V|IV|III|II|I|vii|vi|v|iv|iii|ii|i)([oh+]?)(maj7|7)?([',]*)(?:\/([#b]?)([1-7]))?$/;
export const NUMERALS = ["i", "ii", "iii", "iv", "v", "vi", "vii"];

/** A voice's octave, absent an `octave` directive (SPEC §4.2). */
export const defaultOctave = (voice: string) => (voice === "LH" ? 3 : 4);

const accidental = (s: string | undefined) => (s === "#" ? 1 : s === "b" ? -1 : 0);
const octaveMarks = (s: string) => [...s].reduce((n, c) => n + (c === "'" ? 1 : -1), 0);

/** The pitch of a diatonic index (0 = the tonic of the voice's octave), plus an accidental. */
const midiAt = (idx: number, acc: number, p: Place) => p.tonicMidi + 12 * Math.floor(idx / 7) + p.scale[mod(idx, 7)]! + acc;

/** The diatonic index a degree (1–7) and its octave marks name. */
const indexOf = (degree: number, marks: string) => degree - 1 + 7 * octaveMarks(marks);

/** Returns the MIDI notes a sounding token produces, checking they are in MIDI's range. */
function sound(text: string, line: number, p: Place, span: [number, number] | null): number[] {
  const notes = spell(text, line, p, span);
  for (const midi of notes) {
    if (midi < LOWEST) throw new TuneError(line, `"${text}" is below ${pitchName(LOWEST)}, the lowest MIDI note`, span);
    if (midi > HIGHEST) throw new TuneError(line, `"${text}" is above ${pitchName(HIGHEST)}, the highest MIDI note`, span);
  }
  return notes;
}

function spell(text: string, line: number, p: Place, span: [number, number] | null): number[] {
  let m = NOTE.exec(text);
  if (m) return [midiAt(indexOf(Number(m[2]), m[3]!), accidental(m[1]), p)];

  m = CHORD.exec(text);
  if (m) {
    const [, acc, numeral, quality, seventh, marks, bassAcc, bassDeg] = m;
    const upper = numeral === numeral!.toUpperCase();
    if (quality === "h" && seventh !== "7") throw new TuneError(line, `"h" must be followed by "7" in "${text}"`, span);
    const triad =
      quality === "o" || quality === "h" ? [0, 3, 6] : quality === "+" ? [0, 4, 8] : upper ? [0, 4, 7] : [0, 3, 7];
    if (seventh === "maj7") triad.push(11);
    else if (seventh === "7") triad.push(quality === "o" ? 9 : 10);

    const rootIdx = indexOf(NUMERALS.indexOf(numeral!.toLowerCase()) + 1, marks!);
    const root = midiAt(rootIdx, accidental(acc), p);
    const notes = triad.map((i) => root + i);
    if (bassDeg) {
      const below = mod(rootIdx - (Number(bassDeg) - 1), 7) || 7;
      notes.unshift(midiAt(rootIdx - below, accidental(bassAcc), p));
    }
    return notes;
  }

  if (text.includes("+")) {
    return text.split("+").map((member) => {
      const mm = NOTE.exec(member);
      if (!mm) throw new TuneError(line, `invalid stack "${text}"`, span);
      return midiAt(indexOf(Number(mm[2]), mm[3]!), accidental(mm[1]), p);
    });
  }

  throw new TuneError(line, `unknown token "${text}"`, span);
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
  span: [number, number] | null;
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
  /** Which directives the change before this block set (SPEC §2.1). */
  directives: Set<string>;
  /** First voice line. */
  line: number;
}

const snapshot = (h: Header): Header => ({ ...h, time: [...h.time], octaves: new Map(h.octaves) });
const sameKey = (a: Header, b: Header) => a.tonicPc === b.tonicPc && a.scale.join() === b.scale.join();

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
  let pending = { directives: new Set<string>(), firstLine: 0, span: null as [number, number] | null };
  /** Every error found. Each is recorded and skipped, so one pass reports them all. */
  const errors: TuneError[] = [];
  const attempt = (fn: () => void) => {
    try {
      fn();
    } catch (e) {
      if (!(e instanceof TuneError)) throw e;
      errors.push(e);
    }
  };

  const lines = source.split(/\r?\n/);
  let lineStart = 0;
  for (let i = 0; i < lines.length; i++) {
    const lineNo = i + 1;
    const raw = lines[i]!;
    const offset = lineStart;
    lineStart += raw.length + (source[lineStart + raw.length] === "\r" ? 2 : 1);
    if (raw.trim() === "") {
      block = null;
      continue;
    }
    const text = stripComment(raw).trim();
    if (text === "") continue;
    const textStart = offset + raw.indexOf(text);
    const textSpan: [number, number] = [textStart, textStart + text.length];

    const vm = new RegExp(`^(${VOICE_NAME})\\s*:(.*)$`).exec(text);
    if (vm) {
      const name = vm[1]!;
      const nameSpan: [number, number] = [textStart, textStart + name.length];
      attempt(() => {
        if (DIRECTIVES.has(name)) throw new TuneError(lineNo, `"${name}" is a directive and cannot name a voice`, nameSpan);
        if (!block) {
          block = { voices: [], settings: snapshot(cur), directives: pending.directives, line: lineNo };
          blocks.push(block);
          pending = { directives: new Set(), firstLine: 0, span: null };
        }
        if (block.voices.some((v) => v.name === name))
          throw new TuneError(lineNo, `voice "${name}" appears twice in one block`, nameSpan);
        const colon = offset + raw.indexOf(":");
        if (vm[2]!.trim() === "") throw new TuneError(lineNo, "empty bar", [colon, colon + 1]);
        block.voices.push({ name, line: lineNo, bars: parseBars(vm[2]!, lineNo, colon + 1, (e) => errors.push(e)) });
      });
      continue;
    }

    const word = text.split(/\s+/)[0]!;
    attempt(() => {
      if (block) {
        if (DIRECTIVES.has(word))
          throw new TuneError(lineNo, `"${word}" is inside a block; changes go between blocks, after a blank line`, textSpan);
        throw new TuneError(lineNo, `expected "Name: bars…", got "${text}"`, textSpan);
      }
      if (!pending.firstLine) pending = { ...pending, firstLine: lineNo, span: textSpan };
      parseDirective(text, lineNo, cur, textSpan);
      pending.directives.add(word);
    });
  }
  if (blocks.length > 0 && pending.firstLine)
    errors.push(new TuneError(pending.firstLine, "a change must be followed by a block", pending.span));

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
      if (item.kind === "token")
        out.push({ start: s, end: add(s, each), text: item.text, editable: true, span: [item.from, item.to], ...here });
      else layout(item.items, s, each, here, out);
    });
  };

  for (const [blockIndex, b] of blocks.entries()) {
    const h = b.settings;
    const barLen = q(h.time[0] * 4 * TICKS_PER_QUARTER, h.time[1]);
    // A block whose every voice line failed to parse has no bars.
    const nBars = Math.max(0, ...b.voices.map((v) => v.bars.length));
    for (const name of voices) {
      const vl = b.voices.find((v) => v.name === name);
      const out = slots.get(name)!;
      for (let k = 0; k < nBars; k++) {
        const start = add(blockStart, mul(barLen, k));
        const bar = vl?.bars[k];
        if (bar) layout(bar, start, barLen, { line: vl.line, block: blockIndex, bar: k, path: [] }, out);
        else
          out.push({ start, end: add(start, barLen), text: ".", line: vl?.line ?? 0, block: blockIndex, bar: k, path: [], editable: false, span: null });
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
    let sounding: { start: Q; end: Q; notes: number[]; slot: number } | null = null;

    const flush = () => {
      if (!sounding) return;
      const start = ticks(sounding.start);
      const duration = ticks(sounding.end) - start;
      for (const midi of sounding.notes) events.push({ start, duration, voice: name, midi, slot: sounding.slot });
      sounding = null;
    };

    for (const slot of slots.get(name)!) {
      let notes: number[] = [];
      if (slot.text === "-") {
        if (sounding) sounding.end = slot.end;
      } else if (slot.text === ".") {
        flush();
      } else {
        const h = blocks[slot.block]!.settings;
        const tonicMidi = 12 * ((h.octaves.get(name) ?? defaultOctave(name)) + 1) + h.tonicPc;
        attempt(() => (notes = sound(slot.text, slot.line, { tonicMidi, scale: h.scale }, slot.span)));
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
        span: slot.span,
        kind: kindOf(slot.text),
        start: ticks(slot.start),
        end: ticks(slot.end),
        midi: notes,
      });
    }
    flush();
  }

  if (errors.length > 0) {
    errors.sort((a, b) => a.line - b.line || (a.span?.[0] ?? 0) - (b.span?.[0] ?? 0));
    errors[0]!.errors = errors;
    throw errors[0];
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
