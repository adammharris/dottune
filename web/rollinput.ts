// Everything done on the piano roll with the pointer and keyboard: selecting
// and editing notes, dragging their edges, and moving the view.

import { compile } from "../src/compile";
import {
  cutTick,
  lengthen,
  moveBy,
  onsetAt,
  onsetOf,
  resize,
  restart,
  setDegree,
  snapCut,
  snapStart,
  split,
  toggleAccidental,
  toHold,
  toRest,
  type TokenEdit,
} from "../src/edit";
import { applyMusicalEdit, applyTokenEdits } from "../src/fig";
import type { SlotInfo, Song } from "../src/types";
import { $, escapeHtml, keyOf, type App, type SlotKey } from "./app";
import { slotAt, type Edge } from "./geometry";
import { gestures, type Gesture } from "./gesture";

const LANES_KEY = "tune:lanes";

export interface RollInput {
  /** While an edge is being dragged: the song it would make, and the note to show selected in it. */
  preview(): { song: Song; selected: number } | null;
}

export function setupRollInput(app: App, togglePlay: () => void): RollInput {
  const { canvas, roll, player } = app;
  const lanesBox = $<HTMLInputElement>("lanes");

  // ── editing the selection ─────────────────────────────────────────────────

  /** Applies token edits to the selected slot and previews what it now sounds. */
  function editSelected(op: (song: Song, index: number) => TokenEdit[], then?: (s: SlotInfo) => SlotKey): void {
    const { song, selected } = app;
    if (!song || selected === null) return;
    const s = song.slots[selected]!;
    if (op(song, selected).length === 0) return;
    if (!app.tryEdit(() => applyMusicalEdit(app.source.value, song, selected, op))) return;
    if (then) app.selectKey(then(s));
    const now = app.song && app.selected !== null ? app.song.slots[app.selected] : undefined;
    if (now?.midi.length) void player.preview(now.midi, app.song!.voices.indexOf(now.voice));
  }

  /** Shift+←/→: shrinks or grows the selected note by a slot. */
  function lengthenSelected(dir: 1 | -1): void {
    const { song, selected } = app;
    if (!song || selected === null) return;
    const index = onsetOf(song, selected);
    if (index === null) return;
    const s = song.slots[index]!;
    const edits = lengthen(song, index, dir);
    if (edits.length === 0 || !app.tryEdit(() => applyTokenEdits(app.source.value, edits))) return;
    if (app.song !== song) {
      const i = onsetAt(app.song!, s.voice, s.start);
      app.select(i === -1 ? null : i);
    }
  }

  /** The neighbouring editable slot in the same voice. */
  function neighbour(dir: 1 | -1): number | null {
    const { song, selected } = app;
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
    const { song, selected } = app;
    if (!song || selected === null) return null;
    const cur = song.slots[selected]!;
    const vi = song.voices.indexOf(cur.voice);
    const voice = song.voices[(vi + dir + song.voices.length) % song.voices.length];
    const i = song.slots.findIndex((s) => s.voice === voice && s.editable && cur.start >= s.start && cur.start < s.end);
    return i === -1 ? null : i;
  }

  // ── dragging a note's edges ───────────────────────────────────────────────

  // Dragging a note's right edge changes its length; its left edge, where it
  // starts. The edge snaps to the slot under the pointer, split into halves
  // (thirds with ⌥) as finely as the zoom allows; the roll and status bar
  // show the result until the button is let go.

  /** The drag in progress, for `preview`. */
  let resizing: { song: Song; voice: string; start: number; preview: Song | null } | null = null;

  /** How many equal parts a slot may split into: powers of 2 (or 3), each at least 14px wide, up to 3 deep. */
  const partsFor = (base: number) => (s: SlotInfo) => {
    let n = 1;
    while (n < base ** 3 && roll.width((s.end - s.start) / (n * base)) >= 14) n *= base;
    return n;
  };

  /** A bar of `src` as written, brackets and all. */
  function barText(song: Song, src: string, block: number, voice: string, bar: number): string {
    const spans = song.slots.filter((s) => s.voice === voice && s.block === block && s.bar === bar && s.span).map((s) => s.span!);
    if (!spans.length) return "";
    let from = Math.min(...spans.map((s) => s[0]));
    let to = Math.max(...spans.map((s) => s[1]));
    while (from > 0 && /[\[\s]/.test(src[from - 1]!)) from--;
    while (to < src.length && /[\]\s]/.test(src[to]!)) to++;
    return src.slice(from, to).trim();
  }

  function resizeGesture(song: Song, edge: Edge): Gesture {
    const src = app.source.value;
    const index = edge.event.slot;
    const voice = song.slots[index]!.voice;
    const drag = (resizing = { song, voice, start: edge.event.start, preview: null as Song | null });
    let next: string | null = null;

    return {
      move(e) {
        const tick = roll.tickAt(e.clientX);
        const parts = partsFor(e.altKey ? 3 : 2);
        const cut = tick === null ? null : edge.side === "end" ? snapCut(song, index, tick, parts) : snapStart(song, index, tick, parts);
        const edits = !cut ? [] : edge.side === "end" ? resize(song, index, cut) : restart(song, index, cut);
        drag.preview = next = null;
        drag.start = cut && edge.side === "start" ? cutTick(song, cut) : song.slots[index]!.start;
        if (edits.length === 0) return app.describeSelection();
        try {
          const text = applyTokenEdits(src, edits);
          const nextSong = compile(text);
          [next, drag.preview] = [text, nextSong];
          const bars = [...new Set(edits.map((e) => e.path.slice(1, 4).join("|")))].map((key) => {
            const [block, v, bar] = key.split("|");
            return `bar ${Number(bar) + 1}: <b>${escapeHtml(barText(nextSong, text, Number(block), v!, Number(bar)))}</b>`;
          });
          app.setStatus(`${escapeHtml(voice)} → ${bars.join(" · ")}   (⌥ thirds · esc cancels)`, "html");
        } catch {
          app.describeSelection();
        }
      },
      end() {
        resizing = null;
        const text = next;
        if (text !== null && app.tryEdit(() => text) && app.song) {
          const i = onsetAt(app.song, voice, drag.start);
          app.select(i === -1 ? null : i);
        } else {
          app.describeSelection();
        }
      },
      cancel() {
        resizing = null;
        app.describeSelection();
      },
    };
  }

  // ── the view ──────────────────────────────────────────────────────────────

  // ⌘-scroll or pinch zooms about the pointer, scrolling pans, and the strip
  // across the top shows the whole song: click or drag it to move the view,
  // double-click a block to fit it. [ and ] step through the blocks, 0 fits all.

  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) {
        const tick = roll.tickAt(e.clientX);
        // A pinch arrives as ctrl+wheel with small deltas; a mouse wheel with large ones.
        if (tick !== null) roll.zoom(Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.01)), tick);
      } else {
        roll.pan(Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY);
      }
    },
    { passive: false },
  );

  /** Centres the view on `tick`, keeping its width. */
  const centreOn = (tick: number, span: number) => roll.show(tick - span / 2, tick + span / 2);

  /** Whether the view is exactly block `b`. */
  const fitted = (b: { start: number; end: number }) => {
    const v = roll.shown()!;
    return Math.abs(v.start - b.start) < 1 && Math.abs(v.end - b.end) < 1;
  };

  /** [ and ]: the previous or next block; from a view that is not a block, the block at its left edge. */
  function stepBlock(dir: 1 | -1): void {
    const { song } = app;
    const v = roll.shown();
    if (!song || !v || song.blocks.length === 0) return;
    const i = Math.max(0, song.blocks.findIndex((b) => v.start + 1 >= b.start && v.start + 1 < b.end));
    const next = fitted(song.blocks[i]!) ? i + dir : i;
    const b = song.blocks[next];
    if (b) roll.show(b.start, b.end);
  }

  canvas.addEventListener("dblclick", (e) => {
    const { song } = app;
    const tick = song && roll.stripAt(e.clientX, e.clientY);
    if (!song || tick === null) return;
    const b = song.blocks.find((b) => tick >= b.start && tick < b.end);
    // Double-clicking the block already fitted goes back to the whole song.
    if (!b || fitted(b)) roll.showAll();
    else roll.show(b.start, b.end);
  });

  /** Gives each voice its own lane, or puts them all in one. */
  function setLanes(on: boolean): void {
    roll.lanes = lanesBox.checked = on;
    localStorage.setItem(LANES_KEY, on ? "1" : "0");
  }
  lanesBox.addEventListener("change", () => setLanes(lanesBox.checked));
  setLanes(localStorage.getItem(LANES_KEY) !== "0");

  // ── the pointer ───────────────────────────────────────────────────────────

  gestures(
    canvas,
    (e) => {
      canvas.focus();
      const { song } = app;
      if (!song) return null;

      // The overview strip: drag to move the view.
      const stripTick = roll.stripAt(e.clientX, e.clientY);
      if (stripTick !== null) {
        e.preventDefault();
        const v = roll.shown()!;
        const span = v.end - v.start;
        if (span >= song.length) return null;
        centreOn(stripTick, span);
        const top = canvas.getBoundingClientRect().top;
        return {
          move(e) {
            const tick = roll.stripAt(e.clientX, top);
            if (tick !== null) centreOn(tick, span);
          },
        };
      }

      // A note's edge, while the roll shows what the source says.
      const edge = e.shiftKey || app.compiled !== app.source.value ? null : roll.edgeAt(song, e.clientX, e.clientY);
      if (edge) {
        e.preventDefault();
        app.select(edge.event.slot);
        return resizeGesture(song, edge);
      }

      // A click moves the playhead: to the start of the note clicked, else to the click.
      // Shift-click moves it without changing the selection.
      const p = roll.pointAt(e.clientX, e.clientY);
      if (!p) return null;
      const slot = e.shiftKey ? null : slotAt(song, p.tick, p.midi, p.voice);
      const s = slot !== null ? song.slots[slot] : undefined;
      player.seek(s ? s.start : p.tick);
      if (e.shiftKey) return null;
      app.select(slot);
      if (s?.midi.length && !player.playing) void player.preview(s.midi, song.voices.indexOf(s.voice));
      return null;
    },
    (e) => {
      if (app.song) canvas.style.cursor = roll.edgeAt(app.song, e.clientX, e.clientY) ? "ew-resize" : "";
    },
  );

  // ── the keyboard ──────────────────────────────────────────────────────────

  /** Keys that move the view; true if `e` was one. */
  function viewKey(e: KeyboardEvent): boolean {
    if (e.metaKey || e.ctrlKey) {
      const v = roll.shown();
      const s = app.song && app.selected !== null ? app.song.slots[app.selected] : undefined;
      const at = s ? s.start : v ? (v.start + v.end) / 2 : 0;
      if (e.key === "=" || e.key === "+") return roll.zoom(1.5, at), true;
      if (e.key === "-") return roll.zoom(1 / 1.5, at), true;
      if (e.key === "0") return roll.showAll(), true;
      return false;
    }
    switch (e.key) {
      case "[":
        return stepBlock(-1), true;
      case "]":
        return stepBlock(1), true;
      case "0":
        return roll.showAll(), true;
      case "l":
      case "L":
        return setLanes(!roll.lanes), true;
    }
    return false;
  }

  /** Keys that play, or edit and move the selection; true if `e` was one. */
  function editKey(e: KeyboardEvent): boolean {
    if (e.metaKey || e.ctrlKey) return false;
    if (e.key === " ") return togglePlay(), true;
    if (app.selected === null) return false;
    const steps = e.shiftKey ? 7 : 1;
    switch (e.key) {
      case "ArrowUp":
        return editSelected((s, i) => moveBy(s, i, steps)), true;
      case "ArrowDown":
        return editSelected((s, i) => moveBy(s, i, -steps)), true;
      case "ArrowLeft":
      case "ArrowRight": {
        const dir = e.key === "ArrowLeft" ? -1 : 1;
        if (e.shiftKey) return lengthenSelected(dir), true;
        const n = neighbour(dir);
        if (n !== null) app.select(n);
        return true;
      }
      case "Tab": {
        const n = otherVoice(e.shiftKey ? -1 : 1);
        if (n !== null) app.select(n);
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
        return app.select(null), true;
    }
    if (/^[1-7]$/.test(e.key)) return editSelected((s, i) => setDegree(s, i, Number(e.key) - 1)), true;
    return false;
  }

  canvas.addEventListener("keydown", (e) => {
    if (viewKey(e) || editKey(e)) e.preventDefault();
  });

  return {
    preview() {
      if (!resizing?.preview) return null;
      return { song: resizing.preview, selected: onsetAt(resizing.preview, resizing.voice, resizing.start) };
    },
  };
}
