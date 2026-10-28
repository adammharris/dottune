//! The tune language for fig: how a `.tune` file reads as a tree, how a
//! tree prints as one, and how a new entry is spelled. Registered with fig
//! in-process ([`format`]) for the editor's splices, and served to the `fig`
//! CLI by the `fig-tune` helper.
//!
//! The tree a tune reads as:
//!
//! ```text
//! key: "C major"            ← directives
//! tempo: 100
//! time: "4/4"
//! octave: { LH: 2 }         ← every `octave <voice> <n>` line, merged
//! blocks:                   ← synthetic key: no text of its own
//!   - RH: [[3, 2, 1, 2], [3, 3, 3, "-"]]   ← block → voice → bars → tokens
//!     LH: [[I], [V]]
//!   - RH: [[5]]
//!     tempo: 90             ← a change between blocks belongs to the block after it
//! ```
//!
//! Tokens are strings; a `[ ]` group is a nested sequence. In a block,
//! voices come before settings in the tree, whatever the order in the text,
//! and `blocks` comes before the header's settings: the editor appends a new
//! entry after a mapping's last one, and a setting written after the music
//! it precedes would not parse.
//!
//! Editing is the generic splice engine plus one renderer, `entry`, which
//! spells a new setting or octave. The sequences say how their items are
//! joined, so each takes item edits — insert, append, delete, move: a
//! voice's bars (` | `, with no delimiters of their own), a bar's tokens and
//! a group's (` `), and the blocks (a blank line apart). fig reparses after
//! every edit, rolling back anything that fails. It refuses a block edit
//! that would move a change's directives into the header, and a new voice
//! line; `set` does not create the `octave` mapping when the file has none.

use std::collections::HashMap;
use std::sync::OnceLock;

use fig::language::{
    CommentForm, CommentRow, CommentSlot, Comments, Description, Dialect, Language, LanguageError,
    NodeKind, NodeRow, NodeTable, PrintOptions, RenderArgs, Renderer, Renderers, Splice, Syntax,
};
use fig::{Capabilities, Format, Span};

use crate::compile::{compile, split_voice_line, strip_comment};

const SETTINGS: [&str; 3] = ["key", "tempo", "time"];

const SAMPLES: [&str; 3] = [
    "key C major\ntempo 100\ntime 4/4\n\nRH: 3 2 1 2 | 3 3 3 - |\nLH: I | V |\n",
    "// a waltz\nkey F major\ntime 3/4\noctave LH 2 // low\n\nRH: 5 - 3 | [4 3] 2 1 |\nLH: I | V7 |\n\nRH: 1 - - |\n",
    "RH: 1 2 3 4 |\n\n// up a fifth\nkey G major\ntempo 90\noctave RH 5\nRH: 1 - - - |\n",
];

/// The tune language, registered with fig once per process.
pub fn format() -> Format {
    static FORMAT: OnceLock<Format> = OnceLock::new();
    *FORMAT.get_or_init(|| {
        fig::language::register(Tune)
            .expect("the tune language registers with fig")
            .remove(0)
    })
}

pub struct Tune;

// ── parse ───────────────────────────────────────────────────────────────────
//
// Builds a tree with spans, then flattens it into rows. A tree first
// because `octave` lines merge into one mapping that other directives may
// sit between, and rows must be in pre-order.

#[derive(Clone, Debug)]
struct Node {
    kind: NodeKind,
    span: (usize, usize),
    text: Option<String>,
    children: Vec<usize>,
    /// A keyvalue's key and value.
    kv: Option<(usize, usize)>,
    sep: Option<(usize, usize)>,
    item_sep: Option<&'static str>,
    open: bool,
    leading: Vec<String>,
    trailing: Option<String>,
}

/// Where directive lines go: the root (the header), or a block (a change before it).
#[derive(Default)]
struct Target {
    settings: HashMap<String, usize>,
    octaves: Option<usize>,
}

/// A block's voices and settings, kept apart so voices can go first.
#[derive(Default)]
struct BlockParts {
    node: usize,
    voices: Vec<usize>,
    settings: Vec<usize>,
    target: Target,
}

struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    fn add(&mut self, n: Node) -> usize {
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    fn scalar(&mut self, kind: NodeKind, start: usize, end: usize, text: &str) -> usize {
        self.add(Node {
            kind,
            span: (start, end),
            text: Some(text.to_string()),
            children: vec![],
            kv: None,
            sep: None,
            item_sep: None,
            open: false,
            leading: vec![],
            trailing: None,
        })
    }

    fn container(&mut self, kind: NodeKind, start: usize, end: usize) -> usize {
        self.add(Node {
            kind,
            span: (start, end),
            text: None,
            children: vec![],
            kv: None,
            sep: None,
            item_sep: None,
            open: false,
            leading: vec![],
            trailing: None,
        })
    }

    fn entry(&mut self, key: usize, value: usize, start: usize, end: usize) -> usize {
        let id = self.container(NodeKind::KeyValue, start, end);
        self.nodes[id].kv = Some((key, value));
        id
    }

    fn sequence(&mut self, start: usize, end: usize, sep: &'static str, open: bool) -> usize {
        let id = self.container(NodeKind::Sequence, start, end);
        self.nodes[id].item_sep = Some(sep);
        self.nodes[id].open = open;
        id
    }

    /// One bar's text → a sequence of tokens and groups. `base` is the bar's offset in the input.
    fn bar(&mut self, text: &str, base: usize) -> usize {
        let start = base + (text.len() - text.trim_start().len());
        let end = base + text.trim_end().len();
        let bar = self.sequence(start, end, " ", true);
        let mut stack = vec![bar];
        let mut i = 0;
        while i < text.len() {
            let c = text[i..].chars().next().unwrap();
            let at = base + i;
            if c.is_whitespace() {
                i += c.len_utf8();
            } else if c == '[' {
                let group = self.sequence(at, at + 1, " ", false);
                let top = *stack.last().unwrap();
                self.nodes[top].children.push(group);
                stack.push(group);
                i += 1;
            } else if c == ']' {
                let group = stack.pop().unwrap();
                self.nodes[group].span.1 = at + 1;
                i += 1;
            } else {
                let from = i;
                while i < text.len() {
                    let c = text[i..].chars().next().unwrap();
                    if c.is_whitespace() || c == '[' || c == ']' {
                        break;
                    }
                    i += c.len_utf8();
                }
                let tok = self.scalar(NodeKind::String, base + from, base + i, &text[from..i]);
                let top = *stack.last().unwrap();
                self.nodes[top].children.push(tok);
            }
        }
        bar
    }

    fn key_of(&self, kv: usize) -> usize {
        self.nodes[kv].kv.unwrap().0
    }
}

