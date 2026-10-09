// Where things are on the piano roll: lanes, the part of the song in view,
// and what lies under a point. Pure functions of a song and a size, so they
// can be tested without a canvas. Coordinates are CSS pixels from the roll's
// top-left corner.

import { TICKS_PER_QUARTER, type NoteEvent, type Song } from "../src/types";

export const KEYBOARD_W = 44;
/** The overview strip across the top: the whole song, its blocks, and the part in view. */
export const STRIP_H = 20;
/** The gap between lanes. */
export const LANE_GAP = 2;
/** The narrowest view: one quarter note across the roll. */
export const MIN_SPAN = TICKS_PER_QUARTER;

/** A stretch of the song, in ticks. */
export interface View {
  start: number;
  end: number;
}

/** A band of pitch rows: one voice's in lanes, or every voice's when combined. */
export interface Lane {
  /** The voice drawn here, or null for all of them. */
  voice: string | null;
  lo: number;
  hi: number;
  top: number;
  bottom: number;
}

export interface Layout extends View {
  lanes: Lane[];
  /** The pitch range across every lane, for the overview strip. */
  lo: number;
  hi: number;
  rowH: number;
  rollW: number;
  length: number;
}

/** A point on the roll: its tick, its pitch row, and its lane's voice (null where every voice shares one). */
export interface Point {
  tick: number;
  midi: number;
  voice: string | null;
}

/** A note edge: the handle that moves where a note starts or ends. */
export interface Edge {
  event: NoteEvent;
  side: "start" | "end";
}

// ── the view ────────────────────────────────────────────────────────────────

/** What `view` shows of a song `length` ticks long; null shows all of it. */
export const shown = (view: View | null, length: number): View => view ?? { start: 0, end: length };

/** `span` ticks from `start`, kept within the song and no narrower than a quarter note; null if that is the whole song. */
export function place(length: number, start: number, span: number): View | null {
  span = Math.min(length, Math.max(MIN_SPAN, span));
  start = Math.min(length - span, Math.max(0, start));
  return span >= length ? null : { start, end: start + span };
}

/** `start`–`end`, centred, at least a quarter note wide. */
export function fit(length: number, start: number, end: number): View | null {
  const span = Math.max(MIN_SPAN, end - start);
  return place(length, (start + end) / 2 - span / 2, span);
}

/** A view from before the song got shorter, pulled back inside it. */
export function clamp(view: View | null, length: number): View | null {
  if (!view || view.end <= length) return view;
  return place(length, length, view.end - view.start);
}

/** Zoomed by `factor` (above 1 zooms in), keeping `tick` where it is on screen. */
export function zoom(view: View | null, length: number, factor: number, tick: number): View | null {
  const v = shown(view, length);
  const span = Math.min(length, Math.max(MIN_SPAN, (v.end - v.start) / factor));
  return place(length, tick - ((tick - v.start) / (v.end - v.start)) * span, span);
}

/** Scrolled by `ticks`; the whole song does not scroll. */
export function pan(view: View | null, length: number, ticks: number): View | null {
  return view && place(length, view.start + ticks, view.end - view.start);
}

/** Scrolled, if need be, so `start`–`end` is in view, keeping the zoom. */
export function reveal(view: View | null, length: number, start: number, end: number): View | null {
  if (!view || (start >= view.start && end <= view.end)) return view;
  const span = view.end - view.start;
  return place(length, end - start > span || start < view.start ? start : end - span, span);
}

/** While playing: the page turned when the playhead runs off the right edge, or jumps out of view. */
export function follow(view: View | null, length: number, last: number, tick: number): View | null {
  if (!view) return view;
  const ranOff = last < view.end && tick >= view.end;
  const jumped = tick < last && (tick < view.start || tick >= view.end);
  return ranOff || jumped ? reveal(view, length, tick, tick + (view.end - view.start)) : view;
}

// ── layout ──────────────────────────────────────────────────────────────────

/** The pitches `midis` span, two semitones either side, widened to at least `min`. */
function range(midis: number[], min: number): { lo: number; hi: number } {
  let lo = (midis.length ? Math.min(...midis) : 60) - 2;
  let hi = (midis.length ? Math.max(...midis) : 60) + 2;
  while (hi - lo < min) (lo--, hi++);
  return { lo, hi };
}

/**
 * The roll for `song` at `width` × `height`: a lane per voice (if `lanes` and
 * there is more than one), else one shared, showing `view`.
 */
