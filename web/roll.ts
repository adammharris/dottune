import { TICKS_PER_QUARTER, type NoteEvent, type Song } from "../src/types";

const KEYBOARD_W = 44;
/** The overview strip across the top: the whole song, its blocks, and the part in view. */
const STRIP_H = 20;
/** The narrowest view: one quarter note across the roll. */
const MIN_SPAN = TICKS_PER_QUARTER;
const COLORS = ["#7aa2f7", "#e0af68", "#9ece6a", "#f7768e", "#bb9af7", "#7dcfff"];
const BLACK = new Set([1, 3, 6, 8, 10]);
/** The gap between lanes. */
const LANE_GAP = 2;

export function voiceColor(i: number): string {
  return COLORS[i % COLORS.length]!;
}

/** A band of pitch rows: one voice's in lanes, or every voice's when combined. */
interface Lane {
  /** The voice drawn here, or null for all of them. */
  voice: string | null;
  lo: number;
  hi: number;
  top: number;
  bottom: number;
}

interface Layout {
  lanes: Lane[];
  /** The pitch range across every lane, for the overview strip. */
  lo: number;
  hi: number;
  rowH: number;
  rollW: number;
  length: number;
  /** The ticks in view. */
  start: number;
  end: number;
}

/** Piano roll with a keyboard down the left edge and an overview strip across the top. */
export class PianoRoll {
  /** Whether each voice gets its own lane, with its own pitch range, rather than sharing one. */
  lanes = true;
  private ctx: CanvasRenderingContext2D;
  private layout: Layout | null = null;
  /** The ticks in view; null shows the whole song. */
  private view: { start: number; end: number } | null = null;
  /** The playhead as of the last `follow`. */
  private lastTick = 0;

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
  }

  /**
   * The tick, pitch, and lane under a client point, or null if outside the
   * roll. `voice` is the lane's voice, or null where every voice shares one.
   */
  pointAt(clientX: number, clientY: number): { tick: number; midi: number; voice: string | null } | null {
    const l = this.layout;
    if (!l) return null;
    const rect = this.canvas.getBoundingClientRect();
    const x = clientX - rect.left - KEYBOARD_W;
    const y = clientY - rect.top;
    if (x < 0 || y < STRIP_H) return null;
    // A point in the gap below a lane belongs to it.
    const lane = l.lanes.find((ln) => y < ln.bottom + LANE_GAP) ?? l.lanes.at(-1)!;
    const midi = Math.max(lane.lo, lane.hi - Math.floor((y - lane.top) / l.rowH));
    return { tick: l.start + (x / l.rollW) * (l.end - l.start), midi, voice: lane.voice };
  }

  /** The tick under a client x, clamped to the part in view. */
  tickAt(clientX: number): number | null {
    const l = this.layout;
    if (!l) return null;
    const x = clientX - this.canvas.getBoundingClientRect().left - KEYBOARD_W;
    return Math.min(l.end, Math.max(l.start, l.start + (x / l.rollW) * (l.end - l.start)));
  }

  /** The tick of the whole song under a client point in the overview strip, or null if outside it. */
  stripAt(clientX: number, clientY: number): number | null {
    const l = this.layout;
    if (!l) return null;
    const rect = this.canvas.getBoundingClientRect();
    if (clientY - rect.top >= STRIP_H) return null;
    const x = clientX - rect.left - KEYBOARD_W;
    return Math.min(l.length, Math.max(0, (x / l.rollW) * l.length));
  }

  /** How wide `ticks` is on screen, in CSS pixels. */
  width(ticks: number): number {
    const l = this.layout;
    return l ? (ticks / (l.end - l.start)) * l.rollW : 0;
  }

  /** The ticks in view now, and the song's length: the stored view, not the last frame drawn, so events between frames add up. */
  private current(): { start: number; end: number; length: number } | null {
    const l = this.layout;
    if (!l) return null;
    return { start: this.view?.start ?? 0, end: this.view?.end ?? l.length, length: l.length };
  }

  /** Sets the view to `span` ticks from `start`, kept within the song; the whole song if `span` covers it. */
  private place(start: number, span: number): void {
    const length = this.layout?.length ?? 0;
    if (length === 0) return;
    span = Math.min(length, Math.max(MIN_SPAN, span));
    start = Math.min(length - span, Math.max(0, start));
    this.view = span >= length ? null : { start, end: start + span };
  }

  /** The ticks in view. */
  shown(): { start: number; end: number } | null {
    const v = this.current();
    return v && { start: v.start, end: v.end };
  }

  /** Shows `start`–`end`, kept within the song and no narrower than a quarter note. */
  show(start: number, end: number): void {
    const span = Math.max(MIN_SPAN, end - start);
    this.place((start + end) / 2 - span / 2, span);
  }

  /** Shows the whole song. */
  showAll(): void {
    this.view = null;
  }

  /** Zooms by `factor` (above 1 zooms in), keeping `tick` where it is on screen. */
  zoom(factor: number, tick: number): void {
    const v = this.current();
    if (!v) return;
    const span = Math.min(v.length, Math.max(MIN_SPAN, (v.end - v.start) / factor));
    this.place(tick - ((tick - v.start) / (v.end - v.start)) * span, span);
  }

  /** Scrolls the view by `px` screen pixels. */
  pan(px: number): void {
    const v = this.current();
    if (!v || !this.view) return;
    const span = v.end - v.start;
    this.place(v.start + (px / this.layout!.rollW) * span, span);
  }

  /** Scrolls, if need be, so `start`–`end` is in view, keeping the zoom. */
  reveal(start: number, end: number): void {
    const v = this.current();
    if (!v || !this.view) return;
    const span = v.end - v.start;
    if (start >= v.start && end <= v.end) return;
    this.place(end - start > span || start < v.start ? start : end - span, span);
  }

  /** While playing: turns the page when the playhead runs off the right edge, or jumps out of view. */
  follow(tick: number): void {
    const v = this.current();
    const last = this.lastTick;
    this.lastTick = tick;
    if (!v || !this.view) return;
    const ranOff = last < v.end && tick >= v.end;
    const jumped = tick < last && (tick < v.start || tick >= v.end);
    if (ranOff || jumped) this.reveal(tick, tick + (v.end - v.start));
  }

  /**
   * The note edge under a client point: the handle that moves where a note
   * starts or ends. An edge inside its own note wins over a neighbour's edge
   * just outside it, so where two notes touch, each keeps its own side.
   */
  edgeAt(song: Song, clientX: number, clientY: number): { event: NoteEvent; side: "start" | "end" } | null {
    const p = this.pointAt(clientX, clientY);
    if (!p) return null;
    const px = this.width(1);
    let outside: { event: NoteEvent; side: "start" | "end"; dist: number } | null = null;
    for (const e of song.events) {
      if (e.midi !== p.midi || (p.voice !== null && e.voice !== p.voice)) continue;
      const end = e.start + e.duration;
      const grip = Math.min(6, this.width(e.duration) / 3) / px;
      const margin = 4 / px;
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
   * The slot a click at this point means: the note under it, else the slot
   * at that time in the voice sounding nearest that pitch. Given a lane's
   * voice, only that voice.
   */
  slotAt(song: Song, tick: number, midi: number, voice: string | null = null): number | null {
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

  /** The lanes for `song` in a roll `height` tall: their pitch ranges and where they sit. */
  private laneLayout(song: Song, height: number): { lanes: Lane[]; rowH: number } {
    const range = (midis: number[], min: number) => {
      let lo = (midis.length ? Math.min(...midis) : 60) - 2;
      let hi = (midis.length ? Math.max(...midis) : 60) + 2;
      while (hi - lo < min) (lo--, hi++);
      return { lo, hi };
    };
    const bands =
      this.lanes && song.voices.length > 1
        ? song.voices.map((v) => ({ voice: v as string | null, ...range(song.events.filter((e) => e.voice === v).map((e) => e.midi), 12) }))
        : [{ voice: null, ...range(song.events.map((e) => e.midi), 24) }];
    // Every lane has the same row height, so a semitone looks the same size in each.
    const rows = bands.reduce((n, b) => n + b.hi - b.lo + 1, 0);
    const rowH = (height - STRIP_H - LANE_GAP * (bands.length - 1)) / rows;
    let top = STRIP_H;
    const lanes = bands.map((b) => {
      const lane = { ...b, top, bottom: top + (b.hi - b.lo + 1) * rowH };
      top = lane.bottom + LANE_GAP;
      return lane;
    });
    return { lanes, rowH };
  }

  draw(song: Song | null, tick: number, stale: boolean, selected: number | null): void {
    const { canvas, ctx } = this;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr;
      canvas.height = h * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#16161e";
    ctx.fillRect(0, 0, w, h);
    this.layout = null;
    if (!song || song.length === 0) return;

    const { lanes, rowH } = this.laneLayout(song, h);
    const lo = Math.min(...lanes.map((ln) => ln.lo));
    const hi = Math.max(...lanes.map((ln) => ln.hi));
    const rollW = w - KEYBOARD_W;
    const laneOf = (voice: string) => lanes.find((ln) => ln.voice === voice) ?? lanes[0]!;
    // A view from before the song got shorter is pulled back inside it.
    if (this.view && this.view.end > song.length) {
      const span = this.view.end - this.view.start;
      this.view = span >= song.length ? null : { start: song.length - span, end: song.length };
    }
    const start = this.view?.start ?? 0;
    const end = this.view?.end ?? song.length;
    this.layout = { lanes, lo, hi, rowH, rollW, length: song.length, start, end };
    const x = (t: number) => KEYBOARD_W + ((t - start) / (end - start)) * rollW;
    const y = (lane: Lane, midi: number) => lane.top + (lane.hi - midi) * rowH;

    this.drawStrip(song, tick, rollW);

    ctx.save();
    ctx.beginPath();
    ctx.rect(KEYBOARD_W, STRIP_H, rollW, h - STRIP_H);
    ctx.clip();

    // Row shading for black keys, and the gaps between lanes.
    for (const lane of lanes) {
      for (let m = lane.lo; m <= lane.hi; m++) {
        if (BLACK.has(m % 12)) {
          ctx.fillStyle = "#1a1b26";
          ctx.fillRect(KEYBOARD_W, y(lane, m), rollW, rowH);
        }
      }
    }

    // Beat and bar lines, in each block's own time signature; when zoomed in, the beats' halves too.
    for (const b of song.blocks) {
      if (b.end < start || b.start > end) continue;
      const beat = (TICKS_PER_QUARTER * 4) / b.time[1];
      const bar = beat * b.time[0];
      const step = this.width(beat) >= 96 ? beat / 2 : beat;
      const from = b.start + Math.max(0, Math.floor((start - b.start) / step)) * step;
      for (let t = from; t < Math.min(b.end, end + step); t += step) {
        const rel = t - b.start;
        ctx.fillStyle = rel % bar === 0 ? "#3b3d57" : rel % beat === 0 ? "#22232f" : "#1c1d28";
        ctx.fillRect(Math.round(x(t)), STRIP_H, 1, h - STRIP_H);
      }
    }

    for (const lane of lanes.slice(1)) {
      ctx.fillStyle = "#292e42";
      ctx.fillRect(KEYBOARD_W, lane.top - LANE_GAP, rollW, LANE_GAP);
    }

    // The selected slot's span, behind the notes: in its own lane, or across them all.
    const sel = selected !== null ? song.slots[selected] : undefined;
    if (sel) {
      const lane = laneOf(sel.voice);
      ctx.fillStyle = voiceColor(song.voices.indexOf(sel.voice));
      ctx.globalAlpha = 0.14;
      ctx.fillRect(x(sel.start), lane.top, x(sel.end) - x(sel.start), lane.bottom - lane.top);
      ctx.globalAlpha = 1;
    }

    // Notes.
    /** Sounding keys, by lane, with the voice sounding each. */
    const active = new Map<Lane, Map<number, number>>(lanes.map((ln) => [ln, new Map()]));
    for (const e of song.events) {
      const vi = song.voices.indexOf(e.voice);
      const lane = laneOf(e.voice);
      const on = tick >= e.start && tick < e.start + e.duration;
      if (on) active.get(lane)!.set(e.midi, vi);
      if (e.start + e.duration < start || e.start > end) continue;
      ctx.fillStyle = voiceColor(vi);
      ctx.globalAlpha = (stale ? 0.45 : 1) * (on || e.slot === selected ? 1 : 0.7);
      const nx = x(e.start);
      const nw = Math.max(2, x(e.start + e.duration) - nx - 1);
      roundRect(ctx, nx, y(lane, e.midi) + 1, nw, Math.max(2, rowH - 2), Math.min(3, rowH / 3));
      if (e.slot === selected) {
        ctx.globalAlpha = 1;
        ctx.strokeStyle = "#ff9e64";
        ctx.lineWidth = 2.5;
        ctx.stroke();
      } else if (on) {
        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    }
    ctx.globalAlpha = 1;

    // Each lane's voice, in its corner.
    if (lanes.length > 1) {
      ctx.font = "11px ui-monospace, monospace";
      ctx.textBaseline = "bottom";
      for (const lane of lanes) {
        const name = lane.voice!;
        const lw = ctx.measureText(name).width + 8;
        ctx.fillStyle = "#16161ecc";
        ctx.fillRect(KEYBOARD_W + 2, lane.bottom - 17, lw, 15);
        ctx.fillStyle = voiceColor(song.voices.indexOf(name));
        ctx.fillText(name, KEYBOARD_W + 6, lane.bottom - 3);
      }
    }

    // Changes between blocks: a marker and what changed.
    ctx.font = "11px ui-monospace, monospace";
    ctx.textBaseline = "top";
    for (const c of song.changes) {
      const label = [c.key, c.tempo !== undefined ? `♩=${c.tempo}` : "", c.time?.join("/") ?? ""].filter(Boolean).join(" · ");
      const cx = Math.round(x(c.tick));
      ctx.fillStyle = "#bb9af7";
      ctx.fillRect(cx, STRIP_H, 1.5, h - STRIP_H);
      const lw = ctx.measureText(label).width + 8;
      ctx.fillStyle = "#16161ecc";
      ctx.fillRect(cx + 2, STRIP_H + 2, lw, 15);
      ctx.fillStyle = "#bb9af7";
      ctx.fillText(label, cx + 6, STRIP_H + 4);
    }

    // Playhead.
    ctx.fillStyle = "#ff9e64";
    ctx.fillRect(Math.round(x(tick)), STRIP_H, 2, h - STRIP_H);
    ctx.restore();

    // A keyboard for each lane.
    for (const lane of lanes) {
      for (let m = lane.lo; m <= lane.hi; m++) {
        const lit = active.get(lane)!.get(m);
        ctx.fillStyle = lit !== undefined ? voiceColor(lit) : BLACK.has(m % 12) ? "#2a2b3a" : "#c0caf5";
        ctx.fillRect(0, y(lane, m), KEYBOARD_W - 4, rowH - 0.5);
        if (m % 12 === 0 && rowH >= 8) {
          ctx.fillStyle = "#16161e";
          ctx.font = `${Math.min(10, rowH - 1)}px ui-monospace, monospace`;
          ctx.textBaseline = "middle";
          ctx.fillText(`C${m / 12 - 1}`, 3, y(lane, m) + rowH / 2);
        }
      }
    }
  }

  /** The whole song in miniature, its blocks numbered, and the part in view outlined. */
  private drawStrip(song: Song, tick: number, rollW: number): void {
    const { ctx } = this;
    const l = this.layout!;
    const sx = (t: number) => KEYBOARD_W + (t / song.length) * rollW;
    ctx.fillStyle = "#1a1b26";
    ctx.fillRect(0, 0, KEYBOARD_W + rollW, STRIP_H);

    ctx.font = "10px ui-monospace, monospace";
    ctx.textBaseline = "middle";
    song.blocks.forEach((b, i) => {
      const bx = sx(b.start);
      ctx.fillStyle = i % 2 ? "#1f2030" : "#24263a";
      ctx.fillRect(bx, 0, sx(b.end) - bx, STRIP_H);
      if (song.blocks.length > 1 && sx(b.end) - bx > 14) {
        ctx.fillStyle = "#565f89";
        ctx.fillText(String(i + 1), bx + 4, STRIP_H / 2);
      }
    });

    // Notes, by pitch, one pixel tall.
    const span = Math.max(1, l.hi - l.lo);
    for (const e of song.events) {
      ctx.fillStyle = voiceColor(song.voices.indexOf(e.voice));
      ctx.globalAlpha = 0.6;
      const ny = 2 + ((l.hi - e.midi) / span) * (STRIP_H - 5);
      ctx.fillRect(sx(e.start), ny, Math.max(1, sx(e.start + e.duration) - sx(e.start) - 0.5), 1.5);
    }
    ctx.globalAlpha = 1;

    if (this.view) {
      ctx.strokeStyle = "#c0caf5";
      ctx.lineWidth = 1;
      ctx.strokeRect(Math.round(sx(l.start)) + 0.5, 0.5, Math.max(3, sx(l.end) - sx(l.start) - 1), STRIP_H - 1);
    }
    ctx.fillStyle = "#ff9e64";
    ctx.fillRect(Math.round(sx(tick)), 0, 1, STRIP_H);
    ctx.fillStyle = "#292e42";
    ctx.fillRect(0, STRIP_H - 1, KEYBOARD_W + rollW, 1);
  }
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
  ctx.fill();
}
