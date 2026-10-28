//! Musical edits as token rewrites. Each operation returns the tokens to
//! replace, by fig path; [`crate::splice`] applies them with fig's editor.
//!
//! A token's pitch depends only on itself, the key, and its voice's octave
//! (SPEC §4.2), so an edit rewrites one token and never moves another.

use std::collections::{BTreeMap, HashSet};

use crate::path;
use crate::splice::{Path, Seg, TokenEdit};
use crate::theory::{
    Chord, NUMERALS, count_marks, degree_at, index_of, marks_text, modulo, read_chord, read_note,
    tonic_of,
};
use crate::types::{SlotInfo, SlotKind, Song};

/// Where a slot is in the tree: `blocks → block → voice → bar → path…`.
pub fn slot_path(s: &SlotInfo) -> Path {
    let mut p = path!["blocks", s.block, &s.voice, s.bar];
    p.extend(s.path.iter().map(|&i| Seg::Index(i)));
    p
}

fn bar_path(s: &SlotInfo) -> Path {
    path!["blocks", s.block, &s.voice, s.bar]
}

/// The note, stack, or chord a slot sounds as part of: itself, or the one its holds continue.
pub fn onset_of(song: &Song, index: usize) -> Option<usize> {
    let voice = &song.slots[index].voice;
    for i in (0..=index).rev() {
        let s = &song.slots[i];
        if &s.voice != voice {
            return None;
        }
        if s.sounding() {
            return Some(i);
        }
        if s.kind != SlotKind::Hold {
            return None;
        }
    }
    None
}

/// The slot sounding from `start` in `voice`: for finding a note after an
/// edit that may have rewritten its path.
pub fn onset_at(song: &Song, voice: &str, start: i64) -> Option<usize> {
    song.slots
        .iter()
        .position(|s| s.voice == voice && (s.start - start).abs() <= 1 && s.sounding())
}

/// The token with the octave marks on its first note or chord root replaced.
fn with_marks(text: &str, marks: i64) -> String {
    if let Some(n) = read_note(text) {
        return format!("{}{}{}", n.acc, n.degree, marks_text(marks));
    }
    if let Some(c) = read_chord(text) {
        return Chord {
            marks: marks_text(marks),
            ..c
        }
        .write();
    }
    let mut members: Vec<String> = text.split('+').map(str::to_string).collect();
    members[0] = with_marks(&members[0], marks);
    members.join("+")
}

/// The diatonic triad (or seventh chord) on `degree` in this scale, as a numeral.
fn diatonic_chord(scale: &[i32; 7], degree: i64, seventh: bool) -> String {
    let d = degree as usize;
    let above = |k: usize| modulo((scale[(d + k) % 7] - scale[d]) as i64, 12);
    let mut numeral = NUMERALS[d].to_string();
    let mut quality = "";
    if above(2) == 4 {
        numeral = numeral.to_uppercase();
        if above(4) == 8 {
            quality = "+";
        }
    } else if above(4) == 6 {
        quality = "o";
    }
    if !seventh {
        return numeral + quality;
    }
    match quality {
        "o" => numeral + if above(6) == 10 { "h7" } else { "o7" },
        "+" => numeral + "+7",
        _ => numeral + if above(6) == 11 { "maj7" } else { "7" },
    }
}

fn rewrite(song: &Song, index: usize, text: String) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if text == s.text {
        vec![]
    } else {
        vec![TokenEdit {
            path: slot_path(s),
            text,
        }]
    }
}

fn scale_of(song: &Song, index: usize) -> [i32; 7] {
    song.blocks
        .get(song.slots[index].block)
        .map_or(song.scale, |b| b.scale)
}