export function layout(song: Song, width: number, height: number, lanes: boolean, view: View | null): Layout {
  const bands =
    lanes && song.voices.length > 1
      ? song.voices.map((v) => ({ voice: v as string | null, ...range(song.events.filter((e) => e.voice === v).map((e) => e.midi), 12) }))
      : [{ voice: null, ...range(song.events.map((e) => e.midi), 24) }];
  // Every lane has the same row height, so a semitone looks the same size in each.
  const rows = bands.reduce((n, b) => n + b.hi - b.lo + 1, 0);
  const rowH = (height - STRIP_H - LANE_GAP * (bands.length - 1)) / rows;
  let top = STRIP_H;
  const placed = bands.map((b) => {
    const lane = { ...b, top, bottom: top + (b.hi - b.lo + 1) * rowH };
    top = lane.bottom + LANE_GAP;
    return lane;
  });
  return {
    lanes: placed,
    lo: Math.min(...placed.map((l) => l.lo)),
    hi: Math.max(...placed.map((l) => l.hi)),
    rowH,
    rollW: width - KEYBOARD_W,
    length: song.length,
    ...shown(clamp(view, song.length), song.length),
  };
}

/** The lane `voice` is drawn in. */
export const laneOf = (l: Layout, voice: string): Lane => l.lanes.find((ln) => ln.voice === voice) ?? l.lanes[0]!;

/** Where tick `t` is across the roll. */
export const xOf = (l: Layout, t: number) => KEYBOARD_W + ((t - l.start) / (l.end - l.start)) * l.rollW;

/** The top of pitch row `midi` in `lane`. */
export const yOf = (l: Layout, lane: Lane, midi: number) => lane.top + (lane.hi - midi) * l.rowH;

/** How wide `ticks` is on screen. */
export const widthOf = (l: Layout, ticks: number) => (ticks / (l.end - l.start)) * l.rollW;

// ── what is under a point ───────────────────────────────────────────────────

/** The tick at `x`, clamped to the part in view. */
export function tickAt(l: Layout, x: number): number {
  return Math.min(l.end, Math.max(l.start, l.start + ((x - KEYBOARD_W) / l.rollW) * (l.end - l.start)));
}

/** The tick, pitch, and lane at a point, or null outside the roll. */
export function pointAt(l: Layout, x: number, y: number): Point | null {
  if (x < KEYBOARD_W || y < STRIP_H) return null;
  // A point in the gap below a lane belongs to it.
  const lane = l.lanes.find((ln) => y < ln.bottom + LANE_GAP) ?? l.lanes.at(-1)!;
  const midi = Math.max(lane.lo, lane.hi - Math.floor((y - lane.top) / l.rowH));
  return { tick: l.start + ((x - KEYBOARD_W) / l.rollW) * (l.end - l.start), midi, voice: lane.voice };
}

/** The tick of the whole song at a point in the overview strip, or null outside it. */
export function stripAt(l: Layout, x: number, y: number): number | null {
  if (y >= STRIP_H) return null;
  return Math.min(l.length, Math.max(0, ((x - KEYBOARD_W) / l.rollW) * l.length));
}

/**
 * The note edge at a point. An edge inside its own note wins over a
 * neighbour's edge just outside it, so where two notes touch, each keeps its
 * own side.
 */
export function edgeAt(song: Song, l: Layout, x: number, y: number): Edge | null {
  const p = pointAt(l, x, y);
  if (!p) return null;
  const px = widthOf(l, 1);
  const margin = 4 / px;
  let outside: (Edge & { dist: number }) | null = null;
  for (const e of song.events) {
    if (e.midi !== p.midi || (p.voice !== null && e.voice !== p.voice)) continue;
    const end = e.start + e.duration;
    const grip = Math.min(6, widthOf(l, e.duration) / 3) / px;
    if (p.tick >= e.start && p.tick < e.start + grip && p.tick - e.start < end - p.tick) return { event: e, side: "start" };
    if (p.tick <= end && p.tick > end - grip) return { event: e, side: "end" };
    const before = e.start - p.tick;
    const after = p.tick - end;
    if (before > 0 && before <= margin && (!outside || before < outside.dist)) outside = { event: e, side: "start", dist: before };
    if (after > 0 && after <= margin && (!outside || after < outside.dist)) outside = { event: e, side: "end", dist: after };
  }
  return outside && { event: outside.event, side: outside.side };
}

/**
 * The slot a click means: the note under it, else the slot at that time in
 * the voice sounding nearest that pitch. Given a lane's voice, only that voice.
 */
export function slotAt(song: Song, tick: number, midi: number, voice: string | null = null): number | null {
  const hit = song.events.find(
    (e) => e.midi === midi && (voice === null || e.voice === voice) && tick >= e.start && tick < e.start + e.duration,
  );
  if (hit) return hit.slot;

  let best: { slot: number; dist: number } | null = null;
  for (const v of voice === null ? song.voices : [voice]) {
    let last: number | null = null;
    for (let i = 0; i < song.slots.length; i++) {
      const s = song.slots[i]!;
      if (s.voice !== v) continue;
      if (s.midi.length) last = s.midi[0]!;
      if (tick < s.start || tick >= s.end) continue;
      if (!s.editable) break;
      const dist = last === null ? 24 : Math.abs(last - midi);
      if (!best || dist < best.dist) best = { slot: i, dist };
      break;
    }
  }
  return best?.slot ?? null;
}
