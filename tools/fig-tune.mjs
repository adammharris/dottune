// A fig language for .tune files, served to the `fig` CLI as a helper process.
// Configured in .fig/languages.figl; run with bun so it can import the compiler.
//
// The tree a tune reads as:
//
//   key: "C major"            ← directives
//   tempo: 100
//   time: "4/4"
//   octave: { LH: 2 }         ← every `octave <voice> <n>` line, merged
//   blocks:                   ← synthetic key: no text of its own
//     - RH: [[3, 2, 1, 2], [3, 3, 3, "-"]]   ← block → voice → bars → tokens
//       LH: [[I], [V]]
//     - RH: [[5]]
//       tempo: 90             ← a change between blocks belongs to the block after it
//
// Tokens are strings; a `[ ]` group is a nested sequence. In a block, voices
// come before settings in the tree, whatever the order in the text — see
// `blocks goes first` below for why.
//
// Editing is the generic splice engine plus one renderer, `entry`, which
// spells a new setting, octave, or voice line. Bars are not edited item by
// item (a bar has no delimiters for the engine to splice between); replace a
// whole bar or token raw instead — `replaceValueRaw(["blocks", 0, "RH", 1],
// "1 [2 3] 4 5")` — and fig reparses, rolling back anything that fails.
// `set` does not create the `octave` mapping when the file has none.

import { LanguageError, serve } from "@diaryx/fig/helper";
import { compile } from "../src/compile.ts";
import { TuneError } from "../src/types.ts";

const SETTINGS = ["key", "tempo", "time"];
const VOICE_LINE = /^([A-Za-z][A-Za-z0-9_]*)(\s*):/;

// ── parse ───────────────────────────────────────────────────────────────────
//
// Builds a tree with character spans, then flattens it into byte-span rows.
// A tree first because `octave` lines merge into one mapping that other
// directives may sit between, and rows must be in pre-order.

const scalar = (kind, start, end, text) => ({ kind, span: [start, end], text, leading: [], trailing: null });
const container = (kind, start, end) => ({ kind, span: [start, end], children: [], leading: [], trailing: null });
const entry = (key, value, start, end) => ({ kind: "keyvalue", span: [start, end], key, value, leading: [], trailing: null });

function validate(input) {
  try {
    compile(input);
  } catch (e) {
    if (!(e instanceof TuneError)) throw e;
    const lines = input.split("\n");
    const offset = e.span
      ? byteLength(input.slice(0, e.span[0]))
      : lines.slice(0, e.line - 1).reduce((n, l) => n + byteLength(l) + 1, 0);
    throw new LanguageError(e.message, offset);
  }
}

/** One bar's text → a sequence of tokens and groups. `base` is the bar's offset in the input. */
function parseBar(text, base) {
  const start = base + (text.length - text.trimStart().length);
  const end = base + text.trimEnd().length;
  const bar = container("sequence", start, end);
  const stack = [bar];
  const re = /\[|\]|[^\s\[\]]+/g;
  for (let m; (m = re.exec(text)); ) {
    const at = base + m.index;
    if (m[0] === "[") {
      const group = container("sequence", at, at + 1);
      stack.at(-1).children.push(group);
      stack.push(group);
    } else if (m[0] === "]") {
      stack.pop().span[1] = at + 1;
    } else {
      stack.at(-1).children.push(scalar("string", at, at + m[0].length, m[0]));
    }
  }
  return bar;
}

/** Where directive lines go: the root (the header), or a block (a change before it). */
function target(push, extend) {
  return { settings: new Map(), octaves: null, push, extend };
}

function newBlock(start, end) {
  const block = container("mapping", start, end);
  block.voices = [];
  block.settings = [];
  block.target = target((kv) => block.settings.push(kv), (e) => (block.span[1] = e));
  return block;
}

