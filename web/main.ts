import { compile, defaultOctave } from "../src/compile";
import type { Path } from "../src/edit";
import { moveBy, setDegree, split, toggleAccidental, toHold, toRest, type TokenEdit } from "../src/edit";
import { applyEdits, applyMusicalEdit, loadFig } from "../src/fig";
import { writeMidi } from "../src/midi";
import { TuneError, type SlotInfo, type Song } from "../src/types";
import { EXAMPLES } from "./examples";
import { Player } from "./player";
import { PianoRoll, voiceColor } from "./roll";

const STORAGE_KEY = "tune:source";
const TONICS = ["C", "C#", "Db", "D", "Eb", "E", "F", "F#", "Gb", "G", "Ab", "A", "Bb", "B"];
const MODES = ["major", "minor", "dorian", "phrygian", "lydian", "mixolydian", "locrian", "ionian", "aeolian"];
const TIMES = ["2/4", "3/4", "4/4", "5/4", "6/8", "7/8", "9/8", "12/8"];

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const source = $<HTMLTextAreaElement>("source");
const gutter = $<HTMLPreElement>("gutter");
const status = $<HTMLElement>("status");
const playButton = $<HTMLButtonElement>("play");
const loopBox = $<HTMLInputElement>("loop");
const examples = $<HTMLSelectElement>("examples");
const canvas = $<HTMLCanvasElement>("roll");
const tonicSel = $<HTMLSelectElement>("tonic");
const modeSel = $<HTMLSelectElement>("mode");
const tempoIn = $<HTMLInputElement>("tempo");
const timeSel = $<HTMLSelectElement>("time");
const octavesEl = $<HTMLElement>("octaves");
const scopeEl = $<HTMLElement>("scope");

const player = new Player();
const roll = new PianoRoll(canvas);

/** Last song that compiled; kept playing while the source has errors. */
let song: Song | null = null;
let errorLine: number | null = null;
let figReady = false;

// ── compiling ───────────────────────────────────────────────────────────────

function recompile(): void {
  localStorage.setItem(STORAGE_KEY, source.value);
  try {
    song = compile(source.value);
    errorLine = null;
    player.setSong(song);
    setStatus(`${song.events.length} notes · ${song.voices.map((v, i) => `<span style="color:${voiceColor(i)}">${v}</span>`).join(" ")}`, "html");
    syncSettings(song);
    reselect();
  } catch (e) {
    if (!(e instanceof TuneError)) throw e;
    errorLine = e.line;
    setStatus(e.message, "error");
  }
  renderGutter();
}

function setStatus(text: string, kind: "html" | "error" | "text" = "text"): void {
  status.className = kind === "error" ? "error" : "";
  if (kind === "html") status.innerHTML = text;
  else status.textContent = text;
}

function renderGutter(): void {
  const count = source.value.split("\n").length;
  gutter.innerHTML = Array.from({ length: count }, (_, i) =>
    i + 1 === errorLine ? `<span class="err">${i + 1}</span>` : String(i + 1),
  ).join("\n");
  gutter.scrollTop = source.scrollTop;
}

let debounce: ReturnType<typeof setTimeout> | undefined;
source.addEventListener("input", () => {
  renderGutter();
  clearTimeout(debounce);
  debounce = setTimeout(recompile, 150);
});
source.addEventListener("scroll", () => (gutter.scrollTop = source.scrollTop));
source.addEventListener("keydown", (e) => {
  if (e.key === "Tab" && !e.metaKey && !e.ctrlKey) {
    e.preventDefault();
    source.setRangeText("    ", source.selectionStart, source.selectionEnd, "end");
    source.dispatchEvent(new Event("input"));
  }
});

// ── editing through fig ─────────────────────────────────────────────────────

const undoStack: string[] = [];
const redoStack: string[] = [];

/** Replaces the source with the result of an edit, recording it for undo. */
function commit(next: string): void {
  if (next === source.value) return;
  undoStack.push(source.value);
  if (undoStack.length > 200) undoStack.shift();
  redoStack.length = 0;
  setSource(next);
}