/// Moves a note, stack, or chord by `steps` scale steps (7 = an octave).
pub fn move_by(song: &Song, index: usize, steps: i64) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !s.sounding() || !s.editable {
        return vec![];
    }
    let octave_only = steps % 7 == 0;
    let text = match s.kind {
        SlotKind::Note => {
            let acc = if octave_only {
                read_note(&s.text).map_or("", |n| n.acc)
            } else {
                ""
            };
            format!("{acc}{}", degree_at(index_of(&s.text) + steps))
        }
        SlotKind::Chord => {
            let target = index_of(&s.text) + steps;
            let seventh = read_chord(&s.text).is_some_and(|c| !c.seventh.is_empty());
            if octave_only {
                with_marks(&s.text, target.div_euclid(7))
            } else {
                diatonic_chord(&scale_of(song, index), modulo(target, 7), seventh)
                    + &marks_text(target.div_euclid(7))
            }
        }
        // A stack keeps its shape: every member moves by the same number of steps.
        _ => s
            .text
            .split('+')
            .map(|member| {
                let acc = if octave_only {
                    read_note(member).map_or("", |n| n.acc)
                } else {
                    ""
                };
                format!("{acc}{}", degree_at(index_of(member) + steps))
            })
            .collect::<Vec<_>>()
            .join("+"),
    };
    rewrite(song, index, text)
}

/// Replaces the token with a plain scale degree (0-based), keeping a note or chord's octave marks.
pub fn set_degree(song: &Song, index: usize, degree: i64) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !s.editable {
        return vec![];
    }
    let octave = if s.sounding() {
        index_of(s.text.split('+').next().unwrap_or("")).div_euclid(7)
    } else {
        0
    };
    rewrite(song, index, degree_at(degree + 7 * octave))
}

pub fn to_rest(song: &Song, index: usize) -> Vec<TokenEdit> {
    if song.slots[index].editable {
        rewrite(song, index, ".".into())
    } else {
        vec![]
    }
}

pub fn to_hold(song: &Song, index: usize) -> Vec<TokenEdit> {
    if song.slots[index].editable {
        rewrite(song, index, "-".into())
    } else {
        vec![]
    }
}

/// Splits a slot into `parts` equal parts, each sounding what it sounded.
pub fn split(song: &Song, index: usize, parts: usize) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !s.editable {
        return vec![];
    }
    vec![TokenEdit {
        path: slot_path(s),
        text: format!("[{}]", vec![s.text.as_str(); parts].join(" ")),
    }]
}

/// Toggles a sharp or flat on a single note, or on a chord's root.
pub fn toggle_accidental(song: &Song, index: usize, acc: &str) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !s.editable {
        return vec![];
    }
    let flip = |a: &str| {
        if a == acc {
            String::new()
        } else {
            acc.to_string()
        }
    };
    let text = match s.kind {
        SlotKind::Note => {
            read_note(&s.text).map(|n| format!("{}{}{}", flip(n.acc), n.degree, n.marks))
        }
        SlotKind::Chord => read_chord(&s.text).map(|c| {
            Chord {
                acc: flip(&c.acc),
                ..c
            }
            .write()
        }),
        _ => None,
    };
    match text {
        Some(text) => vec![TokenEdit {
            path: slot_path(s),
            text,
        }],
        None => vec![],
    }
}

/// Writes a note of pitch `midi` in the slot: the scale degree that sounds
/// it, else the one a semitone below, sharpened. For a note, rest, or hold.
pub fn set_pitch(song: &Song, index: usize, midi: i32) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !s.editable || matches!(s.kind, SlotKind::Chord | SlotKind::Stack) {
        return vec![];
    }
    let block = song.blocks.get(s.block);
    let scale = block.map_or(song.scale, |b| b.scale);
    let octave = block.map_or_else(|| crate::default_octave(&s.voice), |b| b.octave(&s.voice));
    let key = block.map_or(song.key.as_str(), |b| b.key.as_str());
    let rel = midi as i64 - (12 * (octave + 1) + tonic_of(key) as i64);
    let pc = modulo(rel, 12) as i32;
    let natural = scale.iter().position(|&x| x == pc);
    let Some(degree) = natural.or_else(|| scale.iter().position(|&x| x == (pc - 1).rem_euclid(12)))
    else {
        return vec![];
    };
    let sharp = if natural.is_some() { "" } else { "#" };
    rewrite(
        song,
        index,
        format!(
            "{sharp}{}",
            degree_at(degree as i64 + 7 * rel.div_euclid(12))
        ),
    )
}