fn parse_tree(input: &str) -> (Tree, Vec<String>) {
    let mut t = Tree { nodes: vec![] };
    let root = t.container(NodeKind::Mapping, 0, input.len());
    let mut header = Target::default();
    let mut blocks_kv: Option<usize> = None;
    let mut parts: Vec<BlockParts> = vec![];
    // Whether the last block in `parts` is open for more voice lines.
    let mut in_block = false;
    // A block opened by a change, waiting for its first voice line.
    let mut changed: Option<BlockParts> = None;
    let mut pending: Vec<String> = vec![];

    let mut line_start = 0;
    for raw_line in input.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let at = line_start;
        line_start += raw_line.len() + 1;
        // The node a trailing comment on this line binds to.
        let mut last_value: Option<usize> = None;

        if line.trim().is_empty() {
            in_block = false;
            continue;
        }
        let content = strip_comment(line);
        let comment =
            (content.len() < line.len()).then(|| line[content.len() + 2..].trim().to_string());
        let indent = content.len() - content.trim_start().len();
        let body = content.trim();
        let body_start = at + indent;
        let body_end = body_start + body.len();

        if !body.is_empty() {
            let owner;
            if let Some((name, bars_text)) = split_voice_line(body) {
                let blocks = *blocks_kv.get_or_insert_with(|| {
                    let key = t.scalar(NodeKind::String, body_start, body_start, "blocks");
                    let seq = t.sequence(body_start, body_end, "\n\n", true);
                    let kv = t.entry(key, seq, body_start, body_end);
                    t.nodes[root].children.push(kv);
                    kv
                });
                let blocks_seq = t.nodes[blocks].kv.unwrap().1;
                if !in_block {
                    let block = changed.take().unwrap_or_else(|| BlockParts {
                        node: t.container(NodeKind::Mapping, body_start, body_end),
                        ..Default::default()
                    });
                    t.nodes[blocks_seq].children.push(block.node);
                    parts.push(block);
                    in_block = true;
                }
                let name_end = body_start + name.len();
                let colon = body_start + body.find(':').unwrap();
                let bars = t.sequence(body_end, body_end, " | ", true);
                let mut seg_start = colon + 1;
                let mut segments: Vec<&str> = bars_text.split('|').collect();
                if segments.len() > 1 && segments.last().unwrap().trim().is_empty() {
                    segments.pop();
                }
                for seg in segments {
                    let bar = t.bar(seg, seg_start);
                    t.nodes[bars].children.push(bar);
                    seg_start += seg.len() + 1;
                }
                if let Some(&first) = t.nodes[bars].children.first() {
                    t.nodes[bars].span = (t.nodes[first].span.0, body_end);
                }
                let key = t.scalar(NodeKind::String, body_start, name_end, name);
                let kv = t.entry(key, bars, body_start, body_end);
                t.nodes[kv].sep = Some((colon, colon + 1));
                let block = parts.last_mut().unwrap();
                block.voices.push(kv);
                t.nodes[block.node].span.1 = body_end;
                t.nodes[blocks].span.1 = body_end;
                t.nodes[blocks_seq].span.1 = body_end;
                owner = kv;
                last_value = Some(bars);
            } else {
                // Before the first block, the header; after it, a change for the next block.
                let (word, rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
                let rest = rest.trim_start();
                let rest_start = body_start + body.len() - rest.len();
                let in_change = blocks_kv.is_some();
                if in_change && changed.is_none() {
                    changed = Some(BlockParts {
                        node: t.container(NodeKind::Mapping, body_start, body_end),
                        ..Default::default()
                    });
                }
                let (target, block) = match changed.as_mut() {
                    Some(b) if in_change => (&mut b.target, Some((&mut b.settings, b.node))),
                    _ => (&mut header, None),
                };
                // The entries this line adds to its header or change.
                let mut pushed: Vec<usize> = vec![];
                if word == "octave" {
                    let (name, value_text) =
                        rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                    let value_text = value_text.trim_start();
                    let name_start = rest_start;
                    let value_start = body_end - value_text.len();
                    let octaves = match target.octaves {
                        Some(o) => o,
                        None => {
                            let key =
                                t.scalar(NodeKind::String, body_start, body_start + 6, "octave");
                            let map = t.container(NodeKind::Mapping, name_start, body_end);
                            let kv = t.entry(key, map, body_start, body_end);
                            target.octaves = Some(kv);
                            pushed.push(kv);
                            kv
                        }
                    };
                    let map = t.nodes[octaves].kv.unwrap().1;
                    let value = t.scalar(NodeKind::Int, value_start, body_end, value_text);
                    let existing = t.nodes[map]
                        .children
                        .iter()
                        .copied()
                        .find(|&c| t.nodes[t.key_of(c)].text.as_deref() == Some(name));
                    let kv = match existing {
                        Some(kv) => {
                            t.nodes[kv].kv.as_mut().unwrap().1 = value;
                            kv
                        }
                        None => {
                            let key = t.scalar(
                                NodeKind::String,
                                name_start,
                                name_start + name.len(),
                                name,
                            );
                            let kv = t.entry(key, value, name_start, body_end);
                            t.nodes[map].children.push(kv);
                            kv
                        }
                    };
                    t.nodes[octaves].span.1 = body_end;
                    t.nodes[map].span.1 = body_end;
                    owner = kv;
                    last_value = Some(value);
                } else {
                    let kind = match word {
                        "tempo" if rest.bytes().all(|b| b.is_ascii_digit()) => NodeKind::Int,
                        "tempo" => NodeKind::Float,
                        _ => NodeKind::String,
                    };
                    let value = t.scalar(kind, rest_start, body_end, rest);
                    owner = match target.settings.get(word) {
                        Some(&prev) => {
                            t.nodes[prev].kv.as_mut().unwrap().1 = value;
                            prev
                        }
                        None => {
                            let key = t.scalar(
                                NodeKind::String,
                                body_start,
                                body_start + word.len(),
                                word,
                            );
                            let kv = t.entry(key, value, body_start, body_end);
                            target.settings.insert(word.to_string(), kv);
                            pushed.push(kv);
                            kv
                        }
                    };
                    last_value = Some(value);
                }
                match block {
                    Some((settings, node)) => {
                        settings.extend(pushed);
                        t.nodes[node].span.1 = body_end;
                    }
                    None => t.nodes[root].children.extend(pushed),
                }
            }
            let key = t.key_of(owner);
            t.nodes[key].leading.append(&mut pending);
        }

        if let Some(c) = comment {
            match last_value {
                Some(v) => t.nodes[v].trailing = Some(c),
                None => pending.push(c),
            }
        }
    }
    for b in &parts {
        t.nodes[b.node].children = b.voices.iter().chain(&b.settings).copied().collect();
    }
    if let Some(blocks) = blocks_kv {
        let rest: Vec<usize> = t.nodes[root]
            .children
            .iter()
            .copied()
            .filter(|&c| c != blocks)
            .collect();
        t.nodes[root].children = std::iter::once(blocks).chain(rest).collect();
    }
    (t, pending)
}

