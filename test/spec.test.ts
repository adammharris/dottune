import { expect, test } from "bun:test";
import { extractExamples, runExample, SPEC_PATH } from "../tools/spec-examples";

const examples = extractExamples(await Bun.file(SPEC_PATH).text());

test("SPEC.md has examples", () => {
  expect(examples.length).toBeGreaterThan(0);
});

for (const ex of examples) {
  test(`SPEC.md:${ex.specLine} ${ex.name}`, () => {
    expect(runExample(ex)).toBeNull();
  });
}
