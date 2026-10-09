import { expect, test } from "bun:test";
import { compile } from "../src/compile";
import { TICKS_PER_QUARTER } from "../src/types";
import * as geo from "../web/geometry";
import { KEYBOARD_W, LANE_GAP, MIN_SPAN, STRIP_H } from "../web/geometry";

const BAR = TICKS_PER_QUARTER * 4;
const W = KEYBOARD_W + 800;
const H = 600;

/** The x of tick `t` and the y of the middle of row `midi` in `voice`'s lane. */
function at(l: geo.Layout, t: number, midi: number, voice: string | null = null): [number, number] {
  const lane = voice === null ? l.lanes[0]! : geo.laneOf(l, voice);
  return [geo.xOf(l, t), geo.yOf(l, lane, midi) + l.rowH / 2];
}

// ── lanes ───────────────────────────────────────────────────────────────────

test("each voice gets a lane, in order, all with one row height", () => {
  const song = compile("RH: 1' 5' |\nLH: 1, |\n");
  const l = geo.layout(song, W, H, true, null);
  expect(l.lanes.map((ln) => ln.voice)).toEqual(["RH", "LH"]);
  const [rh, lh] = l.lanes;
  expect(rh!.top).toBe(STRIP_H);
  expect(lh!.top).toBeCloseTo(rh!.bottom + LANE_GAP);
  expect(lh!.bottom).toBeCloseTo(H);
  // Rows are the same size in both, so the lanes' heights follow their ranges.
  for (const ln of l.lanes) expect((ln.bottom - ln.top) / (ln.hi - ln.lo + 1)).toBeCloseTo(l.rowH);
  // Each lane spans its notes plus two either side, widened to at least an octave.
  expect([rh!.lo, rh!.hi]).toEqual([69, 82]);
  expect([lh!.lo, lh!.hi]).toEqual([30, 42]);
});

test("one voice, or lanes off, shares one lane at least two octaves tall", () => {
  const song = compile("RH: 1 |\nLH: 1 |\n");
  for (const l of [geo.layout(compile("RH: 1 2 |\n"), W, H, true, null), geo.layout(song, W, H, false, null)]) {
    expect(l.lanes).toHaveLength(1);
    expect(l.lanes[0]!.voice).toBeNull();
    expect(l.lanes[0]!.hi - l.lanes[0]!.lo).toBeGreaterThanOrEqual(24);
  }
});

test("a point belongs to the lane it is in, gap included, and to nothing above the roll", () => {
  const song = compile("RH: 1' |\nLH: 1, |\n");
  const l = geo.layout(song, W, H, true, null);
  const [rh, lh] = l.lanes;
  expect(geo.pointAt(l, KEYBOARD_W + 10, rh!.top + 1)?.voice).toBe("RH");
  expect(geo.pointAt(l, KEYBOARD_W + 10, rh!.bottom + LANE_GAP / 2)?.voice).toBe("RH");
  expect(geo.pointAt(l, KEYBOARD_W + 10, lh!.top + 1)?.voice).toBe("LH");
  expect(geo.pointAt(l, KEYBOARD_W + 10, STRIP_H - 1)).toBeNull();
  expect(geo.pointAt(l, KEYBOARD_W - 1, 300)).toBeNull();
  // The strip maps across the whole song, whatever is in view.
  const zoomed = geo.layout(song, W, H, true, { start: 0, end: BAR / 2 });
  expect(geo.stripAt(zoomed, KEYBOARD_W + 400, 5)).toBe(BAR / 2);
});

test("a click in a lane picks that voice, even where another is nearer in pitch", () => {
  const song = compile("RH: 1 . |\nLH: . 1 |\n");
  const lh = song.slots.findIndex((s) => s.voice === "LH");
  // At the start, only RH sounds; asking in LH's lane gets LH's rest.
  expect(geo.slotAt(song, 10, 60, "LH")).toBe(lh);
  expect(geo.slotAt(song, 10, 60, null)).toBe(song.slots.findIndex((s) => s.voice === "RH"));
});

// ── edges ───────────────────────────────────────────────────────────────────