fn flatten(t: &Tree, dangling: Vec<String>) -> NodeTable {
    let mut table = NodeTable::new();
    fn visit(t: &Tree, id: usize, parent: Option<u32>, table: &mut NodeTable) {
        let n = &t.nodes[id];
        let mut row = NodeRow::new(n.kind, parent, Span::from(n.span.0..n.span.1));
        if let Some(text) = &n.text {
            row = row.with_text(text);
        }
        if let Some(sep) = n.sep {
            row = row.with_sep(Span::from(sep.0..sep.1));
        }
        if let Some(sep) = n.item_sep {
            row = row.with_item_sep(sep, n.open);
        }
        let row_id = table.push(row);
        for text in &n.leading {
            table.comments.push(CommentRow::new(
                row_id,
                CommentSlot::Leading,
                CommentForm::Line,
                text.clone(),
            ));
        }
        if let Some(text) = &n.trailing {
            table.comments.push(CommentRow::new(
                row_id,
                CommentSlot::Trailing,
                CommentForm::Line,
                text.clone(),
            ));
        }
        if let Some((k, v)) = n.kv {
            visit(t, k, Some(row_id), table);
            visit(t, v, Some(row_id), table);
        } else {
            for &c in &n.children {
                visit(t, c, Some(row_id), table);
            }
        }
    }
    visit(t, 0, None, &mut table);
    for text in dangling {
        table.comments.push(CommentRow::new(
            0,
            CommentSlot::Dangling,
            CommentForm::Line,
            text,
        ));
    }
    table.comments.sort_by_key(|c| c.node);
    table
}

// ── print ───────────────────────────────────────────────────────────────────

struct Printer<'a> {
    rows: &'a [NodeRow],
    kids: Vec<Vec<usize>>,
    comments: HashMap<u32, Vec<&'a CommentRow>>,
}

impl Printer<'_> {
    fn text(&self, id: usize) -> &str {
        self.rows[id].text.as_deref().unwrap_or("")
    }

    fn leading(&self, id: usize) -> Vec<String> {
        self.comments
            .get(&(id as u32))
            .into_iter()
            .flatten()
            .filter(|c| c.slot == CommentSlot::Leading)
            .map(|c| format!("// {}", c.text))
            .collect()
    }

    fn trailing(&self, id: usize) -> String {
        self.comments
            .get(&(id as u32))
            .into_iter()
            .flatten()
            .find(|c| c.slot == CommentSlot::Trailing)
            .map_or(String::new(), |c| format!(" // {}", c.text))
    }

    fn token(&self, id: usize) -> Result<String, LanguageError> {
        match self.rows[id].kind {
            NodeKind::Sequence => {
                let inner: Result<Vec<String>, _> =
                    self.kids[id].iter().map(|&k| self.token(k)).collect();
                Ok(format!("[{}]", inner?.join(" ")))
            }
            NodeKind::Mapping | NodeKind::KeyValue => {
                Err(LanguageError::new("a bar holds only tokens and [ ] groups"))
            }
            _ => Ok(self.text(id).to_string()),
        }
    }

    fn is_directive(&self, kv: usize) -> bool {
        let key = self.text(self.kids[kv][0]);
        SETTINGS.contains(&key) || key == "octave"
    }

    /// Directive lines for a setting entry, at the root or in a block.
    fn directive(&self, kv: usize) -> Result<Vec<String>, LanguageError> {
        let (key_id, value_id) = (self.kids[kv][0], self.kids[kv][1]);
        let key = self.text(key_id);
        let value = &self.rows[value_id];
        let mut out = self.leading(key_id);
        if key == "octave" {
            if value.kind != NodeKind::Mapping {
                return Err(LanguageError::new(
                    "\"octave\" must map voice names to octave numbers",
                ));
            }
            for &okv in &self.kids[value_id] {
                let (name_id, num_id) = (self.kids[okv][0], self.kids[okv][1]);
                out.extend(self.leading(name_id));
                out.push(format!(
                    "octave {} {}{}",
                    self.text(name_id),
                    self.text(num_id),
                    self.trailing(num_id)
                ));
            }
            return Ok(out);
        }
        if matches!(value.kind, NodeKind::Mapping | NodeKind::Sequence) {
            return Err(LanguageError::new(format!(
                "\"{key}\" must be a single value"
            )));
        }
        out.push(format!(
            "{key} {}{}",
            self.text(value_id),
            self.trailing(value_id)
        ));
        Ok(out)
    }
}

