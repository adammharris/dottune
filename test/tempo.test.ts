import { expect, test } from "bun:test";
import { compile } from "../src/compile";
import { TempoMap } from "../src/tempo";

test("one tempo", () => {
  const map = new TempoMap(compile("tempo 120\nRH: 1 |\n"));
  expect(map.total).toBeCloseTo(2);
  expect(map.seconds(480)).toBeCloseTo(0.5);
  expect(map.ticks(1.5)).toBeCloseTo(1440);
});

test("a tempo change between blocks", () => {
  // Block 1: 4 beats at 120 = 2 s. Block 2: 4 beats at 60 = 4 s.
  const map = new TempoMap(compile("tempo 120\nRH: 1 |\n\ntempo 60\nRH: 1 |\n"));
  expect(map.total).toBeCloseTo(6);
  expect(map.seconds(1920)).toBeCloseTo(2);
  expect(map.seconds(1920 + 480)).toBeCloseTo(3);
  expect(map.ticks(3)).toBeCloseTo(2400);
  expect(map.ticks(1)).toBeCloseTo(960);
});

test("an empty song", () => {
  const map = new TempoMap(compile("tempo 90\n"));
  expect(map.total).toBe(0);
  expect(map.seconds(480)).toBeCloseTo(480 / (1.5 * 480));
});