test("where two notes touch, each side of the boundary grabs its own note", () => {
  // RH's note ends where LH's starts, on the same pitch, in one shared lane.
  const song = compile("octave LH 4\nRH: . 1 . . |\nLH: . . 1 - |\n");
  const l = geo.layout(song, W, H, false, null);
  const [x, y] = at(l, BAR / 2, 60);
  const right = geo.edgeAt(song, l, x + 2, y);
  expect(right?.side).toBe("start");
  expect(right?.event.voice).toBe("LH");
  const left = geo.edgeAt(song, l, x - 2, y);
  expect(left?.side).toBe("end");
  expect(left?.event.voice).toBe("RH");
});

test("a lone edge can be grabbed from just outside the note", () => {
  const song = compile("RH: 1 . . . |\n");
  const l = geo.layout(song, W, H, false, null);
  const [x, y] = at(l, BAR / 4, 60);
  expect(geo.edgeAt(song, l, x + 3, y)?.side).toBe("end");
  expect(geo.edgeAt(song, l, x + 6, y)).toBeNull();
  // The middle of a note is not an edge, nor is another row.
  expect(geo.edgeAt(song, l, geo.xOf(l, BAR / 8), y)).toBeNull();
  expect(geo.edgeAt(song, l, x, y - l.rowH)).toBeNull();
});

test("in lanes, edges only match the lane's voice", () => {
  const song = compile("octave LH 4\nRH: 1 . |\nLH: 1 . |\n");
  const l = geo.layout(song, W, H, true, null);
  const [x, y] = at(l, BAR / 2, 60, "LH");
  expect(geo.edgeAt(song, l, x - 1, y)?.event.voice).toBe("LH");
});

// ── the view ────────────────────────────────────────────────────────────────

const LEN = BAR * 8;

test("zooming keeps the tick under the pointer in place", () => {
  const v = geo.zoom(null, LEN, 4, LEN / 4)!;
  expect(v.end - v.start).toBe(LEN / 4);
  // A quarter of the way across before, and after.
  expect((LEN / 4 - v.start) / (v.end - v.start)).toBeCloseTo(0.25);
});

test("zoom stops at a quarter note and at the whole song", () => {
  expect(geo.zoom(null, LEN, 1e6, 0)).toEqual({ start: 0, end: MIN_SPAN });
  expect(geo.zoom({ start: 0, end: BAR }, LEN, 1 / 100, 0)).toBeNull();
});

test("panning stays inside the song, and the whole song does not pan", () => {
  expect(geo.pan({ start: 0, end: BAR }, LEN, -500)).toEqual({ start: 0, end: BAR });
  expect(geo.pan({ start: 0, end: BAR }, LEN, LEN * 2)).toEqual({ start: LEN - BAR, end: LEN });
  expect(geo.pan(null, LEN, 100)).toBeNull();
});

test("revealing scrolls only as far as needed", () => {
  const v = { start: BAR, end: BAR * 2 };
  expect(geo.reveal(v, LEN, BAR + 10, BAR + 20)).toBe(v);
  expect(geo.reveal(v, LEN, BAR * 2, BAR * 2 + 100)).toEqual({ start: BAR + 100, end: BAR * 2 + 100 });
  expect(geo.reveal(v, LEN, 0, 100)).toEqual({ start: 0, end: BAR });
});

test("following turns the page as the playhead runs off, and after a loop", () => {
  const v = { start: 0, end: BAR };
  expect(geo.follow(v, LEN, BAR - 10, BAR - 5)).toBe(v);
  expect(geo.follow(v, LEN, BAR - 10, BAR + 5)).toEqual({ start: BAR + 5, end: BAR * 2 + 5 });
  expect(geo.follow({ start: BAR * 4, end: BAR * 5 }, LEN, BAR * 5 - 1, 0)).toEqual({ start: 0, end: BAR });
  // Scrolled away by hand while playing: left alone until the playhead crosses the edge.
  expect(geo.follow({ start: BAR * 4, end: BAR * 5 }, LEN, 100, 200)).toEqual({ start: BAR * 4, end: BAR * 5 });
});

test("a view is pulled back inside a song that got shorter", () => {
  expect(geo.clamp({ start: BAR * 6, end: BAR * 8 }, BAR * 4)).toEqual({ start: BAR * 2, end: BAR * 4 });
  expect(geo.clamp({ start: 0, end: BAR * 8 }, BAR * 4)).toBeNull();
});
