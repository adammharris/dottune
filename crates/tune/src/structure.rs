//! Structural edits: bars, blocks, voices, and the directives that set them.
//! Each takes the source and the song compiled from it and returns the new
//! source, or an error — a tune error, or fig's refusal — when it cannot be
//! done.
//!
//! Bars, voice names, and directives go through fig, whose tune language
//! edits a voice's bars item by item. Blocks, new voice lines, and the order
//! of voices are edited as lines: fig refuses a block edit that changes
//! which directives a change holds (deleting the first block hands the next
//! one's change to the header), has no spelling for a new voice line, and
//! traps reordering the voices of a block that has a change. After a block
//! moves or goes, `keep_settings` writes the directives that make every
//! other block sound as it did.

use fig::Value;

use crate::compile::{DIRECTIVES, compile, is_voice_name, strip_comment};
use crate::path;
use crate::splice::{EditError, Path, Seg, apply};
use crate::types::{BlockInfo, Song, Time};

type Result<T> = std::result::Result<T, EditError>;

/// Compiles `source`, so an edit that breaks the tune is refused.
fn checked(source: String) -> Result<String> {
    compile(&source)?;
    Ok(source)
}

/// Where a block's settings sit in fig's tree: the header's at the root, a change's in its block.
fn settings_path(b: usize) -> Path {
    if b == 0 { vec![] } else { path!["blocks", b] }
}

fn voice_path(b: usize, voice: &str) -> Path {
    path!["blocks", b, voice]
}

/// A new bar: a rest on every beat, ready to be filled in.
fn rest_bar(time: Time) -> String {
    vec!["."; time.0 as usize].join(" ")
}

/// An error unless `name` can name a voice.
pub fn check_name(name: &str) -> Result<()> {
    if !is_voice_name(name) {
        return Err(format!(
            "\"{name}\" is not a voice name: start with a letter, then letters, digits, or _"
        )
        .into());
    }
    if DIRECTIVES.contains(&name) {
        return Err(format!("\"{name}\" is a directive and cannot name a voice").into());
    }
    Ok(())
}

/// A directive's value as the text form writes it: `C major`, `100`, `92.5`.
fn value_text(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        other => format!("{other:?}"),
    }
}

/// A number as a value: an int if it is whole.
pub fn number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Value::Int(n as i64)
    } else {
        Value::Float(n)
    }
}

// ── lines ───────────────────────────────────────────────────────────────────

/// A voice line taken apart: `head` is up to the colon, `segs` the text between barlines.
#[derive(Clone, Debug)]
struct VoiceLine {
    name: String,
    head: String,
    segs: Vec<String>,
    /// Whether the line ends with a barline.
    closed: bool,
    /// Whatever follows the last bar: spaces and a comment.
    tail: String,
}

fn read_voice_line(line: &str) -> Option<VoiceLine> {
    let content = strip_comment(line).trim_end();
    let lead = content.len() - content.trim_start().len();
    let (name, after) = crate::compile::split_voice_line(&content[lead..])?;
    if DIRECTIVES.contains(&name) {
        return None;
    }
    let head_len = content.len() - after.len();
    let mut segs: Vec<String> = after.split('|').map(str::to_string).collect();
    let closed = segs.len() > 1 && segs.last().unwrap().trim().is_empty();
    if closed {
        segs.pop();
    }
    Some(VoiceLine {
        name: name.to_string(),
        head: content[..head_len].to_string(),
        segs,
        closed,
        tail: line[content.len()..].to_string(),
    })
}

fn write_voice_line(v: &VoiceLine) -> String {
    format!(
        "{}{}{}{}",
        v.head,
        v.segs.join("|"),
        if v.closed { "|" } else { "" },
        v.tail
    )
}

fn lines_of(source: &str) -> Vec<String> {
    source.split('\n').map(str::to_string).collect()
}

/// The lines `[from, to)` of block `b`: its voice lines and the comments
/// among them, and for any block but the first, the change above it.
fn region(lines: &[String], song: &Song, b: usize) -> (usize, usize) {
    let first = song.blocks[b].line - 1;
    let mut from = first;
    if b > 0 {
        while from > 0 && !lines[from - 1].trim().is_empty() {
            from -= 1;
        }
    }
    let mut to = first;
    while to < lines.len() && !lines[to].trim().is_empty() {
        to += 1;
    }
    (from, to)
}

