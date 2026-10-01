// Extracts conformance examples from SPEC.md.
//
// A ```tune fence immediately followed by an ```events or ```error fence is an
// example. Other tune fences are illustrations and are skipped.
//
//   bun tools/spec-examples.ts          list examples
//   bun tools/spec-examples.ts --check  run them and show diffs for failures

import { compile, formatSong } from "../src/compile";
import { TuneError } from "../src/types";

export type Expectation = { kind: "events"; text: string } | { kind: "error"; line: number };

export interface Example {
  name: string;
  /** Line in SPEC.md where the tune fence starts. */
  specLine: number;
  source: string;
  expect: Expectation;
}

interface Fence {
  lang: string;
  body: string;
  line: number;
  heading: string;
}

export function extractExamples(markdown: string): Example[] {
  const fences: Fence[] = [];
  const lines = markdown.split("\n");
  let heading = "";
  for (let i = 0; i < lines.length; i++) {
    const h = /^#{1,6}\s+(.*)$/.exec(lines[i]!);
    if (h) heading = h[1]!;
    const open = /^```(\w*)\s*$/.exec(lines[i]!);
    if (!open) continue;
    const start = i;
    const body: string[] = [];
    while (++i < lines.length && !/^```\s*$/.test(lines[i]!)) body.push(lines[i]!);
    fences.push({ lang: open[1]!, body: body.join("\n"), line: start + 1, heading });
  }

  const examples: Example[] = [];
  const perHeading = new Map<string, number>();
  for (let i = 0; i < fences.length - 1; i++) {
    const tune = fences[i]!;
    const next = fences[i + 1]!;
    if (tune.lang !== "tune" || (next.lang !== "events" && next.lang !== "error")) continue;

    let expect: Expectation;
    if (next.lang === "events") {
      expect = { kind: "events", text: next.body.trim() };
    } else {
      const m = /^line\s+(\d+)$/.exec(next.body.trim());
      if (!m) throw new Error(`SPEC.md:${next.line}: error fence must contain "line <n>"`);
      expect = { kind: "error", line: Number(m[1]) };
    }

    const n = (perHeading.get(tune.heading) ?? 0) + 1;
    perHeading.set(tune.heading, n);
    examples.push({
      name: n === 1 ? tune.heading : `${tune.heading} #${n}`,
      specLine: tune.line,
      source: tune.body,
      expect,
    });
    i++;
  }
  return examples;
}

/** Runs one example; returns null on success or a description of the failure. */
export function runExample(ex: Example): string | null {
  let actual: string;
  try {
    actual = formatSong(compile(ex.source));
  } catch (e) {
    if (!(e instanceof TuneError)) throw e;
    if (ex.expect.kind === "error" && e.line === ex.expect.line) return null;
    return `expected ${describe(ex.expect)}, got error: ${e.message}`;
  }
  if (ex.expect.kind === "error") return `expected error on line ${ex.expect.line}, got events:\n${actual}`;
  if (actual === ex.expect.text) return null;
  return diff(ex.expect.text, actual);
}

function describe(e: Expectation): string {
  return e.kind === "error" ? `error on line ${e.line}` : "events";
}

function diff(expected: string, actual: string): string {
  const a = expected.split("\n");
  const b = actual.split("\n");
  const out: string[] = [];
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if (a[i] === b[i]) out.push(`  ${a[i]}`);
    else {
      if (a[i] !== undefined) out.push(`- ${a[i]}`);
      if (b[i] !== undefined) out.push(`+ ${b[i]}`);
    }
  }
  return out.join("\n");
}

export const SPEC_PATH = new URL("../SPEC.md", import.meta.url).pathname;

if (import.meta.main) {
  const examples = extractExamples(await Bun.file(SPEC_PATH).text());
  const check = process.argv.includes("--check");
  let failed = 0;
  for (const ex of examples) {
    const failure = check ? runExample(ex) : null;
    console.log(`${failure ? "FAIL" : check ? "ok  " : "    "} SPEC.md:${ex.specLine}  ${ex.name}`);
    if (failure) {
      failed++;
      console.log(failure.replace(/^/gm, "      "));
    }
  }
  console.log(`\n${examples.length} examples${check ? `, ${failed} failed` : ""}`);
  if (failed) process.exit(1);
}