function setSource(text: string): void {
  const scroll = source.scrollTop;
  source.value = text;
  source.scrollTop = scroll;
  recompile();
}

function undo(): void {
  const prev = undoStack.pop();
  if (prev === undefined) return;
  redoStack.push(source.value);
  setSource(prev);
}

function redo(): void {
  const next = redoStack.pop();
  if (next === undefined) return;
  undoStack.push(source.value);
  setSource(next);
}

/** Runs an edit; a refusal (fig rolled it back) is reported, not thrown. */
function tryEdit(fn: () => string): boolean {
  if (!figReady) {
    setStatus("fig is still loading…", "error");
    return false;
  }
  try {
    commit(fn());
    return true;
  } catch (e) {
    setStatus(`edit refused: ${(e as Error).message}`, "error");
    return false;
  }
}

// ── selection ───────────────────────────────────────────────────────────────

/** Identifies a slot by where it is written, so selection survives a recompile. */
type SlotKey = string;
const keyOf = (s: Pick<SlotInfo, "voice" | "block" | "bar" | "path">): SlotKey => `${s.voice}|${s.block}|${s.bar}|${s.path.join(".")}`;

let selectedKey: SlotKey | null = null;
let selected: number | null = null;

function select(index: number | null): void {
  selected = index;
  selectedKey = index === null || !song ? null : keyOf(song.slots[index]!);
  describeSelection();
  if (song) syncSettings(song);
}

function reselect(): void {
  if (!song || selectedKey === null) return select(null);
  const i = song.slots.findIndex((s) => keyOf(s) === selectedKey);
  select(i === -1 ? null : i);
}

function describeSelection(): void {
  if (!song || selected === null || errorLine !== null) return;
  const s = song.slots[selected]!;
  const where = `${s.voice} · block ${s.block + 1} · bar ${s.bar + 1}${s.path.length > 1 ? ` · ${s.path.map((p) => p + 1).join(".")}` : ""}`;
  setStatus(`<b>${escapeHtml(s.text)}</b>  ${where}`, "html");
}

const escapeHtml = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;");

/** The neighbouring editable slot in the same voice. */
function neighbour(dir: 1 | -1): number | null {
  if (!song || selected === null) return null;
  const voice = song.slots[selected]!.voice;
  for (let i = selected + dir; i >= 0 && i < song.slots.length; i += dir) {
    const s = song.slots[i]!;
    if (s.voice !== voice) return null;
    if (s.editable) return i;
  }
  return null;
}

/** The slot sounding at the same time in the next (or previous) voice. */
function otherVoice(dir: 1 | -1): number | null {
  if (!song || selected === null) return null;
  const cur = song.slots[selected]!;
  const vi = song.voices.indexOf(cur.voice);
  const voice = song.voices[(vi + dir + song.voices.length) % song.voices.length];
  const i = song.slots.findIndex((s) => s.voice === voice && s.editable && cur.start >= s.start && cur.start < s.end);
  return i === -1 ? null : i;
}

/** Applies token edits to the selected slot and previews what it now sounds. */
function editSelected(op: (song: Song, index: number) => TokenEdit[], then?: (s: SlotInfo) => SlotKey): void {
  if (!song || selected === null) return;
  const s = song.slots[selected]!;
  const edits = op(song, selected);
  if (edits.length === 0) return;
  const current = song;
  const index = selected;
  if (!tryEdit(() => applyMusicalEdit(source.value, current, index, op))) return;
  if (then) {
    selectedKey = then(s);
    reselect();
  }
  const now = selected !== null ? song.slots[selected] : undefined;
  if (now?.midi.length) void player.preview(now.midi, song.voices.indexOf(now.voice));
}

canvas.addEventListener("mousedown", (e) => {
  canvas.focus();
  if (!song) return;
  const p = roll.pointAt(e.clientX, e.clientY);
  if (!p) return;
  if (e.shiftKey) return player.seek(p.tick);
  const slot = roll.slotAt(song, p.tick, p.midi);
  select(slot);
  const s = slot !== null ? song.slots[slot] : undefined;
  if (s?.midi.length) void player.preview(s.midi, song.voices.indexOf(s.voice));
});