function parseTree(input) {
  const root = container("mapping", 0, input.length);
  root.dangling = [];
  const header = target((kv) => root.children.push(kv), () => {});
  let blocks = null;
  let block = null;
  /** A block opened by a change, waiting for its first voice line. */
  let changed = null;
  let pending = [];
  /** The node a trailing comment on the current line binds to. */
  let lastValue = null;

  let lineStart = 0;
  for (const rawLine of input.split("\n")) {
    const line = rawLine.replace(/\r$/, "");
    const at = lineStart;
    lineStart += rawLine.length + 1;
    lastValue = null;

    if (line.trim() === "") {
      block = null;
      continue;
    }
    const cut = line.indexOf("//");
    const content = cut === -1 ? line : line.slice(0, cut);
    const comment = cut === -1 ? null : line.slice(cut + 2).trim();
    const indent = content.length - content.trimStart().length;
    const body = content.trim();
    const bodyStart = at + indent;
    const bodyEnd = bodyStart + body.length;

    if (body !== "") {
      const voice = VOICE_LINE.exec(body);
      let owner;
      if (voice) {
        if (!blocks) {
          const key = scalar("string", bodyStart, bodyStart, "blocks");
          blocks = entry(key, container("sequence", bodyStart, bodyEnd), bodyStart, bodyEnd);
          root.children.push(blocks);
        }
        if (!block) {
          block = changed ?? newBlock(bodyStart, bodyEnd);
          changed = null;
          blocks.value.children.push(block);
        }
        const nameEnd = bodyStart + voice[1].length;
        const colon = nameEnd + voice[2].length;
        const bars = container("sequence", bodyEnd, bodyEnd);
        let segStart = colon + 1;
        const segments = body.slice(voice[0].length).split("|");
        if (segments.length > 1 && segments.at(-1).trim() === "") segments.pop();
        for (const seg of segments) {
          bars.children.push(parseBar(seg, segStart));
          segStart += seg.length + 1;
        }
        if (bars.children.length) {
          bars.span = [bars.children[0].span[0], bodyEnd];
        }
        const kv = entry(scalar("string", bodyStart, nameEnd, voice[1]), bars, bodyStart, bodyEnd);
        kv.sep = [colon, colon + 1];
        block.voices.push(kv);
        block.span[1] = bodyEnd;
        blocks.span[1] = blocks.value.span[1] = bodyEnd;
        owner = kv;
        lastValue = bars;
      } else {
        // Before the first block, the header; after it, a change for the next block.
        let t = header;
        if (blocks) {
          changed ??= newBlock(bodyStart, bodyEnd);
          t = changed.target;
        }
        const words = /^(\S+)\s+(.*)$/.exec(body);
        const word = words[1];
        const restStart = bodyStart + body.length - words[2].length;
        if (word === "octave") {
          const m = /^(\S+)(\s+)(\S+)$/.exec(words[2]);
          const nameStart = restStart;
          const valueStart = nameStart + m[1].length + m[2].length;
          if (!t.octaves) {
            t.octaves = entry(scalar("string", bodyStart, bodyStart + 6, "octave"), container("mapping", nameStart, bodyEnd), bodyStart, bodyEnd);
            t.push(t.octaves);
          }
          const value = scalar("int", valueStart, bodyEnd, m[3]);
          const kv = entry(scalar("string", nameStart, nameStart + m[1].length, m[1]), value, nameStart, bodyEnd);
          const existing = t.octaves.value.children.findIndex((c) => c.key.text === m[1]);
          if (existing === -1) t.octaves.value.children.push(kv);
          else t.octaves.value.children[existing].value = value;
          t.octaves.span[1] = t.octaves.value.span[1] = bodyEnd;
          owner = kv;
          lastValue = value;
        } else {
          const kind = word === "tempo" ? (/^\d+$/.test(words[2]) ? "int" : "float") : "string";
          const value = scalar(kind, restStart, bodyEnd, words[2]);
          const prev = t.settings.get(word);
          if (prev) {
            prev.value = value;
            owner = prev;
          } else {
            const kv = entry(scalar("string", bodyStart, bodyStart + word.length, word), value, bodyStart, bodyEnd);
            t.settings.set(word, kv);
            t.push(kv);
            owner = kv;
          }
          lastValue = value;
        }
        t.extend(bodyEnd);
      }
      owner.key.leading.push(...pending);
      pending = [];
    }

    if (comment !== null) {
      if (lastValue) lastValue.trailing = comment;
      else pending.push(comment);
    }
  }
  root.dangling = pending;
  // Voices before settings, and `blocks` before the header's settings: the
  // editor appends a new entry after a mapping's last one, and a setting
  // written after the music it precedes would not parse.
  for (const b of blocks?.value.children ?? []) b.children = [...b.voices, ...b.settings];
  if (blocks) root.children = [blocks, ...root.children.filter((c) => c !== blocks)];
  return root;
}

