// The page: builds the shared state and hands it to each part — the source
// pane, the roll, the settings bar, files — then compiles and starts drawing.

import { compile } from "../src/compile";
import { loadFig } from "../src/fig";
import { TuneError } from "../src/types";
import { $, escapeHtml, keyOf, type App, type SlotKey } from "./app";
import { EXAMPLES } from "./examples";
import { NAME_KEY, setupFiles } from "./files";
import { History } from "./history";
import { Player } from "./player";
import { PianoRoll, voiceColor } from "./roll";
import { setupRollInput } from "./rollinput";
import { setupSettings } from "./settings";
import { readShareHash } from "./share";
import { setupTextPane } from "./textpane";

const STORAGE_KEY = "tune:source";

const source = $<HTMLTextAreaElement>("source");
const canvas = $<HTMLCanvasElement>("roll");
const status = $<HTMLElement>("status");
const playButton = $<HTMLButtonElement>("play");
const loopBox = $<HTMLInputElement>("loop");

/** The selected slot, by where it is written, so selection survives a recompile. */
let selectedKey: SlotKey | null = null;

const app: App = {
  source,
  canvas,
  roll: new PianoRoll(canvas),
  player: new Player(),
  history: new History(source, () => app.recompile()),
  song: null,
  compiled: "",
  errors: [],
  errorSource: "",
  figReady: false,
  selected: null,

  recompile() {
    localStorage.setItem(STORAGE_KEY, source.value);
    try {
      const song = (app.song = compile(source.value));
      app.compiled = source.value;
      app.errors = [];
      app.player.setSong(song);
      app.setStatus(`${song.events.length} notes · ${song.voices.map((v, i) => `<span style="color:${voiceColor(i)}">${v}</span>`).join(" ")}`, "html");
      settings.sync(song);
      reselect();
    } catch (e) {
      if (!(e instanceof TuneError)) throw e;
      app.errors = e.errors;
      app.errorSource = source.value;
      text.describeErrors();
    }
    text.renderGutter();
    text.renderHighlight();
  },

  select(index) {
    const { song } = app;
    app.selected = index;
    selectedKey = index === null || !song ? null : keyOf(song.slots[index]!);
    if (index !== null && song) app.roll.reveal(song.slots[index]!.start, song.slots[index]!.end);
    app.describeSelection();
    if (song) settings.sync(song);
    text.renderHighlight(document.activeElement !== source);
  },

  selectKey(key) {
    selectedKey = key;
    reselect();
  },

  tryEdit(fn) {
    if (!app.figReady) {
      app.setStatus("fig is still loading…", "error");
      return false;
    }
    try {
      app.history.commit(fn());
      return true;
    } catch (e) {
      app.setStatus(`edit refused: ${(e as Error).message}`, "error");
      return false;
    }
  },

  setStatus(text, kind = "text") {
    status.className = kind === "error" ? "error" : "";
    if (kind === "html") status.innerHTML = text;
    else status.textContent = text;
  },

  describeSelection() {
    const { song, selected } = app;
    if (!song || selected === null || app.errors.length > 0) return;
    const s = song.slots[selected]!;
    const where = `${s.voice} · block ${s.block + 1} · bar ${s.bar + 1}${s.path.length > 1 ? ` · ${s.path.map((p) => p + 1).join(".")}` : ""}`;
    app.setStatus(`<b>${escapeHtml(s.text)}</b>  ${where}`, "html");
  },
};

function reselect(): void {
  const { song } = app;
  if (!song || selectedKey === null) return app.select(null);
  const i = song.slots.findIndex((s) => keyOf(s) === selectedKey);
  app.select(i === -1 ? null : i);
}

const text = setupTextPane(app);
const settings = setupSettings(app);
const files = setupFiles(app);
const rollInput = setupRollInput(app, togglePlay);

// ── undo ────────────────────────────────────────────────────────────────────

// ⌘Z / ⌘⇧Z (and Ctrl+Y) in the editor or on the roll; other inputs keep their own undo.
document.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey) || ![source, canvas, document.body].includes(e.target as HTMLElement)) return;
  const key = e.key.toLowerCase();
  if (key === "z" || key === "y") {
    e.preventDefault();
    if (key === "y" || e.shiftKey) app.history.redo();
    else app.history.undo();
  }
});
// The Edit menu's Undo and Redo reach the textarea as input events.
source.addEventListener("beforeinput", (e) => {
  if (e.inputType === "historyUndo" || e.inputType === "historyRedo") {
    e.preventDefault();
    if (e.inputType === "historyUndo") app.history.undo();
    else app.history.redo();
  }
});

// ── transport ───────────────────────────────────────────────────────────────

function togglePlay(): void {
  if (app.player.playing) app.player.stop();
  else void app.player.play();
}

// The button follows the player, whatever started or stopped it.
app.player.onChange = (playing) => {
  playButton.classList.toggle("playing", playing);
  playButton.textContent = playing ? "■ Stop" : "▶ Play";
};

playButton.addEventListener("click", togglePlay);
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    e.preventDefault();
    togglePlay();
  }
});
loopBox.addEventListener("change", () => (app.player.loop = loopBox.checked));

// ── drawing, start-up ───────────────────────────────────────────────────────

function frame(): void {
  const { roll, player } = app;
  if (player.playing) roll.follow(player.position());
  const preview = rollInput.preview();
  if (preview) roll.draw(preview.song, player.position(), false, preview.selected);
  else roll.draw(app.song, player.position(), app.errors.length > 0, app.selected);
  requestAnimationFrame(frame);
}

/** Opens the tune a share link carries, else the last one edited, else the first example. */
async function start(): Promise<void> {
  const stored = localStorage.getItem(STORAGE_KEY);
  source.value = stored ?? EXAMPLES["Mary had a little lamb"]!;
  app.history.reset(source.value);
  files.setName(localStorage.getItem(NAME_KEY) ?? (stored === null ? "Mary had a little lamb" : "tune"));
  let opened = false;
  let notice: string | null = null;
  try {
    const shared = await readShareHash(location.hash);
    if (shared) {
      // What was here before stays one undo away.
      files.load(shared.source, shared.name ?? "tune");
      opened = true;
      if (stored !== null && stored !== shared.source) notice = "Opened a shared tune · ⌘Z brings back what you had";
    }
  } catch {
    notice = "That share link is damaged, so it could not be opened";
  }
  if (location.hash) history.replaceState(null, "", location.pathname + location.search);
  if (!opened) app.recompile();
  if (notice) app.setStatus(notice, opened ? "text" : "error");
  requestAnimationFrame(frame);
}
void start();

loadFig().then(
  () => (app.figReady = true),
  (e) => app.setStatus(`fig failed to load, so the GUI cannot edit: ${(e as Error).message}`, "error"),
);
