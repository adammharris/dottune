import { expect, test } from "bun:test";
import { readShareHash, shareHash } from "../web/share";
import { EXAMPLES } from "../web/examples";

test("a share link round-trips the source and name", async () => {
  for (const [name, source] of Object.entries(EXAMPLES)) {
    const hash = await shareHash(source, name);
    expect(hash).toMatch(/^#n=[^#]*&t=[A-Za-z0-9_-]+$/);
    expect(await readShareHash(hash)).toEqual({ name, source });
  }
});

test("non-ASCII text survives", async () => {
  const source = "// café ♩ 🎹\nRH: 1 |\n";
  expect((await readShareHash(await shareHash(source, "naïve")))?.source).toBe(source);
});

test("a hash without a tune shares nothing", async () => {
  expect(await readShareHash("")).toBeNull();
  expect(await readShareHash("#section")).toBeNull();
});

test("a damaged link throws", async () => {
  await expect(readShareHash("#t=AAAA")).rejects.toThrow();
});
