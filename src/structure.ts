// Structural edits: bars, blocks, voices, and the directives that set them.
// Each takes the source and the song compiled from it and returns the new
// source, or throws — a TuneError, or fig's refusal — when it cannot be done.
//
// Bars, voice names, and directives go through fig, whose tune language edits
// a voice's bars item by item. Blocks, new voice lines, and the order of
// voices are edited as lines: fig refuses a block edit that changes which
// directives a change holds (deleting the first block hands the next one's
// change to the header), has no spelling for a new voice line, and traps
// reordering the voices of a block that has a change. After a block moves or
// goes, `keepSettings` writes the directives that make every other block
// sound as it did.

import { compile, defaultOctave } from "./compile";
import type { Path } from "./edit";
import { applyEdits } from "./fig";
import type { BlockInfo, Song } from "./types";

const VOICE_NAME = /^[A-Za-z][A-Za-z0-9_]*$/;
const DIRECTIVE_WORDS = ["key", "tempo", "time", "octave"];

/** Compiles `source`, so an edit that breaks the tune is refused. */
function checked(source: string): string {
  compile(source);
  return source;
}

/** Where a block's settings sit in fig's tree: the header's at the root, a change's in its block. */
const settingsPath = (b: number): Path => (b === 0 ? [] : ["blocks", b]);
const voicePath = (b: number, voice: string): Path => ["blocks", b, voice];

/** How many bars block `b` lasts. */
export const blockBars = (info: BlockInfo) => Math.max(0, ...Object.values(info.bars));

/** A new bar: a rest on every beat, ready to be filled in. */
const restBar = (time: [number, number]) => Array<string>(time[0]).fill(".").join(" ");

/** Throws unless `name` can name a voice. */
function checkName(name: string): void {
  if (!VOICE_NAME.test(name)) throw new Error(`"${name}" is not a voice name: start with a letter, then letters, digits, or _`);
  if (DIRECTIVE_WORDS.includes(name)) throw new Error(`"${name}" is a directive and cannot name a voice`);
}

// ── lines ───────────────────────────────────────────────────────────────────

/** A voice line taken apart: `head` is up to the colon, `segs` the text between barlines. */
interface VoiceLine {
  name: string;
  head: string;
  segs: string[];
  /** Whether the line ends with a barline. */
  closed: boolean;
  /** Whatever follows the last bar: spaces and a comment. */
  tail: string;
}

function readVoiceLine(line: string): VoiceLine | null {
  const cut = line.indexOf("//");
  const content = (cut === -1 ? line : line.slice(0, cut)).trimEnd();
  const m = /^(\s*([A-Za-z][A-Za-z0-9_]*)\s*:)(.*)$/.exec(content);
  if (!m || DIRECTIVE_WORDS.includes(m[2]!)) return null;
  const segs = m[3]!.split("|");
  const closed = segs.length > 1 && segs.at(-1)!.trim() === "";
  if (closed) segs.pop();
  return { name: m[2]!, head: m[1]!, segs, closed, tail: line.slice(content.length) };
}

const writeVoiceLine = (v: VoiceLine) => v.head + v.segs.join("|") + (v.closed ? "|" : "") + v.tail;

/**
 * The lines `[from, to)` of block `b`: its voice lines and the comments among
 * them, and for any block but the first, the change above it.
 */
function region(lines: string[], song: Song, b: number): [number, number] {
  const first = song.blocks[b]!.line - 1;
  let from = first;
  if (b > 0) while (from > 0 && lines[from - 1]!.trim() !== "") from--;
  let to = first;
  while (to < lines.length && lines[to]!.trim() !== "") to++;
  return [from, to];
}

/** The line holding `voice` in block `b`. */
function voiceLineAt(lines: string[], song: Song, b: number, voice: string): number {
  const [from, to] = region(lines, song, b);
  for (let i = from; i < to; i++) if (readVoiceLine(lines[i]!)?.name === voice) return i;
  throw new Error(`${voice} has no line in block ${b + 1}`);
}

// ── directives ──────────────────────────────────────────────────────────────

/**
 * Sets a directive for block `b`: in the header for the first block, in the
 * change before any other. fig edits or adds it in place; where fig cannot —
 * a header or change with nothing to add after, or no `octave` mapping to add
 * to — the directive line is written as text instead.
 */