function byteLength(s) {
  return new TextEncoder().encode(s).length;
}

/** Character offset → byte offset, for every offset in `input`. */
function byteOffsets(input) {
  const out = new Array(input.length + 1);
  let b = 0;
  for (let i = 0; i < input.length; i++) {
    out[i] = b;
    const c = input.charCodeAt(i);
    if (c < 0x80) b += 1;
    else if (c < 0x800) b += 2;
    else if (c >= 0xd800 && c < 0xdc00) {
      out[++i] = b + 2; // the low surrogate is inside the 4-byte sequence
      b += 4;
    } else b += 3;
  }
  out[input.length] = b;
  return out;
}

function flatten(root, input) {
  const bytes = byteOffsets(input);
  const span = ([s, e]) => [bytes[s], bytes[e]];
  const rows = [];
  const comments = [];

  const visit = (node, parent) => {
    const id = rows.length;
    const row = { kind: node.kind, parent, span: span(node.span) };
    if (node.text !== undefined) row.text = node.text;
    if (node.sep) row.sep = span(node.sep);
    rows.push(row);
    for (const text of node.leading) comments.push({ node: id, slot: "leading", style: "line", text });
    if (node.trailing !== null) comments.push({ node: id, slot: "trailing", style: "line", text: node.trailing });
    if (node.kind === "keyvalue") {
      visit(node.key, id);
      visit(node.value, id);
    } else if (node.children) {
      for (const child of node.children) visit(child, id);
    }
  };
  visit(root, null);
  for (const text of root.dangling) comments.push({ node: 0, slot: "dangling", style: "line", text });
  comments.sort((a, b) => a.node - b.node);
  return { rows, comments };
}

function parse(_dialect, input) {
  validate(input);
  return flatten(parseTree(input), input);
}

// ── print ───────────────────────────────────────────────────────────────────

