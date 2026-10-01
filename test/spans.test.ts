import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { compile } from "../src/compile";
import { EXAMPLES } from "../web/examples";

const sources = [
  ...readdirSync("examples").map((f) => readFileSync(`examples/${f}`, "utf8")),
  ...Object.values(EXAMPLES),
  "key C\r\ntempo 90\r\ntime 3/4\r\n\r\n  RH: [1 2] [3 [4 5]]|5' - - | . // comment\r\nLH:1|I7/3\r\n",
];

test("every written slot's span points at its text", () => {
  for (const src of sources) {
    for (const s of compile(src).slots) {
      if (!s.editable) expect(s.span).toBeNull();
      else expect(src.slice(...s.span!)).toBe(s.text);
    }
  }
});