export function setDirective(source: string, song: Song, b: number, path: Path, value: string | number): string {
  try {
    return checked(applyEdits(source, (ed) => ed.setValue([...settingsPath(b), ...path], value)));
  } catch {
    // fall through to a text insertion
  }
  const line = `${path.join(" ")} ${value}`;
  const lines = source.split("\n");
  let at: number;
  if (b === 0) {
    // After the header's last directive, before the first voice.
    const firstVoice = song.blocks[0] ? song.blocks[0].line - 1 : lines.length;
    at = 0;
    for (let i = 0; i < firstVoice; i++) {
      if (/^(key|tempo|time|octave)\b/.test(lines[i]!.replace(/\/\/.*/, "").trim())) at = i + 1;
    }
  } else {
    // Just above the block's first voice, after any change already there.
    at = song.blocks[b]!.line - 1;
  }
  lines.splice(at, 0, line);
  return checked(lines.join("\n"));
}

/** Removes a directive written for block `b` (`"tempo"`, `"octave RH"`…), so the block takes the one before it. */
export function clearDirective(source: string, song: Song, b: number, directive: string): string {
  const info = song.blocks[b]!;
  if (!info.directives.includes(directive)) return source;
  const [word, voice] = directive.split(" ");
  const path: Path = [...settingsPath(b), word!];
  // The last `octave` line of a header or change goes with the mapping that holds it.
  if (voice && info.directives.filter((d) => d.startsWith("octave ")).length > 1) path.push(voice);
  return checked(applyEdits(source, (ed) => ed.deleteKey(path)));
}

/** A voice's octave in a block. */
const octaveIn = (info: BlockInfo, voice: string) => info.octaves[voice] ?? defaultOctave(voice);

/**
 * Writes directives until each block sounds with the settings it had before
 * an edit: new block `i` was block `map[i]` of `before` (none: a new block).
 */
function keepSettings(before: Song, source: string, map: (number | undefined)[]): string {
  for (let i = 0; i < map.length; i++) {
    if (map[i] === undefined) continue;
    const want = before.blocks[map[i]!]!;
    const fix = (path: Path, value: string | number) => {
      source = setDirective(source, compile(source), i, path, value);
    };
    let have = compile(source).blocks[i]!;
    if (have.key !== want.key) fix(["key"], want.key);
    have = compile(source).blocks[i]!;
    if (have.tempo !== want.tempo) fix(["tempo"], want.tempo);
    have = compile(source).blocks[i]!;
    if (have.time.join("/") !== want.time.join("/")) fix(["time"], want.time.join("/"));
    for (const v of compile(source).blocks[i]!.voices) {
      have = compile(source).blocks[i]!;
      if (octaveIn(have, v) !== octaveIn(want, v)) fix(["octave", v], octaveIn(want, v));
    }
  }
  return checked(source);
}

// ── bars ────────────────────────────────────────────────────────────────────

// A bar edit applies to every voice of a block at once, so the voices stay
// lined up. A voice shorter than the block is padded with rests (SPEC §1.1),
// so its missing bars are only written where a later bar needs them.

/** Inserts a bar of rests at index `k` of block `b` (`k` = the block's length appends one). */
export function insertBar(source: string, song: Song, b: number, k: number): string {
  const info = song.blocks[b]!;
  const n = blockBars(info);
  const text = restBar(info.time);
  return checked(
    applyEdits(source, (ed) => {
      for (const v of info.voices) {
        const count = info.bars[v]!;
        if (count > k || (k === n && count === n)) ed.insertItemRaw(voicePath(b, v), k, text);
      }
    }),
  );
}

/** Deletes bar `k` of block `b` from every voice; a voice left with no bars loses its line, a block left empty goes. */
export function deleteBar(source: string, song: Song, b: number, k: number): string {
  const info = song.blocks[b]!;
  if (blockBars(info) <= 1) return deleteBlock(source, song, b);
  return checked(
    applyEdits(source, (ed) => {
      for (const v of info.voices) {
        const count = info.bars[v]!;
        if (count <= k) continue;
        if (count === 1) ed.deleteKey(voicePath(b, v));
        else ed.deleteItem(voicePath(b, v), k);
      }
    }),
  );
}

/** Copies bar `k` of block `b` into a new bar just after it. */
export function duplicateBar(source: string, song: Song, b: number, k: number): string {
  const info = song.blocks[b]!;
  const lines = source.split("\n");
  return checked(
    applyEdits(source, (ed) => {
      for (const v of info.voices) {
        if (info.bars[v]! <= k) continue;
        const bar = readVoiceLine(lines[voiceLineAt(lines, song, b, v)]!)!.segs[k]!.trim();
        ed.insertItemRaw(voicePath(b, v), k + 1, bar);
      }
    }),
  );
}

