// The settings bar: key, tempo, time, and each voice's octave, for the block
// the selected note is in (else the first). Changing one writes its directive;
// × beside one the block writes itself removes it, so the block takes the
// setting before it (or the default).

import { defaultOctave } from "../src/compile";
import type { Path } from "../src/edit";
import { clearDirective, setDirective } from "../src/structure";
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

  /** A × that removes `directive` from the settings block. */
  function clearButton(directive: string): HTMLButtonElement {
    const b = document.createElement("button");
    b.className = "clear";
    b.dataset.directive = directive;
    b.textContent = "×";
    b.addEventListener("click", (e) => {
      e.preventDefault();
      clear(directive);
    });
    return b;
  }
  for (const b of document.querySelectorAll<HTMLButtonElement>("header button.clear[data-directive]")) {
    b.addEventListener("click", (e) => {
      e.preventDefault();
      clear(b.dataset.directive!);
    });
  }

  function sync(song: Song): void {
    const block = settingsBlock();
    const s = song.blocks[block];
    const settings = s ?? { key: song.key, tempo: song.tempo, time: song.time, octaves: song.octaves };
    scopeEl.textContent = song.blocks.length > 1 ? (block === 0 ? "from the start:" : `from block ${block + 1}:`) : "";
    const [tonic, mode] = settings.key.split(" ");
    const set = (el: HTMLSelectElement | HTMLInputElement, value: string) => {
      if (document.activeElement === el) return;
      if (el instanceof HTMLSelectElement && ![...el.options].some((o) => o.value === value)) el.add(new Option(value, value));
      el.value = value;
    };
    set(tonicSel, tonic!);
    set(modeSel, mode!);
    set(tempoIn, String(settings.tempo));
    set(timeSel, settings.time.join("/"));

    const current = [...octavesEl.querySelectorAll("input")].map((i) => i.dataset.voice).join(",");
    if (current !== song.voices.join(",")) {
      octavesEl.replaceChildren(
        ...song.voices.map((v, i) => {
          const label = document.createElement("label");
          label.style.color = voiceColor(i);
          const input = document.createElement("input");
          input.type = "number";
          input.min = "-1";
          input.max = "9";
          input.dataset.voice = v;
          input.title = `Octave ${v}'s first note starts in`;
          input.addEventListener("change", () => setOctave(v, Number(input.value)));
          label.append(v, input, clearButton(`octave ${v}`));
          return label;
        }),
      );
    }
    for (const input of octavesEl.querySelectorAll("input")) {
      const v = input.dataset.voice!;
      if (document.activeElement !== input) input.value = String(settings.octaves[v] ?? defaultOctave(v));
    }
    const written = s?.directives ?? [];
    for (const b of document.querySelectorAll<HTMLButtonElement>("header button.clear")) {
      b.hidden = !written.includes(b.dataset.directive!);
      b.title = block === 0 ? "Remove from the header: back to the default" : `Remove from block ${block + 1}'s change: back to the block before`;
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

  function setSetting(path: Path, value: string | number): void {
    const { song } = app;
    if (!song) return;
    if (app.tryEdit(() => setDirective(app.source.value, song, settingsBlock(), path, value))) return;
    // Refused (say, a tempo out of range): show what the source still says.
    sync(app.song!);
  }

  function clear(directive: string): void {
    const { song } = app;
    if (song && song.blocks.length > 0) app.tryEdit(() => clearDirective(app.source.value, song, settingsBlock(), directive));
  }

  return { sync };
}
