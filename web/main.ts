import { compile, defaultOctave } from "../src/compile";
import type { Path } from "../src/edit";
import { moveBy, setDegree, split, toggleAccidental, toHold, toRest, type TokenEdit } from "../src/edit";
import { applyEdits, applyMusicalEdit, loadFig } from "../src/fig";
import { writeMidi } from "../src/midi";
import { TuneError, type SlotInfo, type Song } from "../src/types";
import { EXAMPLES } from "./examples";
import { Player } from "./player";
import { PianoRoll, voiceColor } from "./roll";
import { readShareHash, shareHash } from "./share";

const STORAGE_KEY = "tune:source";
const NAME_KEY = "tune:name";
const TONICS = ["C", "C#", "Db", "D", "Eb", "E", "F", "F#", "Gb", "G", "Ab", "A", "Bb", "B"];
const MODES = ["major", "minor", "dorian", "phrygian", "lydian", "mixolydian", "locrian", "ionian", "aeolian"];
const TIMES = ["2/4", "3/4", "4/4", "5/4", "6/8", "7/8", "9/8", "12/8"];

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const source = $<HTMLTextAreaElement>("source");
const gutter = $<HTMLPreElement>("gutter");
const highlight = $<HTMLPreElement>("highlight");
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
const nameIn = $<HTMLInputElement>("name");
const fileIn = $<HTMLInputElement>("file");
const editorEl = $<HTMLElement>("editor");

const player = new Player();
const roll = new PianoRoll(canvas);

/** Last song that compiled; kept playing while the source has errors. */
let song: Song | null = null;
/** The text `song` was compiled from; slot spans are only valid while it matches the source. */
let compiled = "";
/** Every error in the source as of the last compile; empty when it compiled. */
let errors: TuneError[] = [];
/** The text `errors` were found in; their spans are only valid while it matches the source. */
let errorSource = "";
let figReady = false;

// ── compiling ───────────────────────────────────────────────────────────────

function recompile(): void {
  localStorage.setItem(STORAGE_KEY, source.value);
  try {
    song = compile(source.value);
    compiled = source.value;
    errors = [];
    player.setSong(song);
    setStatus(`${song.events.length} notes · ${song.voices.map((v, i) => `<span style="color:${voiceColor(i)}">${v}</span>`).join(" ")}`, "html");
    syncSettings(song);
    reselect();
  } catch (e) {
    if (!(e instanceof TuneError)) throw e;
    errors = e.errors;
    errorSource = source.value;
    describeErrors();
  }
  renderGutter();
  renderHighlight();
}

/** Where an error is, as "line 3, col 7". */
function whereIs(e: TuneError): string {
  if (!e.span) return `line ${e.line}`;
  const col = e.span[0] - (errorSource.lastIndexOf("\n", e.span[0] - 1) + 1) + 1;
  return `line ${e.line}, col ${col}`;
}

function describeErrors(): void {
  const first = errors[0]!;
  const more = errors.length > 1 ? `   (+${errors.length - 1} more: hover the red line numbers)` : "";
  setStatus(`${whereIs(first)}: ${first.reason}${more}`, "error");
}

/** Clicking an error in the status bar puts the caret on it. */
status.addEventListener("click", () => {
  const span = errors[0]?.span;
  if (!span || errorSource !== source.value) return;
  source.focus();
  source.setSelectionRange(span[0], span[1]);
  renderHighlight(true);
});

function setStatus(text: string, kind: "html" | "error" | "text" = "text"): void {
  status.className = kind === "error" ? "error" : "";
  if (kind === "html") status.innerHTML = text;
  else status.textContent = text;
}

function renderGutter(): void {
  const count = source.value.split("\n").length;
  const byLine = new Map<number, string[]>();
  for (const e of errors) byLine.set(e.line, [...(byLine.get(e.line) ?? []), `${whereIs(e)}: ${e.reason}`]);
  gutter.innerHTML = Array.from({ length: count }, (_, i) => {
    const messages = byLine.get(i + 1);
    return messages ? `<span class="err" title="${escapeHtml(messages.join("\n"))}">${i + 1}</span>` : String(i + 1);
  }).join("\n");
  gutter.scrollTop = source.scrollTop;
}

