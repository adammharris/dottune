// Exports a tune as a Standard MIDI File.
//
//   bun tools/midi.ts song.tune            writes song.mid
//   bun tools/midi.ts song.tune out.mid

import { compile } from "../src/compile";
import { writeMidi } from "../src/midi";
import { TuneError } from "../src/types";

const [input, output = input?.replace(/\.tune$/, "") + ".mid"] = process.argv.slice(2);
if (!input) {
  console.error("usage: bun tools/midi.ts <song.tune> [out.mid]");
  process.exit(2);
}

try {
  const song = compile(await Bun.file(input).text());
  await Bun.write(output, writeMidi(song));
  console.log(`${output}: ${song.events.length} notes, ${song.voices.length} tracks`);
} catch (e) {
  if (!(e instanceof TuneError)) throw e;
  console.error(`${input}:${e.message}`);
  process.exit(1);
}