// ── harmony ─────────────────────────────────────────────────────────────────

/// Turns a note (or a stack, by its first member) into the diatonic triad on
/// its degree, a triad into its diatonic seventh chord, and a seventh chord
/// back into a note on its root. The octave marks stay.
pub fn cycle_chord(song: &Song, index: usize) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !s.sounding() || !s.editable {
        return vec![];
    }
    let scale = scale_of(song, index);
    if s.kind == SlotKind::Chord
        && let Some(c) = read_chord(&s.text)
    {
        if c.seventh.is_empty() {
            let full = read_chord(&diatonic_chord(&scale, c.root(), true)).unwrap();
            let text = Chord {
                numeral: full.numeral,
                quality: full.quality,
                seventh: full.seventh,
                ..c
            }
            .write();
            return rewrite(song, index, text);
        }
        return rewrite(song, index, format!("{}{}{}", c.acc, c.root() + 1, c.marks));
    }
    let first = s.text.split('+').next().unwrap_or("");
    let idx = index_of(first);
    let acc = read_note(first).map_or("", |n| n.acc);
    rewrite(
        song,
        index,
        format!(
            "{acc}{}{}",
            diatonic_chord(&scale, modulo(idx, 7), false),
            marks_text(idx.div_euclid(7))
        ),
    )
}

/// Chord qualities in the order `cycle_quality` steps through them: (numeral upper case, quality, seventh).
const TRIADS: [(bool, &str, &str); 4] = [
    (true, "", ""),
    (false, "", ""),
    (false, "o", ""),
    (true, "+", ""),
];
const SEVENTHS: [(bool, &str, &str); 5] = [
    (true, "", "7"),
    (true, "", "maj7"),
    (false, "", "7"),
    (false, "h", "7"),
    (false, "o", "7"),
];

/// Steps a chord to its next quality: major, minor, diminished, augmented
/// (with a seventh: 7, maj7, m7, ø7, o7).
pub fn cycle_quality(song: &Song, index: usize) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    let Some(c) = (s.kind == SlotKind::Chord && s.editable)
        .then(|| read_chord(&s.text))
        .flatten()
    else {
        return vec![];
    };
    let list: &[(bool, &str, &str)] = if c.seventh.is_empty() {
        &TRIADS
    } else {
        &SEVENTHS
    };
    let upper = c.upper();
    let at = list
        .iter()
        .position(|&(u, q, sev)| u == upper && q == c.quality && sev == c.seventh);
    let (u, quality, seventh) = list[at.map_or(0, |a| (a + 1) % list.len())];
    let numeral = if u {
        c.numeral.to_uppercase()
    } else {
        c.numeral.to_lowercase()
    };
    let text = Chord {
        numeral,
        quality: quality.into(),
        seventh: seventh.into(),
        ..c
    }
    .write();
    rewrite(song, index, text)
}