let debounce: ReturnType<typeof setTimeout> | undefined;
source.addEventListener("input", () => {
  recordTyping();
  renderGutter();
  renderHighlight();
  clearTimeout(debounce);
  debounce = setTimeout(recompile, 150);
});
source.addEventListener("scroll", () => {
  gutter.scrollTop = highlight.scrollTop = source.scrollTop;
  highlight.scrollLeft = source.scrollLeft;
});

/**
 * Marks the selected token and every error in the source. Each is left out
 * while the source has moved on from the compile that found it.
 */
function renderHighlight(reveal = false): void {
  const text = source.value;
  const marks: [number, number, "sel" | "err"][] = [];
  const span = song && selected !== null && compiled === text ? song.slots[selected]!.span : null;
  if (span) marks.push([...span, "sel"]);
  if (errorSource === text) for (const e of errors) if (e.span) marks.push([...e.span, "err"]);
  marks.sort((a, b) => a[0] - b[0]);
  let html = "";
  let at = 0;
  for (const [from, to, kind] of marks) {
    if (from < at) continue;
    html += `${escapeHtml(text.slice(at, from))}<mark class="${kind}">${escapeHtml(text.slice(from, to))}</mark>`;
    at = to;
  }
  // The trailing newline keeps the mirror as tall as the textarea when the text ends in one.
  highlight.innerHTML = `${html}${escapeHtml(text.slice(at))}\n`;
  const mark = highlight.querySelector(errorSource === text && errors.length ? "mark.err" : "mark.sel") as HTMLElement | null;
  if (reveal && mark) {
    const pad = 24;
    if (mark.offsetTop < source.scrollTop + pad) source.scrollTop = mark.offsetTop - pad;
    else if (mark.offsetTop + mark.offsetHeight > source.scrollTop + source.clientHeight - pad)
      source.scrollTop = mark.offsetTop + mark.offsetHeight - source.clientHeight + pad;
    if (mark.offsetLeft < source.scrollLeft + pad) source.scrollLeft = mark.offsetLeft - pad;
    else if (mark.offsetLeft + mark.offsetWidth > source.scrollLeft + source.clientWidth - pad)
      source.scrollLeft = mark.offsetLeft + mark.offsetWidth - source.clientWidth + pad;
  }
  highlight.scrollTop = source.scrollTop;
  highlight.scrollLeft = source.scrollLeft;
}

/** Placing the caret on a token selects it, as clicking it on the roll would. */
function selectAtCaret(): void {
  if (!song || compiled !== source.value || source.selectionStart !== source.selectionEnd) return;
  const at = source.selectionStart;
  const i = song.slots.findIndex((s) => s.span && s.span[0] <= at && at <= s.span[1]);
  select(i === -1 ? null : i);
}
source.addEventListener("click", () => {
  typing = false;
  selectAtCaret();
});
source.addEventListener("keyup", (e) => {
  if (e.key.startsWith("Arrow") || e.key === "Home" || e.key === "End") {
    typing = false;
    selectAtCaret();
  }
});
source.addEventListener("keydown", (e) => {
  if (e.key === "Tab" && !e.metaKey && !e.ctrlKey) {
    e.preventDefault();
    source.setRangeText("    ", source.selectionStart, source.selectionEnd, "end");
    source.dispatchEvent(new Event("input"));
  }
});

// ── editing through fig ─────────────────────────────────────────────────────

// One history for typing and GUI edits alike. Typing is recorded in bursts:
// an entry per pause, caret move, or GUI edit, not per keystroke.

const undoStack: string[] = [];
const redoStack: string[] = [];
/** The source as of the last change, so typing can record what it replaced. */
let lastSource = "";
/** While true, more typing joins the last undo entry instead of starting one. */
let typing = false;
let typingTimer: ReturnType<typeof setTimeout> | undefined;

