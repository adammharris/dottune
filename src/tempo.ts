import { TICKS_PER_QUARTER, type Song } from "./types";

interface Segment {
  tick: number;
  sec: number;
  ticksPerSec: number;
}

/** Converts between ticks and seconds for a song whose tempo changes between blocks. */
export class TempoMap {
  private segments: Segment[] = [];
  /** Seconds from the start to the end of the song. */
  readonly total: number;

  constructor(song: Song) {
    const tempos = song.blocks.length ? song.blocks.map((b) => [b.start, b.tempo] as const) : [[0, song.tempo] as const];
    let sec = 0;
    for (const [i, [tick, bpm]] of tempos.entries()) {
      if (i > 0) {
        const prev = this.segments.at(-1)!;
        if (bpm * TICKS_PER_QUARTER / 60 === prev.ticksPerSec) continue;
        sec = prev.sec + (tick - prev.tick) / prev.ticksPerSec;
      }
      this.segments.push({ tick, sec, ticksPerSec: (bpm / 60) * TICKS_PER_QUARTER });
    }
    this.total = this.seconds(song.length);
  }

  seconds(tick: number): number {
    let s = this.segments[0]!;
    for (const seg of this.segments) if (seg.tick <= tick) s = seg;
    return s.sec + (tick - s.tick) / s.ticksPerSec;
  }

  ticks(sec: number): number {
    let s = this.segments[0]!;
    for (const seg of this.segments) if (seg.sec <= sec) s = seg;
    return s.tick + (sec - s.sec) * s.ticksPerSec;
  }
}
