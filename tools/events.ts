// Prints the event list for a tune file (or stdin).
//
//   bun tools/events.ts song.tune
//   echo 'RH: 1 2 3 |' | bun tools/events.ts

import { compile, formatSong } from "../src/compile";
import { TuneError } from "../src/types";

const path = process.argv[2];
const source = path ? await Bun.file(path).text() : await new Response(Bun.stdin.stream()).text();

try {
  console.log(formatSong(compile(source)));
} catch (e) {
  if (!(e instanceof TuneError)) throw e;
  console.error(`${path ?? "<stdin>"}:${e.message}`);
  process.exit(1);
}