/// Steps a chord's bass through its inversions: root position, then its
/// third, fifth, and seventh in the bass, spelled with whatever accidental
/// the chord tone needs in this scale.
pub fn cycle_inversion(song: &Song, index: usize) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    let Some(c) = (s.kind == SlotKind::Chord && s.editable)
        .then(|| read_chord(&s.text))
        .flatten()
    else {
        return vec![];
    };
    let scale = scale_of(song, index);
    let root = c.root();
    let root_pc = scale[root as usize] as i64 + crate::theory::accidental(&c.acc) as i64;
    let mut basses: Vec<(String, String)> = vec![(String::new(), String::new())];
    for (i, semis) in c.tones().iter().skip(1).enumerate() {
        let degree = modulo(root + 2 * (i as i64 + 1), 7);
        let off = modulo(
            root_pc + *semis as i64 - scale[degree as usize] as i64 + 6,
            12,
        ) - 6;
        let acc = match off {
            1 => "#",
            -1 => "b",
            _ => "",
        };
        basses.push((acc.into(), (degree + 1).to_string()));
    }
    let at = basses
        .iter()
        .position(|(a, d)| *a == c.bass_acc && *d == c.bass_deg);
    let (bass_acc, bass_deg) = basses[at.map_or(0, |a| (a + 1) % basses.len())].clone();
    rewrite(
        song,
        index,
        Chord {
            bass_acc,
            bass_deg,
            ..c
        }
        .write(),
    )
}

/// Adds the scale degree a third above the top of a note or stack, making (or growing) a stack.
pub fn add_third(song: &Song, index: usize) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !matches!(s.kind, SlotKind::Note | SlotKind::Stack) || !s.editable {
        return vec![];
    }
    let mut members: Vec<String> = s.text.split('+').map(str::to_string).collect();
    let top = members.iter().map(|m| index_of(m)).max().unwrap_or(0);
    members.push(degree_at(top + 2));
    rewrite(song, index, members.join("+"))
}

// ── rhythm ──────────────────────────────────────────────────────────────────

/// A bar as written: a token, or a group of nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Tok(String),
    Group(Vec<Node>),
}

fn tok(s: &str) -> Node {
    Node::Tok(s.to_string())
}

fn serialize(n: &Node) -> String {
    match n {
        Node::Tok(t) => t.clone(),
        Node::Group(g) => format!(
            "[{}]",
            g.iter().map(serialize).collect::<Vec<_>>().join(" ")
        ),
    }
}

fn place(root: &mut Vec<Node>, path: &[usize], value: Node) {
    let (last, init) = path.split_last().unwrap();
    let mut at = root;
    for &i in init {
        while at.len() <= i {
            at.push(Node::Group(vec![]));
        }
        at = match &mut at[i] {
            Node::Group(g) => g,
            n => {
                *n = Node::Group(vec![]);
                let Node::Group(g) = n else { unreachable!() };
                g
            }
        };
    }
    while at.len() <= *last {
        at.push(tok(""));
    }
    at[*last] = value;
}

/// The group (or bar) holding the item at `path`.
fn holder<'a>(bar: &'a mut Vec<Node>, path: &[usize]) -> &'a mut Vec<Node> {
    let mut at = bar;
    for &i in &path[..path.len() - 1] {
        at = match &mut at[i] {
            Node::Group(g) => g,
            Node::Tok(_) => unreachable!("a path runs through groups"),
        };
    }
    at
}

/// The bar a slot is in, as written.
fn bar_of(song: &Song, s: &SlotInfo) -> Vec<Node> {
    let mut bar = vec![];
    for t in &song.slots {
        if t.voice == s.voice && t.block == s.block && t.bar == s.bar {
            place(&mut bar, &t.path, tok(&t.text));
        }
    }
    bar
}

/// Rewrites the bar a slot is in through `f`, which gets the bar as written
/// and the slot's path in it.
fn rewrite_bar(
    song: &Song,
    index: usize,
    f: impl FnOnce(&mut Vec<Node>, &[usize]),
) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    if !s.editable {
        return vec![];
    }
    let old = bar_of(song, s);
    let mut next = old.clone();
    f(&mut next, &s.path);
    let mut edits = vec![];
    let path = bar_path(s);
    // A bar is not a group: one that changes length is written without brackets.
    if old.len() == next.len() {
        for (i, (o, n)) in old.iter().zip(&next).enumerate() {
            let mut p = path.clone();
            p.push(Seg::Index(i));
            diff(o, n, p, &mut edits);
        }
    } else {
        edits.push(TokenEdit {
            path,
            text: next.iter().map(serialize).collect::<Vec<_>>().join(" "),
        });
    }
    edits
}

