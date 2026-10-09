// The settings bar: key, tempo, time, and each voice's octave, for the block
// the selected note is in (else the first). Changing one writes its directive.

import { defaultOctave } from "../src/compile";
import type { Path } from "../src/edit";
import { applyEdits } from "../src/fig";
import type { Song } from "../src/types";
import { $, type App } from "./app";
import { voiceColor } from "./roll";

const TONICS = ["C", "C#", "Db", "D", "Eb", "E", "F", "F#", "Gb", "G", "Ab", "A", "Bb", "B"];
const MODES = ["major", "minor", "dorian", "phrygian", "lydian", "mixolydian", "locrian", "ionian", "aeolian"];
const TIMES = ["2/4", "3/4", "4/4", "5/4", "6/8", "7/8", "9/8", "12/8"];

export interface Settings {
  /** Shows the settings in force for the settings block, leaving alone whichever control is being used. */
  sync(song: Song): void;
}

export function setupSettings(app: App): Settings {
  const tonicSel = $<HTMLSelectElement>("tonic");
  const modeSel = $<HTMLSelectElement>("mode");
  const tempoIn = $<HTMLInputElement>("tempo");
  const timeSel = $<HTMLSelectElement>("time");
  const octavesEl = $<HTMLElement>("octaves");
  const scopeEl = $<HTMLElement>("scope");

  for (const t of TONICS) tonicSel.add(new Option(t, t));
  for (const m of MODES) modeSel.add(new Option(m, m));
  for (const t of TIMES) timeSel.add(new Option(t, t));

  /** The block the settings bar edits: the selected note's, else the first. */
  const settingsBlock = () => (app.song && app.selected !== null ? app.song.slots[app.selected]!.block : 0);

  function sync(song: Song): void {
    const block = settingsBlock();
    const s = song.blocks[block] ?? { ...song, directives: [] };
    scopeEl.textContent = song.blocks.length > 1 ? (block === 0 ? "from the start:" : `from block ${block + 1}:`) : "";
    const [tonic, mode] = s.key.split(" ");
    const set = (el: HTMLSelectElement | HTMLInputElement, value: string) => {
      if (document.activeElement === el) return;
      if (el instanceof HTMLSelectElement && ![...el.options].some((o) => o.value === value)) el.add(new Option(value, value));
      el.value = value;
    };
    set(tonicSel, tonic!);
    set(modeSel, mode!);
    set(tempoIn, String(s.tempo));
    set(timeSel, s.time.join("/"));

    const current = [...octavesEl.querySelectorAll("input")].map((i) => i.dataset.voice).join(",");
    if (current !== song.voices.join(",")) {
      octavesEl.replaceChildren(
        ...song.voices.map((v, i) => {
          const label = document.createElement("label");
          label.style.color = voiceColor(i);
          const input = document.createElement("input");
          input.type = "number";
          input.min = "0";
          input.max = "8";
          input.dataset.voice = v;
          input.title = `Octave ${v}'s first note starts in`;
          input.addEventListener("change", () => setOctave(v, Number(input.value)));
          label.append(v, input);
          return label;
        }),
      );
    }
    for (const input of octavesEl.querySelectorAll("input")) {
      const v = input.dataset.voice!;
      if (document.activeElement !== input) input.value = String(s.octaves[v] ?? defaultOctave(v));
    }
  }

  tonicSel.addEventListener("change", () => setSetting(["key"], `${tonicSel.value} ${modeSel.value}`));
  modeSel.addEventListener("change", () => setSetting(["key"], `${tonicSel.value} ${modeSel.value}`));
  tempoIn.addEventListener("change", () => {
    const bpm = Number(tempoIn.value);
    if (bpm > 0) setSetting(["tempo"], bpm);
  });
  timeSel.addEventListener("change", () => setSetting(["time"], timeSel.value));

  function setOctave(voice: string, n: number): void {
    if (Number.isInteger(n)) setSetting(["octave", voice], n);
  }

  /**
   * Sets a directive for the settings block: the header for the first block, a
   * change before any other. fig edits or adds it in place; where fig cannot —
   * a header or change with nothing to add after, or no `octave` mapping to add
   * to — the directive line is written as text instead.
   */
  function setSetting(path: Path, value: string | number): void {
    const { song, source } = app;
    if (!song || !app.figReady) return;
    const block = settingsBlock();
    const full: Path = block === 0 ? path : ["blocks", block, ...path];
    try {
      app.history.commit(applyEdits(source.value, (ed) => ed.setValue(full, value)));
      return;
    } catch {
      // fall through to a text insertion
    }
    const line = `${path.join(" ")} ${value}`;
    const lines = source.value.split("\n");
    let at: number;
    if (block === 0) {
      // After the header's last directive, before the first voice.
      const firstVoice = song.blocks[0] ? song.blocks[0].line - 1 : lines.length;
      at = 0;
      for (let i = 0; i < firstVoice; i++) {
        if (/^(key|tempo|time|octave)\b/.test(lines[i]!.replace(/\/\/.*/, "").trim())) at = i + 1;
      }
    } else {
      // Just above the block's first voice line, after any change already there.
      at = song.blocks[block]!.line - 1;
    }
    lines.splice(at, 0, line);
    app.tryEdit(() => lines.join("\n"));
  }

  return { sync };
}