function pushUndo(text: string): void {
  undoStack.push(text);
  if (undoStack.length > 200) undoStack.shift();
}

function recordTyping(): void {
  if (!typing) pushUndo(lastSource);
  typing = true;
  redoStack.length = 0;
  lastSource = source.value;
  clearTimeout(typingTimer);
  typingTimer = setTimeout(() => (typing = false), 1000);
}

/** Replaces the source with the result of an edit, recording it for undo. */
function commit(next: string): void {
  if (next === source.value) return;
  typing = false;
  pushUndo(source.value);
  redoStack.length = 0;
  setSource(next);
}

/** Replaces the source, leaving the caret at the end of what changed. */
function setSource(text: string): void {
  const before = source.value;
  const scroll = source.scrollTop;
  source.value = text;
  lastSource = text;
  let suffix = 0;
  const max = Math.min(before.length, text.length);
  let prefix = 0;
  while (prefix < max && before[prefix] === text[prefix]) prefix++;
  while (suffix < max - prefix && before[before.length - 1 - suffix] === text[text.length - 1 - suffix]) suffix++;
  source.setSelectionRange(text.length - suffix, text.length - suffix);
  source.scrollTop = scroll;
  recompile();
}

function undo(): void {
  typing = false;
  const prev = undoStack.pop();
  if (prev === undefined) return;
  redoStack.push(source.value);
  setSource(prev);
}

function redo(): void {
  typing = false;
  const next = redoStack.pop();
  if (next === undefined) return;
  pushUndo(source.value);
  setSource(next);
}

// ⌘Z / ⌘⇧Z (and Ctrl+Y) in the editor or on the roll; other inputs keep their own undo.
document.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey) || ![source, canvas, document.body].includes(e.target as HTMLElement)) return;
  const key = e.key.toLowerCase();
  if (key === "z" || key === "y") {
    e.preventDefault();
    if (key === "y" || e.shiftKey) redo();
    else undo();
  }
});
// The Edit menu's Undo and Redo reach the textarea as input events.
source.addEventListener("beforeinput", (e) => {
  if (e.inputType === "historyUndo" || e.inputType === "historyRedo") {
    e.preventDefault();
    if (e.inputType === "historyUndo") undo();
    else redo();
  }
});

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
  renderHighlight(document.activeElement !== source);
}

function reselect(): void {
  if (!song || selectedKey === null) return select(null);
  const i = song.slots.findIndex((s) => keyOf(s) === selectedKey);
  select(i === -1 ? null : i);
}

function describeSelection(): void {
  if (!song || selected === null || errors.length > 0) return;
  const s = song.slots[selected]!;
  const where = `${s.voice} · block ${s.block + 1} · bar ${s.bar + 1}${s.path.length > 1 ? ` · ${s.path.map((p) => p + 1).join(".")}` : ""}`;
  setStatus(`<b>${escapeHtml(s.text)}</b>  ${where}`, "html");
}

const escapeHtml = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");

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
  // A click moves the playhead: to the start of the note clicked, else to the click.
  // Shift-click moves it without changing the selection.
  const slot = e.shiftKey ? null : roll.slotAt(song, p.tick, p.midi);
  const s = slot !== null ? song.slots[slot] : undefined;
  player.seek(s ? s.start : p.tick);
  if (e.shiftKey) return;
  select(slot);
  if (s?.midi.length && !player.playing) void player.preview(s.midi, song.voices.indexOf(s.voice));
});

canvas.addEventListener("keydown", (e) => {
  if (e.metaKey || e.ctrlKey) return;
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
}

// The button follows the player, whatever started or stopped it.
player.onChange = (playing) => {
  playButton.classList.toggle("playing", playing);
  playButton.textContent = playing ? "■ Stop" : "▶ Play";
};

playButton.addEventListener("click", togglePlay);
document.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey)) return;
  const key = e.key.toLowerCase();
  if (key === "enter") {
    e.preventDefault();
    void togglePlay();
  } else if (key === "s") {
    e.preventDefault();
    save();
  } else if (key === "o") {
    e.preventDefault();
    fileIn.click();
  }
});