/** Swaps bar `k` of block `b` with its neighbour (`dir` −1 before, 1 after). */
export function moveBar(source: string, song: Song, b: number, k: number, dir: 1 | -1): string {
  const info = song.blocks[b]!;
  const j = k + dir;
  if (j < 0 || j >= blockBars(info)) return source;
  const [lo, hi] = [Math.min(k, j), Math.max(k, j)];
  return checked(
    applyEdits(source, (ed) => {
      for (const v of info.voices) {
        const count = info.bars[v]!;
        if (count > hi) ed.moveItem(voicePath(b, v), k, j);
        // Bar `hi` is padding: write the rest it becomes in front of bar `lo`.
        else if (count === hi) ed.insertItemRaw(voicePath(b, v), lo, ".");
      }
    }),
  );
}

// ── blocks ──────────────────────────────────────────────────────────────────

/**
 * Adds a block of one bar of rests for the same voices after block `after`,
 * or, in a tune with no blocks, a first block for `RH`.
 */
export function insertBlock(source: string, song: Song, after: number): string {
  const lines = source.split("\n");
  if (song.blocks.length === 0) {
    const body = source.replace(/\s+$/, "");
    return checked(`${body ? `${body}\n\n` : ""}RH: ${restBar(song.time)} |\n`);
  }
  const info = song.blocks[after]!;
  const [, to] = region(lines, song, after);
  lines.splice(to, 0, "", ...info.voices.map((v) => `${v}: ${restBar(info.time)} |`));
  return checked(lines.join("\n"));
}

/** Deletes block `b`; the blocks after it keep their settings. */
export function deleteBlock(source: string, song: Song, b: number): string {
  const lines = source.split("\n");
  let [from, to] = region(lines, song, b);
  // Take the blank lines that separated it, before it (or, for the first block, after it).
  if (b > 0) while (from > 0 && lines[from - 1]!.trim() === "") from--;
  else while (to < lines.length && lines[to]!.trim() === "") to++;
  lines.splice(from, to - from);
  const map = song.blocks.map((_, i) => i).filter((i) => i !== b);
  return keepSettings(song, lines.join("\n"), map);
}

/** Swaps block `b` with its neighbour (`dir` −1 before, 1 after); every block keeps its settings. */
export function moveBlock(source: string, song: Song, b: number, dir: 1 | -1): string {
  const [a, c] = [Math.min(b, b + dir), Math.max(b, b + dir)];
  if (a < 0 || c >= song.blocks.length) return source;
  const lines = source.split("\n");
  const [fa, ta] = region(lines, song, a);
  const [fc, tc] = region(lines, song, c);
  const next = [...lines.slice(0, fa), ...lines.slice(fc, tc), ...lines.slice(ta, fc), ...lines.slice(fa, ta), ...lines.slice(tc)];
  const map = song.blocks.map((_, i) => (i === a ? c : i === c ? a : i));
  return keepSettings(song, next.join("\n"), map);
}

/** Copies block `b`, change and all, into a new block just after it. */
export function duplicateBlock(source: string, song: Song, b: number): string {
  const lines = source.split("\n");
  const [from, to] = region(lines, song, b);
  lines.splice(to, 0, "", ...lines.slice(from, to));
  const map = [...song.blocks.map((_, i) => i)];
  map.splice(b + 1, 0, b);
  return keepSettings(song, lines.join("\n"), map);
}

/** Splits block `b` before bar `k`, into two blocks that play just as one did. */
export function splitBlock(source: string, song: Song, b: number, k: number): string {
  if (k <= 0 || k >= blockBars(song.blocks[b]!)) return source;
  const lines = source.split("\n");
  const [from, to] = region(lines, song, b);
  const first: string[] = [];
  const second: string[] = [];
  for (const line of lines.slice(from, to)) {
    const v = readVoiceLine(line);
    if (!v || v.segs.length <= k) {
      first.push(line);
      continue;
    }
    first.push(writeVoiceLine({ ...v, segs: v.segs.slice(0, k), closed: true }));
    second.push(writeVoiceLine({ ...v, segs: v.segs.slice(k), tail: "" }));
  }
  lines.splice(from, to - from, ...first, "", ...second);
  return checked(lines.join("\n"));
}