/// Puts a rest after the slot, in its group or bar, which re-divides to make room.
pub fn insert_slot(song: &Song, index: usize) -> Vec<TokenEdit> {
    rewrite_bar(song, index, |bar, path| {
        holder(bar, path).insert(path[path.len() - 1] + 1, tok("."));
    })
}

/// Takes the slot out of its group or bar, which re-divides among the rest.
pub fn delete_slot(song: &Song, index: usize) -> Vec<TokenEdit> {
    rewrite_bar(song, index, |bar, path| {
        let n = path.len();
        let group = holder(bar, path);
        group.remove(path[n - 1]);
        if n == 1 {
            if group.is_empty() {
                group.push(tok("."));
            }
            return;
        }
        let mut items = std::mem::take(group);
        let replacement = match items.len() {
            0 => tok("."),
            1 => items.remove(0),
            _ => Node::Group(items),
        };
        holder(bar, &path[..n - 1])[path[n - 2]] = replacement;
    })
}

/// The first token written in a node.
fn first_token(n: &Node) -> Node {
    match n {
        Node::Tok(_) => n.clone(),
        Node::Group(g) => first_token(&g[0]),
    }
}

/// Undoes a split: the group the slot is in becomes one slot, sounding what the group started with.
pub fn join_group(song: &Song, index: usize) -> Vec<TokenEdit> {
    if song.slots[index].path.len() < 2 {
        return vec![];
    }
    rewrite_bar(song, index, |bar, path| {
        let at = &path[..path.len() - 1];
        let first = first_token(&Node::Group(holder(bar, path).clone()));
        holder(bar, at)[at[at.len() - 1]] = first;
    })
}

/// Re-divides the slot's whole bar into `n` equal slots. Each takes the
/// first note that starts within it, else holds what sounds at its start,
/// else rests: what fits the new grid sounds as before.
pub fn divide_bar(song: &Song, index: usize, n: usize) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    let slots: Vec<&SlotInfo> = song
        .slots
        .iter()
        .filter(|t| t.voice == s.voice && t.block == s.block && t.bar == s.bar)
        .collect();
    if n < 1 || slots.iter().any(|t| !t.editable) {
        return vec![];
    }
    let start = slots[0].start as f64;
    let len = slots[slots.len() - 1].end as f64 - start;
    let tokens: Vec<String> = (0..n)
        .map(|i| {
            let from = start + len * i as f64 / n as f64;
            let to = start + len * (i + 1) as f64 / n as f64;
            if let Some(onset) = slots.iter().find(|t| {
                t.sounding() && t.start as f64 >= from - 0.5 && (t.start as f64) < to - 0.5
            }) {
                return onset.text.clone();
            }
            let held = song.events.iter().any(|e| {
                e.voice == s.voice && e.start as f64 <= from + 0.5 && from + 0.5 < e.end() as f64
            });
            if held { "-".into() } else { ".".into() }
        })
        .collect();
    let text = tokens.join(" ");
    let current = slots
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if text == current && slots.iter().all(|t| t.path.len() == 1) {
        return vec![];
    }
    vec![TokenEdit {
        path: bar_path(s),
        text,
    }]
}

// ── resizing ────────────────────────────────────────────────────────────────
//
// A note lasts its own slot plus the holds after it, so its length is
// changed by rewriting slots, never by moving barlines: the slots it grows
// over become `-`, the holds it gives up become `.`, and where the new end
// falls inside a slot, that slot is split. Groups the edit touches are then
// written in their coarsest form, so `[- -]` reads `-` and `[1 - . .]` reads
// `[1 .]`.

/// Where a resized note ends: `k`/`n` of the way through slot `slot` (0 < n, 0 ≤ k ≤ n).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cut {
    pub slot: usize,
    pub k: usize,
    pub n: usize,
}

