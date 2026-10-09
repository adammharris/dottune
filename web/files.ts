// Getting tunes in and out: the name, Open (or a dropped file), Save, MIDI
// export, share links, and the examples.

import { writeMidi } from "../src/midi";
import { $, type App } from "./app";
import { EXAMPLES } from "./examples";
import { shareHash } from "./share";

export const NAME_KEY = "tune:name";

export interface Files {
  setName(name: string): void;
  /** Replaces the tune with another, as one undoable step. */
  load(text: string, name: string): void;
}

export function setupFiles(app: App): Files {
  const nameIn = $<HTMLInputElement>("name");
  const fileIn = $<HTMLInputElement>("file");
  const examples = $<HTMLSelectElement>("examples");

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
    download(app.source.value, "text/plain", `${fileName()}.tune`);
  }

  function load(text: string, name: string): void {
    app.select(null);
    app.history.commit(text);
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
  const editorEl = $<HTMLElement>("editor");
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
    if (app.song) download(writeMidi(app.song), "audio/midi", `${fileName()}.mid`);
  });

  $<HTMLButtonElement>("share").addEventListener("click", async () => {
    const url = `${location.origin}${location.pathname}${await shareHash(app.source.value, fileName())}`;
    try {
      await navigator.clipboard.writeText(url);
      app.setStatus("Link copied to the clipboard");
    } catch {
      history.replaceState(null, "", url);
      app.setStatus("Copy the link from the address bar");
    }
  });

  // ⌘S saves, ⌘O opens.
  document.addEventListener("keydown", (e) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const key = e.key.toLowerCase();
    if (key === "s") {
      e.preventDefault();
      save();
    } else if (key === "o") {
      e.preventDefault();
      fileIn.click();
    }
  });

  for (const name of Object.keys(EXAMPLES)) examples.add(new Option(name, name));
  examples.addEventListener("change", () => {
    const text = EXAMPLES[examples.value];
    if (text !== undefined) load(text, examples.value);
    examples.value = "";
  });

  return { setName, load };
}