struct PrintedVoice {
    leading: Vec<String>,
    name: String,
    bars: Vec<String>,
    trailing: String,
}

fn print(table: &NodeTable, options: &PrintOptions) -> Result<Vec<u8>, LanguageError> {
    let rows = &table.rows;
    let mut kids = vec![vec![]; rows.len()];
    for (i, r) in rows.iter().enumerate() {
        if let Some(p) = r.parent {
            kids[p as usize].push(i);
        }
    }
    let mut comments: HashMap<u32, Vec<&CommentRow>> = HashMap::new();
    if !options.strip_comments {
        for c in &table.comments {
            comments.entry(c.node).or_default().push(c);
        }
    }
    let p = Printer {
        rows,
        kids,
        comments,
    };

    // A fragment the editor splices in: a scalar is its text, a sequence is
    // a `[ ]` group — which, standing in for a whole bar, times the same as
    // the bar.
    let Some(root) = rows.first() else {
        return Ok(vec![]);
    };
    if root.kind != NodeKind::Mapping {
        if root.kind == NodeKind::KeyValue {
            return Err(LanguageError::new(
                "a tune can only hold a mapping at its root",
            ));
        }
        return Ok(p.token(0)?.into_bytes());
    }

    let mut header: Vec<String> = vec![];
    let mut blocks: Vec<(Vec<String>, Vec<PrintedVoice>)> = vec![];
    for &kv in &p.kids[0] {
        let (key_id, value_id) = (p.kids[kv][0], p.kids[kv][1]);
        let key = p.text(key_id);
        if p.is_directive(kv) {
            header.extend(p.directive(kv)?);
        } else if key == "blocks" {
            if rows[value_id].kind != NodeKind::Sequence {
                return Err(LanguageError::new(
                    "\"blocks\" must be a sequence of blocks",
                ));
            }
            for &block_id in &p.kids[value_id] {
                if rows[block_id].kind != NodeKind::Mapping {
                    return Err(LanguageError::new(
                        "each block must map voice names to bars",
                    ));
                }
                let mut change = vec![];
                let mut voices = vec![];
                for &entry in &p.kids[block_id] {
                    if p.is_directive(entry) {
                        change.extend(p.directive(entry)?);
                        continue;
                    }
                    let (name_id, bars_id) = (p.kids[entry][0], p.kids[entry][1]);
                    if rows[bars_id].kind != NodeKind::Sequence {
                        return Err(LanguageError::new(format!(
                            "voice \"{}\" must be a sequence of bars",
                            p.text(name_id)
                        )));
                    }
                    let mut bars = vec![];
                    for &bar_id in &p.kids[bars_id] {
                        bars.push(if rows[bar_id].kind == NodeKind::Sequence {
                            let toks: Result<Vec<String>, _> =
                                p.kids[bar_id].iter().map(|&k| p.token(k)).collect();
                            toks?.join(" ")
                        } else {
                            p.token(bar_id)?
                        });
                    }
                    voices.push(PrintedVoice {
                        leading: p.leading(name_id),
                        name: p.text(name_id).to_string(),
                        bars,
                        trailing: p.trailing(bars_id),
                    });
                }
                blocks.push((change, voices));
            }
        } else {
            return Err(LanguageError::new(format!(
                "a tune has no setting \"{key}\""
            )));
        }
    }

    // House style: line up the barlines of every voice in a block.
    let width = |s: &str| s.chars().count();
    let pad = |s: &str, w: usize| format!("{s}{}", " ".repeat(w.saturating_sub(width(s))));
    let mut out = header;
    for (change, voices) in blocks {
        if !out.is_empty() {
            out.push(String::new());
        }
        out.extend(change);
        let name_w = voices.iter().map(|v| width(&v.name)).max().unwrap_or(0);
        let mut bar_w: Vec<usize> = vec![];
        for v in &voices {
            for (i, b) in v.bars.iter().enumerate() {
                if bar_w.len() <= i {
                    bar_w.push(0);
                }
                bar_w[i] = bar_w[i].max(width(b));
            }
        }
        for v in voices {
            let bars: Vec<String> = v
                .bars
                .iter()
                .enumerate()
                .map(|(i, b)| pad(b, bar_w[i]))
                .collect();
            out.extend(v.leading);
            out.push(format!(
                "{} {} |{}",
                pad(&format!("{}:", v.name), name_w + 1),
                bars.join(" | "),
                v.trailing
            ));
        }
    }
    let dangling: Vec<String> = p
        .comments
        .get(&0)
        .into_iter()
        .flatten()
        .filter(|c| c.slot == CommentSlot::Dangling)
        .map(|c| format!("// {}", c.text))
        .collect();
    if !dangling.is_empty() {
        out.push(String::new());
        out.extend(dangling);
    }
    Ok((out.join("\n") + "\n").into_bytes())
}

