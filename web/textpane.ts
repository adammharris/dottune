// The source pane: the textarea, its line-number gutter, and the mirror
// behind it that marks the selected token and every error.

import type { TuneError } from "../src/types";
import { $, escapeHtml, type App } from "./app";

export interface TextPane {
  renderGutter(): void;
  /** Re-marks the selection and errors; `reveal` scrolls the first mark into view. */
  renderHighlight(reveal?: boolean): void;
  /** Puts the first error, and how many more there are, in the status bar. */
  describeErrors(): void;
}

export function setupTextPane(app: App): TextPane {
  const { source } = app;
  const gutter = $<HTMLPreElement>("gutter");
  const highlight = $<HTMLPreElement>("highlight");
  const status = $<HTMLElement>("status");

  /** Where an error is, as "line 3, col 7". */
  function whereIs(e: TuneError): string {
    if (!e.span) return `line ${e.line}`;
    const col = e.span[0] - (app.errorSource.lastIndexOf("\n", e.span[0] - 1) + 1) + 1;
    return `line ${e.line}, col ${col}`;
  }

  function describeErrors(): void {
    const first = app.errors[0]!;
    const more = app.errors.length > 1 ? `   (+${app.errors.length - 1} more: hover the red line numbers)` : "";
    app.setStatus(`${whereIs(first)}: ${first.reason}${more}`, "error");
  }

  function renderGutter(): void {
    const count = source.value.split("\n").length;
    const byLine = new Map<number, string[]>();
    for (const e of app.errors) byLine.set(e.line, [...(byLine.get(e.line) ?? []), `${whereIs(e)}: ${e.reason}`]);
    gutter.innerHTML = Array.from({ length: count }, (_, i) => {
      const messages = byLine.get(i + 1);
      return messages ? `<span class="err" title="${escapeHtml(messages.join("\n"))}">${i + 1}</span>` : String(i + 1);
    }).join("\n");
    gutter.scrollTop = source.scrollTop;
  }

  /**
   * Marks the selected token and every error in the source. Each is left out
   * while the source has moved on from the compile that found it.
   */
  function renderHighlight(reveal = false): void {
    const text = source.value;
    const { song, selected, errors } = app;
    const marks: [number, number, "sel" | "err"][] = [];
    const span = song && selected !== null && app.compiled === text ? song.slots[selected]!.span : null;
    if (span) marks.push([...span, "sel"]);
    if (app.errorSource === text) for (const e of errors) if (e.span) marks.push([...e.span, "err"]);
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
    const mark = highlight.querySelector(app.errorSource === text && errors.length ? "mark.err" : "mark.sel") as HTMLElement | null;
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

  /** Clicking an error in the status bar puts the caret on it. */
  status.addEventListener("click", () => {
    const span = app.errors[0]?.span;
    if (!span || app.errorSource !== source.value) return;
    source.focus();
    source.setSelectionRange(span[0], span[1]);
    renderHighlight(true);
  });

  let debounce: ReturnType<typeof setTimeout> | undefined;
  source.addEventListener("input", () => {
    app.history.typed();
    renderGutter();
    renderHighlight();
    clearTimeout(debounce);
    debounce = setTimeout(() => app.recompile(), 150);
  });
  source.addEventListener("scroll", () => {
    gutter.scrollTop = highlight.scrollTop = source.scrollTop;
    highlight.scrollLeft = source.scrollLeft;
  });

  /** Placing the caret on a token selects it, as clicking it on the roll would. */
  function selectAtCaret(): void {
    const { song } = app;
    if (!song || app.compiled !== source.value || source.selectionStart !== source.selectionEnd) return;
    const at = source.selectionStart;
    const i = song.slots.findIndex((s) => s.span && s.span[0] <= at && at <= s.span[1]);
    app.select(i === -1 ? null : i);
  }
  source.addEventListener("click", () => {
    app.history.stopTyping();
    selectAtCaret();
  });
  source.addEventListener("keyup", (e) => {
    if (e.key.startsWith("Arrow") || e.key === "Home" || e.key === "End") {
      app.history.stopTyping();
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

  return { renderGutter, renderHighlight, describeErrors };
}