/// `Math.round`: halves up.
fn round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

/// The index of the last hold continuing the note at `index` (itself if none).
fn hold_end(song: &Song, index: usize) -> usize {
    let voice = &song.slots[index].voice;
    let mut e = index;
    while let Some(t) = song.slots.get(e + 1) {
        if &t.voice != voice || t.kind != SlotKind::Hold {
            break;
        }
        e += 1;
    }
    e
}

/// Whether slot `m` is an editable slot in `voice`.
fn editable_in(song: &Song, voice: &str, m: usize) -> bool {
    song.slots
        .get(m)
        .is_some_and(|s| s.voice == voice && s.editable)
}

/// The cut nearest `tick` for the note at `index`: one of `parts(slot)`
/// equal points through the slot under `tick`.
pub fn snap_cut(
    song: &Song,
    index: usize,
    tick: f64,
    parts: impl Fn(&SlotInfo) -> usize,
) -> Option<Cut> {
    let s = &song.slots[index];
    if !s.sounding() || !s.editable {
        return None;
    }
    let mut m = index;
    while (song.slots[m].end as f64) <= tick && editable_in(song, &s.voice, m + 1) {
        m += 1;
    }
    let slot = &song.slots[m];
    let n = parts(slot).max(1);
    let frac = (tick - slot.start as f64) / (slot.end - slot.start) as f64;
    let lo = if m == index { 1 } else { 0 };
    let k = round(frac * n as f64).max(lo).min(n as i64) as usize;
    Some(Cut { slot: m, k, n })
}

/// Where the note at `index` ends now, as a cut.
pub fn current_cut(song: &Song, index: usize) -> Cut {
    Cut {
        slot: hold_end(song, index),
        k: 1,
        n: 1,
    }
}

/// Grows the note at `index` over the next slot, or shrinks it by its last hold (by half if it has none).
pub fn lengthen(song: &Song, index: usize, dir: i32) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    let e = hold_end(song, index);
    if dir > 0 {
        return if editable_in(song, &s.voice, e + 1) {
            resize(
                song,
                index,
                Cut {
                    slot: e + 1,
                    k: 1,
                    n: 1,
                },
            )
        } else {
            vec![]
        };
    }
    let cut = if e > index {
        Cut {
            slot: e,
            k: 0,
            n: 1,
        }
    } else {
        Cut {
            slot: index,
            k: 1,
            n: 2,
        }
    };
    resize(song, index, cut)
}

fn fill(text: &str, count: usize) -> Vec<Node> {
    vec![tok(text); count]
}

/// Makes the note at `index` end at `cut`. Growing over another note cuts
/// it short: what is left of it, if anything, is struck again where this one
/// ends.
pub fn resize(song: &Song, index: usize, cut: Cut) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    let Cut { slot: j, k, n } = cut;
    if !s.sounding() || !s.editable || j < index || (j == index && k == 0) {
        return vec![];
    }
    if (index..=j).any(|m| !editable_in(song, &s.voice, m)) {
        return vec![];
    }

    let e = hold_end(song, index);
    let mut rep: BTreeMap<usize, Node> = BTreeMap::new();
    // The token sounding where the note now ends: the last one it grew over.
    let mut onset: Option<String> = None;

    let mut m = index;
    while m <= e.max(j) + 1 && editable_in(song, &s.voice, m) {
        let t = &song.slots[m];
        let held = m <= e;
        if m < j || (m == j && k == n) {
            if !held {
                if t.kind != SlotKind::Hold {
                    onset = Some(t.text.clone());
                }
                rep.insert(m, tok("-"));
            }
            m += 1;
            continue;
        }
        let struck = if t.kind == SlotKind::Hold {
            onset.clone()
        } else {
            Some(t.text.clone())
        };
        let rest = |count: usize| -> Vec<Node> {
            if held {
                fill(".", count)
            } else {
                let mut v = vec![tok(struck.as_deref().unwrap_or("-"))];
                v.extend(fill("-", count.saturating_sub(1)));
                v
            }
        };
        if m > j || k == 0 {
            if held {
                rep.insert(m, tok("."));
            } else {
                if t.kind == SlotKind::Hold
                    && let Some(o) = &onset
                {
                    rep.insert(m, tok(o));
                }
                break;
            }
            m += 1;
            continue;
        }
        let mut group = if m == index {
            let mut h = vec![tok(&t.text)];
            h.extend(fill("-", k - 1));
            h
        } else {
            fill("-", k)
        };
        group.extend(rest(n - k));
        rep.insert(m, Node::Group(group));
        if !held {
            break;
        }
        m += 1;
    }
    bar_edits(song, &rep)
}