loopBox.addEventListener("change", () => (player.loop = loopBox.checked));

// ── files ───────────────────────────────────────────────────────────────────

/** The name Save and MIDI export use, without an extension. */
const fileName = () => nameIn.value.trim().replace(/[\\/:*?"<>|]+/g, "-") || "tune";

function setName(name: string): void {
  nameIn.value = name.replace(/\.(tune|txt)$/i, "");
  nameChanged();
}

function nameChanged(): void {
  localStorage.setItem(NAME_KEY, nameIn.value);
  document.title = `${fileName()} · tune`;
}
nameIn.addEventListener("input", nameChanged);

function download(data: BlobPart, type: string, filename: string): void {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([data], { type }));
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

function save(): void {
  download(source.value, "text/plain", `${fileName()}.tune`);
}

/** Replaces the tune with another, as one undoable step. */
function load(text: string, name: string): void {
  select(null);
  commit(text);
  setName(name);
}

async function openFile(file: File | undefined): Promise<void> {
  if (!file) return;
  load(await file.text(), file.name);
}

$<HTMLButtonElement>("open").addEventListener("click", () => fileIn.click());
fileIn.addEventListener("change", () => {
  void openFile(fileIn.files?.[0]);
  fileIn.value = "";
});
editorEl.addEventListener("dragover", (e) => {
  if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
});
editorEl.addEventListener("drop", (e) => {
  const file = e.dataTransfer?.files[0];
  if (!file) return;
  e.preventDefault();
  void openFile(file);
});

$<HTMLButtonElement>("save").addEventListener("click", save);

$<HTMLButtonElement>("export").addEventListener("click", () => {
  if (song) download(writeMidi(song), "audio/midi", `${fileName()}.mid`);
});

$<HTMLButtonElement>("share").addEventListener("click", async () => {
  const url = `${location.origin}${location.pathname}${await shareHash(source.value, fileName())}`;
  try {
    await navigator.clipboard.writeText(url);
    setStatus("Link copied to the clipboard");
  } catch {
    history.replaceState(null, "", url);
    setStatus("Copy the link from the address bar");
  }
});

for (const name of Object.keys(EXAMPLES)) examples.add(new Option(name, name));
examples.addEventListener("change", () => {
  const text = EXAMPLES[examples.value];
  if (text !== undefined) load(text, examples.value);
  examples.value = "";
});

function frame(): void {
  roll.draw(song, player.position(), errors.length > 0, selected);
  requestAnimationFrame(frame);
}

/** Opens the tune a share link carries, else the last one edited, else the first example. */
async function start(): Promise<void> {
  const stored = localStorage.getItem(STORAGE_KEY);
  source.value = lastSource = stored ?? EXAMPLES["Mary had a little lamb"]!;
  setName(localStorage.getItem(NAME_KEY) ?? (stored === null ? "Mary had a little lamb" : "tune"));
  let opened = false;
  let notice: string | null = null;
  try {
    const shared = await readShareHash(location.hash);
    if (shared) {
      // What was here before stays one undo away.
      load(shared.source, shared.name ?? "tune");
      opened = true;
      if (stored !== null && stored !== shared.source) notice = "Opened a shared tune · ⌘Z brings back what you had";
    }
  } catch {
    notice = "That share link is damaged, so it could not be opened";
  }
  if (location.hash) history.replaceState(null, "", location.pathname + location.search);
  if (!opened) recompile();
  if (notice) setStatus(notice, opened ? "text" : "error");
  requestAnimationFrame(frame);
}
void start();

loadFig().then(
  () => (figReady = true),
  (e) => setStatus(`fig failed to load, so the GUI cannot edit: ${(e as Error).message}`, "error"),
);
