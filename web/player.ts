import { TempoMap } from "../src/tempo";
import type { Song } from "../src/types";

const LOOKAHEAD_SEC = 0.12;
const INTERVAL_MS = 25;

interface Timed {
  start: number;
  duration: number;
  midi: number;
  voice: number;
}

/**
 * Schedules a Song on the Web Audio clock. Position is tracked in "unwrapped"
 * song seconds — laps × song length + seconds into the song — so looping is
 * modular arithmetic, tempo changes are the tempo map's business, and the song
 * can be swapped while playing (live editing) without restarting.
 */
export class Player {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private previewOut: GainNode | null = null;
  private song: Song | null = null;
  private map: TempoMap | null = null;
  private timed: Timed[] = [];
  private timer: ReturnType<typeof setInterval> | null = null;

  /** Audio-clock time at which unwrapped second `anchorSec` plays. */
  private anchorTime = 0;
  private anchorSec = 0;
  /** Everything before this unwrapped second has been scheduled. */
  private cursor = 0;
  /** Where playback resumes, in ticks. */
  private pausedAt = 0;

  loop = true;
  onStop: (() => void) | null = null;

  get playing(): boolean {
    return this.timer !== null;
  }

  private get total(): number {
    return Math.max(this.map?.total ?? 0, 0.001);
  }

  /** Current position within the song, in ticks. */
  position(): number {
    if (!this.playing || !this.ctx || !this.map) return this.pausedAt;
    const u = this.anchorSec + (this.ctx.currentTime - this.anchorTime);
    return this.map.ticks(this.loop ? u % this.total : Math.min(u, this.total));
  }

  setSong(song: Song): void {
    const pos = this.position();
    const map = new TempoMap(song);
    this.timed = song.events.map((e) => {
      const start = map.seconds(e.start);
      return { start, duration: map.seconds(e.start + e.duration) - start, midi: e.midi, voice: song.voices.indexOf(e.voice) };
    });
    const playing = this.playing && this.ctx;
    // Seconds already scheduled past now, under the old map: keep them.
    const ahead = playing ? this.cursor - (this.anchorSec + this.ctx!.currentTime - this.anchorTime) : 0;
    this.song = song;
    this.map = map;
    const tick = pos >= song.length ? 0 : pos;
    if (!playing) {
      this.pausedAt = tick;
      return;
    }
    this.anchorTime = this.ctx!.currentTime;
    this.anchorSec = map.seconds(tick);
    this.cursor = this.anchorSec + Math.max(0, ahead);
  }

  async play(): Promise<void> {
    if (this.playing || !this.song || !this.map) return;
    this.ctx ??= new AudioContext();
    await this.ctx.resume();

    const comp = this.ctx.createDynamicsCompressor();
    comp.connect(this.ctx.destination);
    this.master = this.ctx.createGain();
    this.master.gain.value = 0.5;
    this.master.connect(comp);

    this.anchorTime = this.ctx.currentTime + 0.05;
    this.anchorSec = this.map.seconds(this.pausedAt);
    this.cursor = this.anchorSec;
    this.timer = setInterval(() => this.schedule(), INTERVAL_MS);
    this.schedule();
  }

  stop(): void {
    if (!this.playing) return;
    this.pausedAt = this.position();
    clearInterval(this.timer!);
    this.timer = null;
    const master = this.master!;
    master.gain.setTargetAtTime(0, this.ctx!.currentTime, 0.02);
    setTimeout(() => master.disconnect(), 200);
    this.onStop?.();
  }

  seek(tick: number): void {
    const wasPlaying = this.playing;
    if (wasPlaying) this.stop();
    this.pausedAt = Math.max(0, Math.min(tick, (this.song?.length ?? 1) - 1));
    if (wasPlaying) void this.play();
  }

  private schedule(): void {
    const ctx = this.ctx!;
    const total = this.total;
    const end = this.anchorSec + (ctx.currentTime + LOOKAHEAD_SEC - this.anchorTime);

    if (!this.loop && this.cursor >= total) {
      if (this.anchorSec + (ctx.currentTime - this.anchorTime) >= total) {
        this.stop();
        this.pausedAt = 0;
      }
      return;
    }
    const limit = this.loop ? end : Math.min(end, total);

    for (let lap = Math.floor(this.cursor / total); lap * total < limit; lap++) {
      const offset = lap * total;
      for (const e of this.timed) {
        const u = e.start + offset;
        if (u < this.cursor || u >= limit) continue;
        this.note(this.master!, e.midi, this.anchorTime + (u - this.anchorSec), e.duration, e.voice);
      }
    }
    this.cursor = Math.max(this.cursor, limit);
  }

  /** Sounds notes right away, outside playback — feedback for an edit. */
  async preview(midis: number[], voiceIndex: number, seconds = 0.4): Promise<void> {
    this.ctx ??= new AudioContext();
    await this.ctx.resume();
    if (!this.previewOut) {
      this.previewOut = this.ctx.createGain();
      this.previewOut.gain.value = 0.4;
      this.previewOut.connect(this.ctx.destination);
    }
    for (const midi of midis) this.note(this.previewOut, midi, this.ctx.currentTime, seconds, voiceIndex);
  }

  /** A soft electric-piano-ish voice: triangle + octave sine through a closing lowpass. */
  private note(out: AudioNode, midi: number, when: number, dur: number, voiceIndex: number): void {
    const ctx = this.ctx!;
    const t = Math.max(when, ctx.currentTime);
    const freq = 440 * 2 ** ((midi - 69) / 12);
    const release = 0.25;

    const env = ctx.createGain();
    env.gain.setValueAtTime(0, t);
    env.gain.linearRampToValueAtTime(voiceIndex === 0 ? 0.22 : 0.15, t + 0.006);
    env.gain.setTargetAtTime(0.05, t + 0.006, 0.6);
    env.gain.setTargetAtTime(0, t + dur, release / 4);

    const filter = ctx.createBiquadFilter();
    filter.type = "lowpass";
    filter.frequency.setValueAtTime(Math.min(freq * 8, 9000), t);
    filter.frequency.setTargetAtTime(Math.min(freq * 3, 4000), t, 0.3);
    filter.connect(env);
    env.connect(out);

    const partials: [OscillatorType, number, number][] = [
      ["triangle", 1, 1],
      ["sine", 2, 0.25],
      ["sine", 1.003, 0.4],
    ];
    for (const [type, ratio, level] of partials) {
      const osc = ctx.createOscillator();
      osc.type = type;
      osc.frequency.value = freq * ratio;
      const g = ctx.createGain();
      g.gain.value = level;
      osc.connect(g).connect(filter);
      osc.start(t);
      osc.stop(t + dur + release);
    }
  }
}