/// The tick a cut falls at.
pub fn cut_tick(song: &Song, cut: Cut) -> f64 {
    let s = &song.slots[cut.slot];
    s.start as f64 + (s.end - s.start) as f64 * cut.k as f64 / cut.n as f64
}

/// The cut nearest `tick` for a new start of the note at `index`: anywhere
/// from the voice's first editable slot before it to just short of where it
/// ends.
pub fn snap_start(
    song: &Song,
    index: usize,
    tick: f64,
    parts: impl Fn(&SlotInfo) -> usize,
) -> Option<Cut> {
    let s = &song.slots[index];
    if !s.sounding() || !s.editable {
        return None;
    }
    let e = hold_end(song, index);
    let mut m = index;
    while (song.slots[m].start as f64) > tick && m > 0 && editable_in(song, &s.voice, m - 1) {
        m -= 1;
    }
    while (song.slots[m].end as f64) <= tick && m < e {
        m += 1;
    }
    let slot = &song.slots[m];
    let n = parts(slot).max(1);
    let frac = (tick - slot.start as f64) / (slot.end - slot.start) as f64;
    let hi = if m == e { n - 1 } else { n };
    let k = round(frac * n as f64).max(0).min(hi as i64) as usize;
    Some(Cut { slot: m, k, n })
}

/// Makes the note at `index` start at `cut`, keeping where it ends.
/// Starting earlier cuts short whatever sounds before; starting later leaves
/// a rest.
pub fn restart(song: &Song, index: usize, cut: Cut) -> Vec<TokenEdit> {
    let s = &song.slots[index];
    let Cut {
        slot: mut j,
        mut k,
        mut n,
    } = cut;
    if k == n {
        (j, k, n) = (j + 1, 0, 1);
    }
    let e = hold_end(song, index);
    if !s.sounding() || !s.editable || j > e || (j == index && k == 0) {
        return vec![];
    }
    if (j.min(index)..=j.max(index)).any(|m| !editable_in(song, &s.voice, m)) {
        return vec![];
    }

    let mut rep: BTreeMap<usize, Node> = BTreeMap::new();
    let mut tail = vec![tok(&s.text)];
    tail.extend(fill("-", n - k - 1));
    if j < index {
        for m in j + 1..=index {
            rep.insert(m, tok("-"));
        }
        let t = &song.slots[j];
        let node = if k == 0 {
            tok(&s.text)
        } else {
            let mut g = vec![tok(&t.text)];
            g.extend(fill("-", k - 1));
            g.extend(tail);
            Node::Group(g)
        };
        rep.insert(j, node);
    } else {
        for m in index..j {
            rep.insert(m, tok("."));
        }
        let node = if k == 0 {
            tok(&s.text)
        } else {
            let mut g = fill(".", k);
            g.extend(tail);
            Node::Group(g)
        };
        rep.insert(j, node);
    }
    bar_edits(song, &rep)
}

