// One undo history for typing and GUI edits alike. Typing is recorded in
// bursts: an entry per pause, caret move, or GUI edit, not per keystroke.

const LIMIT = 200;

export class History {
  private undoStack: string[] = [];
  private redoStack: string[] = [];
  /** The source as of the last change, so typing can record what it replaced. */
  private last = "";
  /** While true, more typing joins the last undo entry instead of starting one. */
  private typing = false;
  private typingTimer: ReturnType<typeof setTimeout> | undefined;

  /** `changed` runs after the source is replaced by an edit, undo, or redo. */
  constructor(
    private source: HTMLTextAreaElement,
    private changed: () => void,
  ) {
    this.last = source.value;
  }

  /** Starts afresh from text put in the source directly. */
  reset(text: string): void {
    this.last = text;
  }

  /** Ends the current burst of typing, so the next starts an entry of its own. */
  stopTyping(): void {
    this.typing = false;
  }

  /** Records typing in the source. */
  typed(): void {
    if (!this.typing) this.push(this.last);
    this.typing = true;
    this.redoStack.length = 0;
    this.last = this.source.value;
    clearTimeout(this.typingTimer);
    this.typingTimer = setTimeout(() => (this.typing = false), 1000);
  }

  /** Replaces the source with the result of an edit, recording it for undo. */
  commit(next: string): void {
    if (next === this.source.value) return;
    this.typing = false;
    this.push(this.source.value);
    this.redoStack.length = 0;
    this.set(next);
  }

  undo(): void {
    this.typing = false;
    const prev = this.undoStack.pop();
    if (prev === undefined) return;
    this.redoStack.push(this.source.value);
    this.set(prev);
  }

  redo(): void {
    this.typing = false;
    const next = this.redoStack.pop();
    if (next === undefined) return;
    this.push(this.source.value);
    this.set(next);
  }

  private push(text: string): void {
    this.undoStack.push(text);
    if (this.undoStack.length > LIMIT) this.undoStack.shift();
  }

  /** Replaces the source, leaving the caret at the end of what changed. */
  private set(text: string): void {
    const { source } = this;
    const before = source.value;
    const scroll = source.scrollTop;
    source.value = text;
    this.last = text;
    const max = Math.min(before.length, text.length);
    let prefix = 0;
    let suffix = 0;
    while (prefix < max && before[prefix] === text[prefix]) prefix++;
    while (suffix < max - prefix && before[before.length - 1 - suffix] === text[text.length - 1 - suffix]) suffix++;
    source.setSelectionRange(text.length - suffix, text.length - suffix);
    source.scrollTop = scroll;
    this.changed();
  }
}