/** Joins block `b` and the one after it into one block. The second may not start with a change. */
export function joinBlocks(source: string, song: Song, b: number): string {
  const next = song.blocks[b + 1];
  if (!next) return source;
  if (next.directives.length > 0) throw new Error(`block ${b + 2} starts with a change (${next.directives.join(", ")}); clear it first`);
  const n = blockBars(song.blocks[b]!);
  const lines = source.split("\n");
  const [fa, ta] = region(lines, song, b);
  const [fb, tb] = region(lines, song, b + 1);
  const later = lines.slice(fb, tb);
  const laterVoices = new Map(later.map(readVoiceLine).filter((v) => v !== null).map((v) => [v.name, v]));

  /** Bars `v` gets in front of its bars from the second block: rests up to the first one's length. */
  const pad = (count: number) => Array<string>(n - count).fill(" . ");
  const joined = lines.slice(fa, ta).map((line) => {
    const v = readVoiceLine(line);
    const w = v && laterVoices.get(v.name);
    if (!v || !w) return line;
    laterVoices.delete(v.name);
    const segs = [...v.segs];
    if (!v.closed && !/\s$/.test(segs.at(-1)!)) segs[segs.length - 1] += " ";
    return writeVoiceLine({ ...v, segs: [...segs, ...pad(v.segs.length), ...w.segs], closed: w.closed, tail: v.tail.trim() ? v.tail : w.tail });
  });
  for (const w of laterVoices.values()) joined.push(writeVoiceLine({ ...w, segs: [...pad(0), ...w.segs] }));
  // The second block's comments follow the joined lines.
  joined.push(...later.filter((line) => readVoiceLine(line) === null));
  lines.splice(fa, tb - fa, ...joined);
  return checked(lines.join("\n"));
}

// ── voices ──────────────────────────────────────────────────────────────────

/** Adds a line for a new voice to block `b`, resting for the whole block. */
export function addVoice(source: string, song: Song, b: number, name: string): string {
  checkName(name);
  if (song.blocks.length === 0) {
    const body = source.replace(/\s+$/, "");
    return checked(`${body ? `${body}\n\n` : ""}${name}: ${restBar(song.time)} |\n`);
  }
  const info = song.blocks[b]!;
  if (info.voices.includes(name)) throw new Error(`block ${b + 1} already has ${name}`);
  const lines = source.split("\n");
  const [, to] = region(lines, song, b);
  lines.splice(to, 0, `${name}: ${Array<string>(Math.max(1, blockBars(info))).fill(".").join(" | ")} |`);
  return checked(lines.join("\n"));
}

/** Deletes `voice`'s line from block `b`, or from every block; a block left with no voice goes. */
export function deleteVoice(source: string, song: Song, voice: string, b: number | null): string {
  const blocks = b === null ? song.blocks.map((_, i) => i) : [b];
  for (const i of blocks.reverse()) {
    const now = compile(source);
    const info = now.blocks[i]!;
    if (!info.voices.includes(voice)) continue;
    source = info.voices.length === 1 ? deleteBlock(source, now, i) : applyEdits(source, (ed) => ed.deleteKey(voicePath(i, voice)));
  }
  return checked(source);
}

/** Renames a voice everywhere: its lines and its `octave` directives. */
export function renameVoice(source: string, song: Song, from: string, to: string): string {
  if (from === to) return source;
  checkName(to);
  if (song.voices.includes(to)) throw new Error(`there is already a voice called ${to}`);
  return checked(
    applyEdits(source, (ed) => {
      song.blocks.forEach((info, i) => {
        if (info.voices.includes(from)) ed.renameKey(voicePath(i, from), to);
        if (info.directives.includes(`octave ${from}`)) ed.renameKey([...settingsPath(i), "octave", from], to);
      });
    }),
  );
}

/**
 * Moves `voice` one place earlier or later among the voices, swapping it with
 * its neighbour in every block that has both. Voices are ordered by where they
 * first appear, so that is the order changed.
 */
export function moveVoice(source: string, song: Song, voice: string, dir: 1 | -1): string {
  const other = song.voices[song.voices.indexOf(voice) + dir];
  if (!other) return source;
  const lines = source.split("\n");
  song.blocks.forEach((info, b) => {
    if (!info.voices.includes(voice) || !info.voices.includes(other)) return;
    const i = voiceLineAt(lines, song, b, voice);
    const j = voiceLineAt(lines, song, b, other);
    [lines[i], lines[j]] = [lines[j]!, lines[i]!];
  });
  const next = lines.join("\n");
  const order = compile(next).voices;
  if (order.indexOf(voice) === song.voices.indexOf(voice))
    throw new Error(`${voice} and ${other} are ordered by the block each first appears in; move a block instead`);
  return next;
}