/// The line holding `voice` in block `b`.
fn voice_line_at(lines: &[String], song: &Song, b: usize, voice: &str) -> Result<usize> {
    let (from, to) = region(lines, song, b);
    (from..to)
        .find(|&i| read_voice_line(&lines[i]).is_some_and(|v| v.name == voice))
        .ok_or_else(|| format!("{voice} has no line in block {}", b + 1).into())
}

/// Whether `line` is a directive line.
fn is_directive_line(line: &str) -> bool {
    let text = strip_comment(line).trim();
    DIRECTIVES.iter().any(|d| {
        text.strip_prefix(d)
            .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

// ── directives ──────────────────────────────────────────────────────────────

/// Sets a directive for block `b`: in the header for the first block, in
/// the change before any other. fig edits or adds it in place; where fig
/// cannot — a header or change with nothing to add after, or no `octave`
/// mapping to add to — the directive line is written as text instead.
pub fn set_directive(
    source: &str,
    song: &Song,
    b: usize,
    path: &[Seg],
    value: Value,
) -> Result<String> {
    let mut full = settings_path(b);
    full.extend(path.iter().cloned());
    if let Ok(Ok(s)) = apply(source, |ed| ed.set(&full, value.clone())).map(checked) {
        return Ok(s);
    }
    let words: Vec<String> = path
        .iter()
        .map(|s| match s {
            Seg::Key(k) => k.clone(),
            Seg::Index(i) => i.to_string(),
        })
        .collect();
    let line = format!("{} {}", words.join(" "), value_text(&value));
    let mut lines = lines_of(source);
    let at = if b == 0 {
        // After the header's last directive, before the first voice.
        let first_voice = song.blocks.first().map_or(lines.len(), |b| b.line - 1);
        (0..first_voice)
            .filter(|&i| is_directive_line(&lines[i]))
            .map(|i| i + 1)
            .next_back()
            .unwrap_or(0)
    } else {
        // Just above the block's first voice, after any change already there.
        song.blocks[b].line - 1
    };
    lines.insert(at, line);
    checked(lines.join("\n"))
}

/// Removes a directive written for block `b` (`"tempo"`, `"octave RH"`…),
/// so the block takes the one before it.
pub fn clear_directive(source: &str, song: &Song, b: usize, directive: &str) -> Result<String> {
    let info = &song.blocks[b];
    if !info.directives.iter().any(|d| d == directive) {
        return Ok(source.to_string());
    }
    let mut words = directive.split(' ');
    let word = words.next().unwrap_or("");
    let voice = words.next();
    let mut path = settings_path(b);
    path.push(Seg::from(word));
    // The last `octave` line of a header or change goes with the mapping that holds it.
    if let Some(v) = voice
        && info
            .directives
            .iter()
            .filter(|d| d.starts_with("octave "))
            .count()
            > 1
    {
        path.push(Seg::from(v));
    }
    checked(apply(source, |ed| ed.delete_key(&path))?)
}

/// Writes directives until each block sounds with the settings it had
/// before an edit: new block `i` was block `map[i]` of `before` (`None`: a
/// new block).
fn keep_settings(before: &Song, mut source: String, map: &[Option<usize>]) -> Result<String> {
    for (i, was) in map.iter().enumerate() {
        let Some(was) = *was else { continue };
        let want = &before.blocks[was];
        let have = |s: &str| -> Result<BlockInfo> { Ok(compile(s)?.blocks[i].clone()) };
        let fix = |s: String, path: Path, value: Value| -> Result<String> {
            let song = compile(&s)?;
            set_directive(&s, &song, i, &path, value)
        };
        if have(&source)?.key != want.key {
            source = fix(source, path!["key"], Value::Str(want.key.clone()))?;
        }
        if have(&source)?.tempo != want.tempo {
            source = fix(source, path!["tempo"], number(want.tempo))?;
        }
        if have(&source)?.time != want.time {
            source = fix(
                source,
                path!["time"],
                Value::Str(format!("{}/{}", want.time.0, want.time.1)),
            )?;
        }
        for v in have(&source)?.voices {
            if have(&source)?.octave(&v) != want.octave(&v) {
                source = fix(source, path!["octave", &v], Value::Int(want.octave(&v)))?;
            }
        }
    }
    checked(source)
}

// ── bars ────────────────────────────────────────────────────────────────────
//
// A bar edit applies to every voice of a block at once, so the voices stay
// lined up. A voice shorter than the block is padded with rests (SPEC §1.1),
// so its missing bars are only written where a later bar needs them.

/// Inserts a bar of rests at index `k` of block `b` (`k` = the block's length appends one).
pub fn insert_bar(source: &str, song: &Song, b: usize, k: usize) -> Result<String> {
    let info = &song.blocks[b];
    let n = info.len();
    let text = rest_bar(info.time);
    checked(apply(source, |ed| {
        for (v, &count) in info.voices.iter().zip(&info.bars) {
            if count > k || (k == n && count == n) {
                ed.insert_item(&voice_path(b, v), k, &text)?;
            }
        }
        Ok(())
    })?)
}

/// Deletes bar `k` of block `b` from every voice; a voice left with no bars
/// loses its line, a block left empty goes.
pub fn delete_bar(source: &str, song: &Song, b: usize, k: usize) -> Result<String> {
    let info = &song.blocks[b];
    if info.len() <= 1 {
        return delete_block(source, song, b);
    }
    checked(apply(source, |ed| {
        for (v, &count) in info.voices.iter().zip(&info.bars) {
            if count <= k {
                continue;
            }
            if count == 1 {
                ed.delete_key(&voice_path(b, v))?;
            } else {
                ed.delete_item(&voice_path(b, v), k)?;
            }
        }
        Ok(())
    })?)
}

/// Copies bar `k` of block `b` into a new bar just after it.
pub fn duplicate_bar(source: &str, song: &Song, b: usize, k: usize) -> Result<String> {
    let info = &song.blocks[b];
    let lines = lines_of(source);
    let mut copies = vec![];
    for (v, &count) in info.voices.iter().zip(&info.bars) {
        if count <= k {
            continue;
        }
        let line = read_voice_line(&lines[voice_line_at(&lines, song, b, v)?]).unwrap();
        copies.push((v.clone(), line.segs[k].trim().to_string()));
    }
    checked(apply(source, |ed| {
        for (v, bar) in &copies {
            ed.insert_item(&voice_path(b, v), k + 1, bar)?;
        }
        Ok(())
    })?)
}

/// Swaps bar `k` of block `b` with its neighbour (`dir` −1 before, 1 after).
pub fn move_bar(source: &str, song: &Song, b: usize, k: usize, dir: i32) -> Result<String> {
    let info = &song.blocks[b];
    let j = k as i64 + dir as i64;
    if j < 0 || j >= info.len() as i64 {
        return Ok(source.to_string());
    }
    let j = j as usize;
    let (lo, hi) = (k.min(j), k.max(j));
    checked(apply(source, |ed| {
        for (v, &count) in info.voices.iter().zip(&info.bars) {
            if count > hi {
                ed.move_item(&voice_path(b, v), k, j)?;
            } else if count == hi {
                // Bar `hi` is padding: write the rest it becomes in front of bar `lo`.
                ed.insert_item(&voice_path(b, v), lo, ".")?;
            }
        }
        Ok(())
    })?)
}

// ── blocks ──────────────────────────────────────────────────────────────────

/// Adds a block of one bar of rests for the same voices after block
/// `after`, or, in a tune with no blocks, a first block for `RH`.
pub fn insert_block(source: &str, song: &Song, after: usize) -> Result<String> {
    if song.blocks.is_empty() {
        let body = source.trim_end();
        let sep = if body.is_empty() { "" } else { "\n\n" };
        return checked(format!("{body}{sep}RH: {} |\n", rest_bar(song.time)));
    }
    let mut lines = lines_of(source);
    let info = &song.blocks[after];
    let (_, to) = region(&lines, song, after);
    let mut new = vec![String::new()];
    new.extend(
        info.voices
            .iter()
            .map(|v| format!("{v}: {} |", rest_bar(info.time))),
    );
    lines.splice(to..to, new);
    checked(lines.join("\n"))
}

/// Deletes block `b`; the blocks after it keep their settings.
pub fn delete_block(source: &str, song: &Song, b: usize) -> Result<String> {
    let mut lines = lines_of(source);
    let (mut from, mut to) = region(&lines, song, b);
    // Take the blank lines that separated it, before it (or, for the first block, after it).
    if b > 0 {
        while from > 0 && lines[from - 1].trim().is_empty() {
            from -= 1;
        }
    } else {
        while to < lines.len() && lines[to].trim().is_empty() {
            to += 1;
        }
    }
    lines.drain(from..to);
    let map: Vec<Option<usize>> = (0..song.blocks.len())
        .filter(|&i| i != b)
        .map(Some)
        .collect();
    keep_settings(song, lines.join("\n"), &map)
}

/// Swaps block `b` with its neighbour (`dir` −1 before, 1 after); every block keeps its settings.
pub fn move_block(source: &str, song: &Song, b: usize, dir: i32) -> Result<String> {
    let other = b as i64 + dir as i64;
    if other < 0 || other >= song.blocks.len() as i64 {
        return Ok(source.to_string());
    }
    let (a, c) = (b.min(other as usize), b.max(other as usize));
    let lines = lines_of(source);
    let (fa, ta) = region(&lines, song, a);
    let (fc, tc) = region(&lines, song, c);
    let next: Vec<String> = [
        &lines[..fa],
        &lines[fc..tc],
        &lines[ta..fc],
        &lines[fa..ta],
        &lines[tc..],
    ]
    .concat();
    let map: Vec<Option<usize>> = (0..song.blocks.len())
        .map(|i| {
            Some(if i == a {
                c
            } else if i == c {
                a
            } else {
                i
            })
        })
        .collect();
    keep_settings(song, next.join("\n"), &map)
}

/// Copies block `b`, change and all, into a new block just after it.
pub fn duplicate_block(source: &str, song: &Song, b: usize) -> Result<String> {
    let mut lines = lines_of(source);
    let (from, to) = region(&lines, song, b);
    let mut copy = vec![String::new()];
    copy.extend(lines[from..to].iter().cloned());
    lines.splice(to..to, copy);
    let mut map: Vec<Option<usize>> = (0..song.blocks.len()).map(Some).collect();
    map.insert(b + 1, Some(b));
    keep_settings(song, lines.join("\n"), &map)
}

/// Splits block `b` before bar `k`, into two blocks that play just as one did.
pub fn split_block(source: &str, song: &Song, b: usize, k: usize) -> Result<String> {
    if k == 0 || k >= song.blocks[b].len() {
        return Ok(source.to_string());
    }
    let mut lines = lines_of(source);
    let (from, to) = region(&lines, song, b);
    let mut first = vec![];
    let mut second = vec![];
    for line in &lines[from..to] {
        match read_voice_line(line) {
            Some(v) if v.segs.len() > k => {
                first.push(write_voice_line(&VoiceLine {
                    segs: v.segs[..k].to_vec(),
                    closed: true,
                    ..v.clone()
                }));
                second.push(write_voice_line(&VoiceLine {
                    segs: v.segs[k..].to_vec(),
                    tail: String::new(),
                    ..v
                }));
            }
            _ => first.push(line.clone()),
        }
    }
    first.push(String::new());
    first.extend(second);
    lines.splice(from..to, first);
    checked(lines.join("\n"))
}

/// Joins block `b` and the one after it into one block. The second may not start with a change.
pub fn join_blocks(source: &str, song: &Song, b: usize) -> Result<String> {
    let Some(next) = song.blocks.get(b + 1) else {
        return Ok(source.to_string());
    };
    if !next.directives.is_empty() {
        return Err(format!(
            "block {} starts with a change ({}); clear it first",
            b + 2,
            next.directives.join(", ")
        )
        .into());
    }
    let n = song.blocks[b].len();
    let mut lines = lines_of(source);
    let (fa, ta) = region(&lines, song, b);
    let (fb, tb) = region(&lines, song, b + 1);
    let later: Vec<String> = lines[fb..tb].to_vec();
    let mut later_voices: Vec<VoiceLine> =
        later.iter().filter_map(|l| read_voice_line(l)).collect();

    // Bars a voice gets in front of its bars from the second block: rests up to the first one's length.
    let pad = |count: usize| vec![" . ".to_string(); n - count];
    let mut joined: Vec<String> = lines[fa..ta]
        .iter()
        .map(|line| {
            let Some(v) = read_voice_line(line) else {
                return line.clone();
            };
            let Some(at) = later_voices.iter().position(|w| w.name == v.name) else {
                return line.clone();
            };
            let w = later_voices.remove(at);
            let mut segs = v.segs.clone();
            if !v.closed && !segs.last().unwrap().ends_with(char::is_whitespace) {
                segs.last_mut().unwrap().push(' ');
            }
            segs.extend(pad(v.segs.len()));
            segs.extend(w.segs);
            let tail = if v.tail.trim().is_empty() {
                w.tail
            } else {
                v.tail.clone()
            };
            write_voice_line(&VoiceLine {
                segs,
                closed: w.closed,
                tail,
                ..v
            })
        })
        .collect();
    for w in later_voices {
        let mut segs = pad(0);
        segs.extend(w.segs.clone());
        joined.push(write_voice_line(&VoiceLine { segs, ..w }));
    }
    // The second block's comments follow the joined lines.
    joined.extend(
        later
            .iter()
            .filter(|l| read_voice_line(l).is_none())
            .cloned(),
    );
    lines.splice(fa..tb, joined);
    checked(lines.join("\n"))
}

// ── voices ──────────────────────────────────────────────────────────────────

/// Adds a line for a new voice to block `b`, resting for the whole block.
pub fn add_voice(source: &str, song: &Song, b: usize, name: &str) -> Result<String> {
    check_name(name)?;
    if song.blocks.is_empty() {
        let body = source.trim_end();
        let sep = if body.is_empty() { "" } else { "\n\n" };
        return checked(format!("{body}{sep}{name}: {} |\n", rest_bar(song.time)));
    }
    let info = &song.blocks[b];
    if info.voices.iter().any(|v| v == name) {
        return Err(format!("block {} already has {name}", b + 1).into());
    }
    let mut lines = lines_of(source);
    let (_, to) = region(&lines, song, b);
    lines.insert(
        to,
        format!("{name}: {} |", vec!["."; info.len().max(1)].join(" | ")),
    );
    checked(lines.join("\n"))
}

/// Deletes `voice`'s line from block `b`, or from every block; a block left with no voice goes.
pub fn delete_voice(source: &str, song: &Song, voice: &str, b: Option<usize>) -> Result<String> {
    let blocks: Vec<usize> = match b {
        Some(b) => vec![b],
        None => (0..song.blocks.len()).collect(),
    };
    let mut source = source.to_string();
    for &i in blocks.iter().rev() {
        let now = compile(&source)?;
        let info = &now.blocks[i];
        if !info.voices.iter().any(|v| v == voice) {
            continue;
        }
        source = if info.voices.len() == 1 {
            delete_block(&source, &now, i)?
        } else {
            apply(&source, |ed| ed.delete_key(&voice_path(i, voice)))?
        };
    }
    checked(source)
}

/// Renames a voice everywhere: its lines and its `octave` directives.
pub fn rename_voice(source: &str, song: &Song, from: &str, to: &str) -> Result<String> {
    if from == to {
        return Ok(source.to_string());
    }
    check_name(to)?;
    if song.voices.iter().any(|v| v == to) {
        return Err(format!("there is already a voice called {to}").into());
    }
    let octave = format!("octave {from}");
    checked(apply(source, |ed| {
        for (i, info) in song.blocks.iter().enumerate() {
            if info.voices.iter().any(|v| v == from) {
                ed.rename_key(&voice_path(i, from), to)?;
            }
            if info.directives.contains(&octave) {
                let mut p = settings_path(i);
                p.extend(path!["octave", from]);
                ed.rename_key(&p, to)?;
            }
        }
        Ok(())
    })?)
}

/// Moves `voice` one place earlier or later among the voices, swapping it
/// with its neighbour in every block that has both. Voices are ordered by
/// where they first appear, so that is the order changed.
pub fn move_voice(source: &str, song: &Song, voice: &str, dir: i32) -> Result<String> {
    let Some(at) = song.voice_index(voice) else {
        return Ok(source.to_string());
    };
    let other = at as i64 + dir as i64;
    if other < 0 || other >= song.voices.len() as i64 {
        return Ok(source.to_string());
    }
    let other = &song.voices[other as usize];
    let mut lines = lines_of(source);
    for (b, info) in song.blocks.iter().enumerate() {
        if !info.voices.iter().any(|v| v == voice) || !info.voices.iter().any(|v| v == other) {
            continue;
        }
        let i = voice_line_at(&lines, song, b, voice)?;
        let j = voice_line_at(&lines, song, b, other)?;
        lines.swap(i, j);
    }
    let next = lines.join("\n");
    if compile(&next)?.voice_index(voice) == Some(at) {
        return Err(format!(
            "{voice} and {other} are ordered by the block each first appears in; move a block instead"
        )
        .into());
    }
    Ok(next)
}