// ── the language ────────────────────────────────────────────────────────────

impl Language for Tune {
    fn describe(&self) -> Description {
        let mut d = Description::new("tune");
        d.caps = Capabilities::new(true, true, true);
        d.max_mapping_depth = None;
        let mut syntax = Syntax::default();
        syntax.comments = Comments::slashes();
        syntax.kv_sep = Some(" ".into());
        syntax.empty_map_literal = None;
        syntax.flow_containers = false;
        syntax.block_seq_editable = false;
        d.syntax = Some(syntax);
        let mut dialect = Dialect::new("tune");
        dialect.extensions = vec!["tune".into()];
        dialect.splice = Splice::Raw;
        dialect.empty_doc_seed = None;
        d.dialects = vec![dialect];
        d.samples = SAMPLES.iter().map(|s| s.to_string()).collect();
        let mut renderers = Renderers::default();
        renderers.entry = true;
        d.renderers = renderers;
        d
    }

    fn parse(&self, _dialect: &str, input: &[u8]) -> Result<NodeTable, LanguageError> {
        let input = std::str::from_utf8(input)
            .map_err(|e| LanguageError::at("not UTF-8", e.valid_up_to()))?;
        if let Err(errors) = compile(input) {
            let e = errors.first();
            let offset = match e.span {
                Some((start, _)) => start,
                None => input
                    .split('\n')
                    .take(e.line.saturating_sub(1))
                    .map(|l| l.len() + 1)
                    .sum(),
            };
            return Err(LanguageError::at(e.to_string(), offset));
        }
        let (tree, dangling) = parse_tree(input);
        Ok(flatten(&tree, dangling))
    }

    fn print(
        &self,
        _dialect: &str,
        table: &NodeTable,
        options: &PrintOptions,
    ) -> Result<Vec<u8>, LanguageError> {
        print(table, options)
    }

    fn render(&self, which: Renderer, args: RenderArgs<'_>) -> Result<Vec<u8>, LanguageError> {
        if which != Renderer::Entry {
            return Err(LanguageError::new(format!(
                "no renderer `{}`",
                which.name()
            )));
        }
        let key = String::from_utf8_lossy(args.key);
        let value = String::from_utf8_lossy(args.value);
        let parent = args.parent_key;
        let text = if parent == b"octave" {
            format!("octave {key} {value}")
        } else if parent.is_empty() && (SETTINGS.contains(&key.as_ref()) || key == "octave") {
            format!("{key} {value}")
        } else {
            format!("{key}: {value}")
        };
        Ok(text.into_bytes())
    }
}