function print(_dialect, table, options) {
  const { rows } = table;
  const kids = rows.map(() => []);
  rows.forEach((r, i) => r.parent !== null && kids[r.parent].push(i));
  const comments = new Map();
  if (!options.strip_comments) {
    for (const c of table.comments ?? []) {
      if (!comments.has(c.node)) comments.set(c.node, []);
      comments.get(c.node).push(c);
    }
  }
  const leading = (id) => (comments.get(id) ?? []).filter((c) => c.slot === "leading").map((c) => `// ${c.text}`);
  const trailing = (id) => {
    const c = (comments.get(id) ?? []).find((c) => c.slot === "trailing");
    return c ? ` // ${c.text}` : "";
  };
  const fail = (message) => {
    throw new LanguageError(message);
  };

  const token = (id) => {
    const r = rows[id];
    if (r.kind === "sequence") return `[${kids[id].map(token).join(" ")}]`;
    if (r.kind === "mapping" || r.kind === "keyvalue") fail("a bar holds only tokens and [ ] groups");
    return r.text ?? "";
  };

  // A fragment the editor splices in: a scalar is its text, a sequence is a
  // `[ ]` group — which, standing in for a whole bar, times the same as the bar.
  const root = rows[0];
  if (root.kind !== "mapping") {
    if (root.kind === "keyvalue") fail("a tune can only hold a mapping at its root");
    return token(0);
  }

  const isDirective = (kv) => {
    const key = rows[kids[kv][0]].text;
    return SETTINGS.includes(key) || key === "octave";
  };

  /** Directive lines for a setting entry, at the root or in a block. */
  const directive = (kv) => {
    const [keyId, valueId] = kids[kv];
    const key = rows[keyId].text;
    const value = rows[valueId];
    if (key === "octave") {
      if (value.kind !== "mapping") fail(`"octave" must map voice names to octave numbers`);
      return [
        ...leading(keyId),
        ...kids[valueId].flatMap((okv) => {
          const [nameId, numId] = kids[okv];
          return [...leading(nameId), `octave ${rows[nameId].text} ${rows[numId].text}${trailing(numId)}`];
        }),
      ];
    }
    if (value.kind === "mapping" || value.kind === "sequence") fail(`"${key}" must be a single value`);
    return [...leading(keyId), `${key} ${value.text}${trailing(valueId)}`];
  };

  const header = [];
  const blocks = [];
  for (const kv of kids[0]) {
    const [keyId, valueId] = kids[kv];
    const key = rows[keyId].text;
    const value = rows[valueId];
    if (isDirective(kv)) {
      header.push(...directive(kv));
    } else if (key === "blocks") {
      if (value.kind !== "sequence") fail(`"blocks" must be a sequence of blocks`);
      for (const blockId of kids[valueId]) {
        if (rows[blockId].kind !== "mapping") fail("each block must map voice names to bars");
        const entries = kids[blockId];
        blocks.push({
          change: entries.filter(isDirective).flatMap(directive),
          voices: entries
            .filter((vkv) => !isDirective(vkv))
            .map((vkv) => {
              const [nameId, barsId] = kids[vkv];
              if (rows[barsId].kind !== "sequence") fail(`voice "${rows[nameId].text}" must be a sequence of bars`);
              const bars = kids[barsId].map((barId) =>
                rows[barId].kind === "sequence" ? kids[barId].map(token).join(" ") : token(barId),
              );
              return { leading: leading(nameId), name: rows[nameId].text, bars, trailing: trailing(barsId) };
            }),
        });
      }
    } else {
      fail(`a tune has no setting "${key}"`);
    }
  }

  // House style: line up the barlines of every voice in a block.
  const out = [...header];
  for (const { change, voices } of blocks) {
    if (out.length) out.push("");
    out.push(...change);
    const nameW = Math.max(...voices.map((v) => v.name.length));
    const barW = [];
    for (const v of voices) v.bars.forEach((b, i) => (barW[i] = Math.max(barW[i] ?? 0, b.length)));
    for (const v of voices) {
      const bars = v.bars.map((b, i) => b.padEnd(barW[i]));
      out.push(...v.leading, `${`${v.name}:`.padEnd(nameW + 1)} ${bars.join(" | ")} |${v.trailing}`);
    }
  }
  const dangling = (comments.get(0) ?? []).filter((c) => c.slot === "dangling").map((c) => `// ${c.text}`);
  if (dangling.length) out.push("", ...dangling);
  return out.join("\n") + "\n";
}

// ── render ──────────────────────────────────────────────────────────────────

/** A new entry: a setting, an octave, or a voice in a block. */
function render(which, args) {
  if (which !== "entry") throw new Error(`no renderer \`${which}\``);
  const { key, value } = args;
  if (args.parent_key === "octave") return `octave ${key} ${value}`;
  if (args.parent_key === "" && (SETTINGS.includes(key) || key === "octave")) return `${key} ${value}`;
  return `${key}: ${value}`;
}

// ── the language ────────────────────────────────────────────────────────────

/** @type {import("@diaryx/fig/helper").Language} */
export const tune = {
  name: "tune",
  caps: { read: true, edit: true, serialize: true },
  max_mapping_depth: null,
  syntax: {
    comments: { style: "slashes", line: { open: "//" }, trailing: { open: "//" } },
    kv_sep: " ",
    empty_map_literal: null,
    flow_containers: false,
    block_seq_editable: false,
  },
  dialects: [{ name: "tune", extensions: ["tune"], splice: "raw", empty_doc_seed: null }],
  samples: [
    "key C major\ntempo 100\ntime 4/4\n\nRH: 3 2 1 2 | 3 3 3 - |\nLH: I | V |\n",
    "// a waltz\nkey F major\ntime 3/4\noctave LH 2 // low\n\nRH: 5 - 3 | [4 3] 2 1 |\nLH: I | V7 |\n\nRH: 1 - - |\n",
    "RH: 1 2 3 4 |\n\n// up a fifth\nkey G major\ntempo 90\noctave RH 5\nRH: 1 - - - |\n",
  ],
  renderers: ["entry"],
  parse,
  print,
  render,
};

if (import.meta.main) await serve(tune);