canvas.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (mod && e.key.toLowerCase() === "z") {
    e.preventDefault();
    return e.shiftKey ? redo() : undo();
  }
  if (mod) return;
  if (e.key === " ") {
    e.preventDefault();
    return void togglePlay();
  }
  if (selected === null) return;

  const steps = e.shiftKey ? 7 : 1;
  const handled = (() => {
    switch (e.key) {
      case "ArrowUp":
        return editSelected((s, i) => moveBy(s, i, steps)), true;
      case "ArrowDown":
        return editSelected((s, i) => moveBy(s, i, -steps)), true;
      case "ArrowLeft":
      case "ArrowRight": {
        const n = neighbour(e.key === "ArrowLeft" ? -1 : 1);
        if (n !== null) select(n);
        return true;
      }
      case "Tab": {
        const n = otherVoice(e.shiftKey ? -1 : 1);
        if (n !== null) select(n);
        return true;
      }
      case ".":
      case "Backspace":
      case "Delete":
        return editSelected(toRest), true;
      case "-":
        return editSelected(toHold), true;
      case "s":
      case "/":
        return editSelected(split, (s) => keyOf({ ...s, path: [...s.path, 0] })), true;
      case "#":
        return editSelected((s, i) => toggleAccidental(s, i, "#")), true;
      case "b":
        return editSelected((s, i) => toggleAccidental(s, i, "b")), true;
      case "Escape":
        return select(null), true;
    }
    if (/^[1-7]$/.test(e.key)) return editSelected((s, i) => setDegree(s, i, Number(e.key) - 1)), true;
    return false;
  })();
  if (handled) e.preventDefault();
});

// ── settings ────────────────────────────────────────────────────────────────

for (const t of TONICS) tonicSel.add(new Option(t, t));
for (const m of MODES) modeSel.add(new Option(m, m));
for (const t of TIMES) timeSel.add(new Option(t, t));

/** The block the settings bar edits: the selected note's, else the first. */
const settingsBlock = () => (song && selected !== null ? song.slots[selected]!.block : 0);

/** Shows the settings in force for the settings block, leaving alone whichever control is being used. */
function syncSettings(song: Song): void {
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
  if (!song || !figReady) return;
  const block = settingsBlock();
  const full: Path = block === 0 ? path : ["blocks", block, ...path];
  try {
    commit(applyEdits(source.value, (ed) => ed.setValue(full, value)));
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
  tryEdit(() => lines.join("\n"));
}

// ── transport, examples, frame loop ─────────────────────────────────────────

async function togglePlay(): Promise<void> {
  if (player.playing) player.stop();
  else await player.play();
  playButton.classList.toggle("playing", player.playing);
  playButton.textContent = player.playing ? "■ Stop" : "▶ Play";
}

player.onStop = () => {
  playButton.classList.remove("playing");
  playButton.textContent = "▶ Play";
};

playButton.addEventListener("click", togglePlay);
document.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    void togglePlay();
  }
});

loopBox.addEventListener("change", () => (player.loop = loopBox.checked));

$<HTMLButtonElement>("export").addEventListener("click", () => {
  if (!song) return;
  const blob = new Blob([writeMidi(song)], { type: "audio/midi" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = "tune.mid";
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
});

for (const name of Object.keys(EXAMPLES)) examples.add(new Option(name, name));
examples.addEventListener("change", () => {
  const text = EXAMPLES[examples.value];
  if (text !== undefined) {
    select(null);
    commit(text);
  }
  examples.value = "";
});

function frame(): void {
  roll.draw(song, player.position(), errorLine !== null, selected);
  requestAnimationFrame(frame);
}

source.value = localStorage.getItem(STORAGE_KEY) ?? EXAMPLES["Mary had a little lamb"]!;
recompile();
requestAnimationFrame(frame);

loadFig().then(
  () => (figReady = true),
  (e) => setStatus(`fig failed to load, so the GUI cannot edit: ${(e as Error).message}`, "error"),
);