/// Token edits for slot replacements, with each group they touch reduced.
fn bar_edits(song: &Song, rep: &BTreeMap<usize, Node>) -> Vec<TokenEdit> {
    let bar_key = |t: &SlotInfo| (t.voice.clone(), t.block, t.bar);
    let mut bars: Vec<((String, usize, usize), Vec<usize>)> = vec![];
    for &m in rep.keys() {
        let key = bar_key(&song.slots[m]);
        if !bars.iter().any(|(k, _)| *k == key) {
            bars.push((key, vec![]));
        }
    }
    for (m, t) in song.slots.iter().enumerate() {
        let key = bar_key(t);
        if let Some((_, members)) = bars.iter_mut().find(|(k, _)| *k == key) {
            members.push(m);
        }
    }

    let mut edits = vec![];
    for (_, members) in bars {
        let first = &song.slots[members[0]];
        let mut old = vec![];
        let mut next = vec![];
        let mut touched: HashSet<Vec<usize>> = HashSet::new();
        for &m in &members {
            let t = &song.slots[m];
            place(&mut old, &t.path, tok(&t.text));
            place(
                &mut next,
                &t.path,
                rep.get(&m).cloned().unwrap_or_else(|| tok(&t.text)),
            );
            if rep.contains_key(&m) {
                for d in 1..=t.path.len() {
                    touched.insert(t.path[..d].to_vec());
                }
            }
        }
        let reduced: Vec<Node> = next
            .into_iter()
            .enumerate()
            .map(|(i, child)| reduce_touched(child, vec![i], &touched))
            .collect();
        diff(
            &Node::Group(old),
            &Node::Group(reduced),
            bar_path(first),
            &mut edits,
        );
    }
    edits
}

/// Reduces, bottom up, every group on the way to a replaced slot.
fn reduce_touched(node: Node, path: Vec<usize>, touched: &HashSet<Vec<usize>>) -> Node {
    if !touched.contains(&path) {
        return node;
    }
    match node {
        Node::Tok(_) => node,
        Node::Group(g) => reduce(
            g.into_iter()
                .enumerate()
                .map(|(i, c)| {
                    let mut p = path.clone();
                    p.push(i);
                    reduce_touched(c, p, touched)
                })
                .collect(),
        ),
    }
}

/// A group in its coarsest equivalent form: split into as few equal runs as
/// possible where each run is one token held (`1 -` → `1`) or a rest (`. .` → `.`).
fn reduce(group: Vec<Node>) -> Node {
    let tokens: Option<Vec<&str>> = group
        .iter()
        .map(|c| match c {
            Node::Tok(t) => Some(t.as_str()),
            Node::Group(_) => None,
        })
        .collect();
    let Some(tokens) = tokens else {
        return Node::Group(group);
    };
    let n = tokens.len();
    for m in 1..n {
        if n % m != 0 {
            continue;
        }
        let r = n / m;
        let mut runs: Vec<&str> = vec![];
        for run in tokens.chunks(r) {
            let tail = &run[1..];
            if tail.iter().all(|t| *t == "-") {
                runs.push(run[0]);
            } else if run[0] == "." && tail.iter().all(|t| *t == "." || *t == "-") {
                runs.push(".");
            } else {
                break;
            }
        }
        if runs.len() == m {
            return if m == 1 {
                tok(runs[0])
            } else {
                Node::Group(runs.into_iter().map(tok).collect())
            };
        }
    }
    Node::Group(group)
}

/// Edits turning `old` into `next`, each at the outermost node whose shape changed.
fn diff(old: &Node, next: &Node, path: Path, out: &mut Vec<TokenEdit>) {
    if let (Node::Group(a), Node::Group(b)) = (old, next)
        && a.len() == b.len()
    {
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            let mut p = path.clone();
            p.push(Seg::Index(i));
            diff(x, y, p, out);
        }
    } else if serialize(old) != serialize(next) {
        out.push(TokenEdit {
            path,
            text: serialize(next),
        });
    }
}

/// Marks a token's octave count, for a test's sake.
#[doc(hidden)]
pub fn octave_marks(text: &str) -> i64 {
    count_marks(text)
}
