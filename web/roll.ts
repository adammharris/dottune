import { TICKS_PER_QUARTER, type Song } from "../src/types";
import * as geo from "./geometry";
import { KEYBOARD_W, LANE_GAP, STRIP_H, type Edge, type Layout, type Point, type View } from "./geometry";

const COLORS = ["#7aa2f7", "#e0af68", "#9ece6a", "#f7768e", "#bb9af7", "#7dcfff"];
const BLACK = new Set([1, 3, 6, 8, 10]);

export function voiceColor(i: number): string {
  return COLORS[i % COLORS.length]!;
}

/**
 * Piano roll with a keyboard down the left edge and an overview strip across
 * the top. It keeps the view and draws; where things are is `geometry`'s.
 */
export class PianoRoll {
  /** Whether each voice gets its own lane, with its own pitch range, rather than sharing one. */
  lanes = true;
  private ctx: CanvasRenderingContext2D;
  /** The roll as last drawn: what screen points mean. */
  private layout: Layout | null = null;
  /** The ticks in view; null shows the whole song. */
  private view: View | null = null;
  /** The playhead as of the last `follow`. */
  private lastTick = 0;

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
  }

  /** A client point in the roll's own coordinates. */
  private local(clientX: number, clientY: number): [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    return [clientX - rect.left, clientY - rect.top];
  }

  pointAt(clientX: number, clientY: number): Point | null {
    return this.layout && geo.pointAt(this.layout, ...this.local(clientX, clientY));
  }

  /** The tick under a client x, clamped to the part in view. */
  tickAt(clientX: number): number | null {
    return this.layout && geo.tickAt(this.layout, this.local(clientX, 0)[0]);
  }

  stripAt(clientX: number, clientY: number): number | null {
    return this.layout && geo.stripAt(this.layout, ...this.local(clientX, clientY));
  }

  edgeAt(song: Song, clientX: number, clientY: number): Edge | null {
    return this.layout && geo.edgeAt(song, this.layout, ...this.local(clientX, clientY));
  }

  /** How wide `ticks` is on screen, in CSS pixels. */
  width(ticks: number): number {
    return this.layout ? geo.widthOf(this.layout, ticks) : 0;
  }

  // The view changes from the stored view, not the last frame drawn, so events between frames add up.

  /** The ticks in view. */
  shown(): View | null {
    return this.layout && geo.shown(this.view, this.layout.length);
  }

  /** Shows `start`–`end`, kept within the song and no narrower than a quarter note. */
  show(start: number, end: number): void {
    if (this.layout) this.view = geo.fit(this.layout.length, start, end);
  }

  showAll(): void {
    this.view = null;
  }

  /** Zooms by `factor` (above 1 zooms in), keeping `tick` where it is on screen. */
  zoom(factor: number, tick: number): void {
    if (this.layout) this.view = geo.zoom(this.view, this.layout.length, factor, tick);
  }

  /** Scrolls the view by `px` screen pixels. */
  pan(px: number): void {
    const l = this.layout;
    if (!l || !this.view) return;
    this.view = geo.pan(this.view, l.length, (px / l.rollW) * (this.view.end - this.view.start));
  }

  /** Scrolls, if need be, so `start`–`end` is in view, keeping the zoom. */
  reveal(start: number, end: number): void {
    if (this.layout) this.view = geo.reveal(this.view, this.layout.length, start, end);
  }

  /** While playing: turns the page when the playhead runs off the right edge, or jumps out of view. */
  follow(tick: number): void {
    const last = this.lastTick;
    this.lastTick = tick;
    if (this.layout) this.view = geo.follow(this.view, this.layout.length, last, tick);
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

    this.view = geo.clamp(this.view, song.length);
    const l = (this.layout = geo.layout(song, w, h, this.lanes, this.view));
    const { lanes, rowH, rollW, start, end } = l;
    const laneOf = (voice: string) => geo.laneOf(l, voice);
    const x = (t: number) => geo.xOf(l, t);
    const y = (lane: geo.Lane, midi: number) => geo.yOf(l, lane, midi);

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
    const active = new Map<geo.Lane, Map<number, number>>(lanes.map((ln) => [ln, new Map()]));
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
