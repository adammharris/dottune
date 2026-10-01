import { TICKS_PER_QUARTER, type Song } from "../src/types";

const KEYBOARD_W = 44;
const COLORS = ["#7aa2f7", "#e0af68", "#9ece6a", "#f7768e", "#bb9af7", "#7dcfff"];
const BLACK = new Set([1, 3, 6, 8, 10]);

export function voiceColor(i: number): string {
  return COLORS[i % COLORS.length]!;
}

interface Layout {
  lo: number;
  hi: number;
  rowH: number;
  rollW: number;
  length: number;
}

/** Piano roll with a keyboard down the left edge. */
export class PianoRoll {
  private ctx: CanvasRenderingContext2D;
  private layout: Layout | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
  }

  /** The tick and pitch under a client point, or null if outside the roll. */
  pointAt(clientX: number, clientY: number): { tick: number; midi: number } | null {
    const l = this.layout;
    if (!l) return null;
    const rect = this.canvas.getBoundingClientRect();
    const x = clientX - rect.left - KEYBOARD_W;
    if (x < 0) return null;
    const y = clientY - rect.top;
    return { tick: (x / l.rollW) * l.length, midi: l.hi - Math.floor(y / l.rowH) };
  }

  /**
   * The slot a click at this point means: the note under it, else the slot
   * at that time in the voice sounding nearest that pitch.
   */
  slotAt(song: Song, tick: number, midi: number): number | null {
    const hit = song.events.find((e) => e.midi === midi && tick >= e.start && tick < e.start + e.duration);
    if (hit) return hit.slot;

    let best: { slot: number; dist: number } | null = null;
    for (const voice of song.voices) {
      let last: number | null = null;
      for (let i = 0; i < song.slots.length; i++) {
        const s = song.slots[i]!;
        if (s.voice !== voice) continue;
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

    const pitches = song.events.map((e) => e.midi);
    let lo = (pitches.length ? Math.min(...pitches) : 60) - 2;
    let hi = (pitches.length ? Math.max(...pitches) : 60) + 2;
    while (hi - lo < 24) (lo--, hi++);
    const rows = hi - lo + 1;
    const rowH = h / rows;
    const rollW = w - KEYBOARD_W;
    this.layout = { lo, hi, rowH, rollW, length: song.length };
    const x = (t: number) => KEYBOARD_W + (t / song.length) * rollW;
    const y = (midi: number) => (hi - midi) * rowH;

    // Row shading for black keys.
    for (let m = lo; m <= hi; m++) {
      if (BLACK.has(m % 12)) {
        ctx.fillStyle = "#1a1b26";
        ctx.fillRect(KEYBOARD_W, y(m), rollW, rowH);
      }
    }

    // Beat and bar lines, in each block's own time signature.
    for (const b of song.blocks) {
      const beat = (TICKS_PER_QUARTER * 4) / b.time[1];
      const bar = beat * b.time[0];
      for (let t = b.start; t < b.end; t += beat) {
        ctx.fillStyle = (t - b.start) % bar === 0 ? "#3b3d57" : "#22232f";
        ctx.fillRect(Math.round(x(t)), 0, 1, h);
      }
    }

    // The selected slot's span, behind the notes.
    const sel = selected !== null ? song.slots[selected] : undefined;
    if (sel) {
      ctx.fillStyle = voiceColor(song.voices.indexOf(sel.voice));
      ctx.globalAlpha = 0.14;
      ctx.fillRect(x(sel.start), 0, x(sel.end) - x(sel.start), h);
      ctx.globalAlpha = 1;
    }

    // Notes.
    const active = new Map<number, number>();
    for (const e of song.events) {
      const vi = song.voices.indexOf(e.voice);
      const on = tick >= e.start && tick < e.start + e.duration;
      if (on) active.set(e.midi, vi);
      ctx.fillStyle = voiceColor(vi);
      ctx.globalAlpha = (stale ? 0.45 : 1) * (on || e.slot === selected ? 1 : 0.7);
      const nx = x(e.start);
      const nw = Math.max(2, x(e.start + e.duration) - nx - 1);
      roundRect(ctx, nx, y(e.midi) + 1, nw, Math.max(2, rowH - 2), Math.min(3, rowH / 3));
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

    // Keyboard.
    for (let m = lo; m <= hi; m++) {
      const lit = active.get(m);
      ctx.fillStyle = lit !== undefined ? voiceColor(lit) : BLACK.has(m % 12) ? "#2a2b3a" : "#c0caf5";
      ctx.fillRect(0, y(m), KEYBOARD_W - 4, rowH - 0.5);
      if (m % 12 === 0 && rowH >= 8) {
        ctx.fillStyle = "#16161e";
        ctx.font = `${Math.min(10, rowH - 1)}px ui-monospace, monospace`;
        ctx.textBaseline = "middle";
        ctx.fillText(`C${m / 12 - 1}`, 3, y(m) + rowH / 2);
      }
    }

    // Changes between blocks: a marker and what changed.
    ctx.font = "11px ui-monospace, monospace";
    ctx.textBaseline = "top";
    for (const c of song.changes) {
      const label = [c.key, c.tempo !== undefined ? `♩=${c.tempo}` : "", c.time?.join("/") ?? ""].filter(Boolean).join(" · ");
      const cx = Math.round(x(c.tick));
      ctx.fillStyle = "#bb9af7";
      ctx.fillRect(cx, 0, 1.5, h);
      const w = ctx.measureText(label).width + 8;
      ctx.fillStyle = "#16161ecc";
      ctx.fillRect(cx + 2, 2, w, 15);
      ctx.fillStyle = "#bb9af7";
      ctx.fillText(label, cx + 6, 4);
    }

    // Playhead.
    ctx.fillStyle = "#ff9e64";
    ctx.fillRect(Math.round(x(tick)), 0, 2, h);
  }
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
  ctx.fill();
}
