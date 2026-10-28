//! A tune's source to its song: the event list (SPEC §6), and where every
//! token sits and sounds.

use crate::theory::{
    Note, accidental, letter_pc, mode_rotation, mode_scale, modulo, pitch_name, read_chord,
    read_note, read_stack,
};
use crate::types::{
    BlockInfo, Change, NoteEvent, SlotInfo, SlotKind, Song, Span, TICKS_PER_QUARTER, Time,
    TuneError, TuneErrors,
};

// ── exact time ──────────────────────────────────────────────────────────────
//
// Positions are fractions until output (SPEC §3.3).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Q {
    n: i64,
    d: i64,
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn q(mut n: i64, mut d: i64) -> Q {
    if d < 0 {
        (n, d) = (-n, -d);
    }
    let g = gcd(n, d).max(1);
    Q { n: n / g, d: d / g }
}

impl Q {
    fn add(self, b: Q) -> Q {
        q(self.n * b.d + b.n * self.d, self.d * b.d)
    }
    fn mul(self, k: i64) -> Q {
        q(self.n * k, self.d)
    }
    fn div(self, k: i64) -> Q {
        q(self.n, self.d * k)
    }
    /// To the nearest tick, halves up.
    fn ticks(self) -> i64 {
        (2 * self.n + self.d).div_euclid(2 * self.d)
    }
}

// ── parsing ─────────────────────────────────────────────────────────────────

pub const DIRECTIVES: [&str; 4] = ["key", "tempo", "time", "octave"];

/// Tempo limits: below 4 bpm a quarter note is too long for MIDI's tempo field.
pub const MIN_TEMPO: f64 = 4.0;
pub const MAX_TEMPO: f64 = 1000.0;
/// The longest bar numerator, and the denominators allowed (powers of two, as MIDI requires).
const MAX_BEATS: u64 = 64;
const DENOMINATORS: [u64; 7] = [1, 2, 4, 8, 16, 32, 64];
/// MIDI's pitch range: C-1 to G9.
const LOWEST: i64 = 0;
const HIGHEST: i64 = 127;

/// A voice's octave, absent an `octave` directive (SPEC §4.2).
pub fn default_octave(voice: &str) -> i64 {
    if voice == "LH" { 3 } else { 4 }
}

/// Whether `name` could name a voice, by its spelling alone.
pub fn is_voice_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The voice name `s` starts with, if any.
fn voice_name_prefix(s: &str) -> Option<&str> {
    if !s.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    Some(&s[..end])
}

/// A voice line's name and what follows its colon: `Name\s*:(.*)`.
pub(crate) fn split_voice_line(text: &str) -> Option<(&str, &str)> {
    let name = voice_name_prefix(text)?;
    let rest = text[name.len()..].trim_start();
    rest.strip_prefix(':').map(|bars| (name, bars))
}

#[derive(Clone, Debug)]
struct Header {
    key: String,
    tonic_pc: i32,
    scale: [i32; 7],
    tempo: f64,
    time: Time,
    octaves: Vec<(String, i64)>,
}

impl Header {
    fn octave(&self, voice: &str) -> i64 {
        self.octaves
            .iter()
            .find(|(v, _)| v == voice)
            .map_or_else(|| default_octave(voice), |(_, n)| *n)
    }
}

/// A token, with where it sits in the source, or a `[ ]` group.
#[derive(Clone, Debug)]
enum Item {
    Token {
        text: String,
        from: usize,
        to: usize,
    },
    Group(Vec<Item>),
}

struct VoiceLine {
    name: String,
    line: usize,
    bars: Vec<Vec<Item>>,
}

pub(crate) fn strip_comment(line: &str) -> &str {
    line.find("//").map_or(line, |i| &line[..i])
}

/// `s` with a leading `word` and at least one whitespace after it removed.
fn after_word<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(word)?;
    let trimmed = rest.trim_start();
    (trimmed.len() < rest.len()).then_some(trimmed)
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `\d+(\.\d+)?`
fn is_decimal(s: &str) -> bool {
    match s.split_once('.') {
        Some((a, b)) => all_digits(a) && all_digits(b),
        None => all_digits(s),
    }
}

/// Applies a directive to `h`; `span` is where `text` sits in the source.
fn parse_directive(text: &str, line: usize, h: &mut Header, span: Span) -> Result<(), TuneError> {
    let word = text.split_whitespace().next().unwrap_or("");
    let fail = |reason: String| TuneError::new(line, reason, Some(span));
    match word {
        "key" => {
            let invalid = || fail(format!("invalid key: \"{text}\""));
            let rest = after_word(text, "key").ok_or_else(invalid)?;
            let mut chars = rest.chars();
            let letter = chars
                .next()
                .filter(|c| ('A'..='G').contains(c))
                .ok_or_else(invalid)?;
            let after = chars.as_str();
            let (acc, after) = match after.chars().next() {
                Some(c @ ('#' | 'b')) => (c.to_string(), &after[1..]),
                _ => (String::new(), after),
            };
            let mode_text = if after.is_empty() {
                None
            } else {
                let m = after.trim_start();
                if m.len() == after.len()
                    || m.is_empty()
                    || !m.chars().all(|c| c.is_ascii_alphabetic())
                {
                    return Err(invalid());
                }
                Some(m)
            };
            let mode = mode_text.unwrap_or("major").to_lowercase();
            let rotation = mode_rotation(&mode)
                .ok_or_else(|| fail(format!("unknown mode \"{}\"", mode_text.unwrap_or(""))))?;
            h.tonic_pc = letter_pc(letter).unwrap_or(0) + accidental(&acc);
            h.scale = mode_scale(rotation);
            h.key = format!("{letter}{acc} {mode}");
            Ok(())
        }
        "tempo" => {
            let value = after_word(text, "tempo")
                .filter(|v| is_decimal(v))
                .ok_or_else(|| fail(format!("invalid tempo: \"{text}\"")))?;
            let bpm: f64 = value.parse().unwrap_or(f64::INFINITY);
            if !(MIN_TEMPO..=MAX_TEMPO).contains(&bpm) {
                return Err(fail(format!(
                    "tempo must be from {MIN_TEMPO} to {MAX_TEMPO}, got {value}"
                )));
            }
            h.tempo = bpm;
            Ok(())
        }
        "time" => {
            let invalid = || fail(format!("invalid time signature: \"{text}\""));
            let rest = after_word(text, "time").ok_or_else(invalid)?;
            let (n, d) = rest.split_once('/').ok_or_else(invalid)?;
            let (n, d) = (n.trim_end(), d.trim_start());
            if !all_digits(n) || !all_digits(d) {
                return Err(invalid());
            }
            let beats: u64 = n.parse().unwrap_or(u64::MAX);
            if !(1..=MAX_BEATS).contains(&beats) {
                return Err(fail(format!(
                    "a bar must have from 1 to {MAX_BEATS} beats, got {n}"
                )));
            }
            let beat: u64 = d.parse().unwrap_or(u64::MAX);
            if !DENOMINATORS.contains(&beat) {
                let list: Vec<String> = DENOMINATORS.iter().map(|d| d.to_string()).collect();
                return Err(fail(format!(
                    "the beat must be one of {}, got {d}",
                    list.join(", ")
                )));
            }
            h.time = (beats as u32, beat as u32);
            Ok(())
        }
        "octave" => {
            let invalid = || fail("expected \"octave <voice> <n>\"".into());
            let rest = after_word(text, "octave").ok_or_else(invalid)?;
            let name = voice_name_prefix(rest).ok_or_else(invalid)?;
            let n = after_word(rest, name).ok_or_else(invalid)?;
            let digits = n.strip_prefix('-').unwrap_or(n);
            if !all_digits(digits) {
                return Err(invalid());
            }
            if DIRECTIVES.contains(&name) {
                return Err(fail(format!(
                    "\"{name}\" is a directive and cannot name a voice"
                )));
            }
            // Past ±1000 every pitch is out of MIDI's range anyway.
            let value = n
                .parse::<i64>()
                .unwrap_or(if n.starts_with('-') { -1000 } else { 1000 })
                .clamp(-1000, 1000);
            match h.octaves.iter_mut().find(|(v, _)| v == name) {
                Some(entry) => entry.1 = value,
                None => h.octaves.push((name.to_string(), value)),
            }
            Ok(())
        }
        _ => Err(fail(format!("unknown directive \"{word}\""))),
    }
}

/// Parses one bar; `offset` is where `text` starts in the source.
fn parse_bar(text: &str, line: usize, offset: usize) -> Result<Vec<Item>, TuneError> {
    let mut stack: Vec<Vec<Item>> = vec![vec![]];
    // Where each open `[` is.
    let mut opens: Vec<usize> = vec![];
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        let at = offset + i;
        if c.is_whitespace() {
            i += c.len_utf8();
        } else if c == '[' {
            stack.push(vec![]);
            opens.push(at);
            i += 1;
        } else if c == ']' {
            if stack.len() == 1 {
                return Err(TuneError::new(line, "unbalanced \"]\"", Some((at, at + 1))));
            }
            let items = stack.pop().unwrap();
            let open = opens.pop().unwrap();
            if items.is_empty() {
                return Err(TuneError::new(
                    line,
                    "empty group \"[]\"",
                    Some((open, at + 1)),
                ));
            }
            stack.last_mut().unwrap().push(Item::Group(items));
            i += 1;
        } else {
            let start = i;
            while i < text.len() {
                let c = text[i..].chars().next().unwrap();
                if c.is_whitespace() || c == '[' || c == ']' {
                    break;
                }
                i += c.len_utf8();
            }
            stack.last_mut().unwrap().push(Item::Token {
                text: text[start..i].to_string(),
                from: offset + start,
                to: offset + i,
            });
        }
    }
    if stack.len() != 1 {
        let open = *opens.last().unwrap();
        return Err(TuneError::new(
            line,
            "unbalanced \"[\"",
            Some((open, open + 1)),
        ));
    }
    Ok(stack.pop().unwrap())
}

/// Parses a voice line's bars. A bar that fails to parse is reported to
/// `errors` and read as a rest, so the bars after it are still checked.
fn parse_bars(
    text: &str,
    line: usize,
    mut offset: usize,
    errors: &mut Vec<TuneError>,
) -> Vec<Vec<Item>> {
    let mut parts: Vec<&str> = text.split('|').collect();
    if parts.len() > 1 && parts.last().unwrap().trim().is_empty() {
        parts.pop();
    }
    let end = offset + text.len();
    let mut bars = vec![];
    for p in parts {
        let parsed = if p.trim().is_empty() {
            // An empty bar is marked by the `|` that closes it, or the one before it at the end of the line.
            let at = if offset + p.len() < end {
                offset + p.len()
            } else {
                offset - 1
            };
            Err(TuneError::new(line, "empty bar", Some((at, at + 1))))
        } else {
            parse_bar(p, line, offset)
        };
        bars.push(parsed.unwrap_or_else(|e| {
            errors.push(e);
            vec![Item::Token {
                text: ".".into(),
                from: offset,
                to: offset,
            }]
        }));
        offset += p.len() + 1;
    }
    bars
}

// ── pitch ───────────────────────────────────────────────────────────────────

/// Where a voice's notes are placed: the tonic of its octave, and the key's scale (SPEC §4.2).
struct Place {
    tonic_midi: i64,
    scale: [i32; 7],
}

impl Place {
    /// The pitch of a diatonic index (0 = the tonic of the voice's octave), plus an accidental.
    fn midi_at(&self, idx: i64, acc: i32) -> i64 {
        self.tonic_midi
            + 12 * idx.div_euclid(7)
            + self.scale[modulo(idx, 7) as usize] as i64
            + acc as i64
    }

    fn note(&self, n: &Note) -> i64 {
        self.midi_at(n.index(), accidental(n.acc))
    }
}

/// The MIDI notes a sounding token produces, checked to be in MIDI's range.
fn sound(text: &str, line: usize, p: &Place, span: Option<Span>) -> Result<Vec<i32>, TuneError> {
    let notes = spell(text, line, p, span)?;
    for &midi in &notes {
        if midi < LOWEST {
            return Err(TuneError::new(
                line,
                format!(
                    "\"{text}\" is below {}, the lowest MIDI note",
                    pitch_name(LOWEST as i32)
                ),
                span,
            ));
        }
        if midi > HIGHEST {
            return Err(TuneError::new(
                line,
                format!(
                    "\"{text}\" is above {}, the highest MIDI note",
                    pitch_name(HIGHEST as i32)
                ),
                span,
            ));
        }
    }
    Ok(notes.into_iter().map(|m| m as i32).collect())
}

fn spell(text: &str, line: usize, p: &Place, span: Option<Span>) -> Result<Vec<i64>, TuneError> {
    if let Some(n) = read_note(text) {
        return Ok(vec![p.note(&n)]);
    }
    if let Some(c) = read_chord(text) {
        if c.quality == "h" && c.seventh != "7" {
            return Err(TuneError::new(
                line,
                format!("\"h\" must be followed by \"7\" in \"{text}\""),
                span,
            ));
        }
        let root_idx = c.index();
        let root = p.midi_at(root_idx, accidental(&c.acc));
        let mut notes: Vec<i64> = c.tones().iter().map(|&i| root + i as i64).collect();
        if !c.bass_deg.is_empty() {
            let deg: i64 = c.bass_deg.parse().unwrap_or(1);
            let below = match modulo(root_idx - (deg - 1), 7) {
                0 => 7,
                b => b,
            };
            notes.insert(0, p.midi_at(root_idx - below, accidental(&c.bass_acc)));
        }
        return Ok(notes);
    }
    if text.contains('+') {
        return match read_stack(text) {
            Some(members) => Ok(members.iter().map(|n| p.note(n)).collect()),
            None => Err(TuneError::new(
                line,
                format!("invalid stack \"{text}\""),
                span,
            )),
        };
    }
    Err(TuneError::new(
        line,
        format!("unknown token \"{text}\""),
        span,
    ))
}

pub fn kind_of(text: &str) -> SlotKind {
    match text {
        "-" => SlotKind::Hold,
        "." => SlotKind::Rest,
        _ if read_note(text).is_some() => SlotKind::Note,
        _ if read_chord(text).is_some() => SlotKind::Chord,
        _ => SlotKind::Stack,
    }
}

// ── compile ─────────────────────────────────────────────────────────────────

struct Slot {
    start: Q,
    end: Q,
    text: String,
    line: usize,
    block: usize,
    bar: usize,
    path: Vec<usize>,
    editable: bool,
    span: Option<Span>,
}

struct Block {
    voices: Vec<VoiceLine>,
    /// The settings in force for this block.
    settings: Header,
    /// Which directives the header (for the first block) or the change before this block set (SPEC §2.1).
    directives: Vec<String>,
    /// First voice line.
    line: usize,
}

fn same_key(a: &Header, b: &Header) -> bool {
    a.tonic_pc == b.tonic_pc && a.scale == b.scale
}

/// Lays `items` out evenly over `len` from `start` (SPEC §3).
fn layout(items: &[Item], start: Q, len: Q, at: &Slot, out: &mut Vec<Slot>) {
    let each = len.div(items.len() as i64);
    for (j, item) in items.iter().enumerate() {
        let s = start.add(each.mul(j as i64));
        let mut path = at.path.clone();
        path.push(j);
        let here = Slot {
            start: s,
            end: s.add(each),
            text: String::new(),
            line: at.line,
            block: at.block,
            bar: at.bar,
            path,
            editable: true,
            span: None,
        };
        match item {
            Item::Token { text, from, to } => out.push(Slot {
                text: text.clone(),
                span: Some((*from, *to)),
                ..here
            }),
            Item::Group(items) => layout(items, s, each, &here, out),
        }
    }
}

/// Compiles a tune. Every error is found in one pass; they come back in
/// order of position, the first being the one on the lowest line.
pub fn compile(source: &str) -> Result<Song, TuneErrors> {
    let mut cur = Header {
        key: "C major".into(),
        tonic_pc: 0,
        scale: mode_scale(0),
        tempo: 120.0,
        time: (4, 4),
        octaves: vec![],
    };
    let mut blocks: Vec<Block> = vec![];
    // Whether a block is open: a voice line was read since the last blank line.
    let mut open = false;
    // Directives read since the last block: the change the next block starts with.
    let mut pending: Vec<String> = vec![];
    let mut pending_first: Option<(usize, Span)> = None;
    let mut errors: Vec<TuneError> = vec![];

    let mut line_start = 0;
    for (i, raw_line) in source.split('\n').enumerate() {
        let line_no = i + 1;
        let offset = line_start;
        line_start += raw_line.len() + 1;
        let raw = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if raw.trim().is_empty() {
            open = false;
            continue;
        }
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }
        let text_start = offset + (raw.len() - raw.trim_start().len());
        let text_span = (text_start, text_start + text.len());

        if let Some((name, bars)) = split_voice_line(text) {
            let name_span = (text_start, text_start + name.len());
            if DIRECTIVES.contains(&name) {
                errors.push(TuneError::new(
                    line_no,
                    format!("\"{name}\" is a directive and cannot name a voice"),
                    Some(name_span),
                ));
                continue;
            }
            if !open {
                blocks.push(Block {
                    voices: vec![],
                    settings: cur.clone(),
                    directives: std::mem::take(&mut pending),
                    line: line_no,
                });
                pending_first = None;
                open = true;
            }
            let block = blocks.last_mut().unwrap();
            if block.voices.iter().any(|v| v.name == name) {
                errors.push(TuneError::new(
                    line_no,
                    format!("voice \"{name}\" appears twice in one block"),
                    Some(name_span),
                ));
                continue;
            }
            let colon = offset + raw.find(':').unwrap();
            if bars.trim().is_empty() {
                errors.push(TuneError::new(
                    line_no,
                    "empty bar",
                    Some((colon, colon + 1)),
                ));
                continue;
            }
            let bars = parse_bars(bars, line_no, colon + 1, &mut errors);
            block.voices.push(VoiceLine {
                name: name.to_string(),
                line: line_no,
                bars,
            });
            continue;
        }

        let word = text.split_whitespace().next().unwrap_or("");
        if open {
            let reason = if DIRECTIVES.contains(&word) {
                format!(
                    "\"{word}\" is inside a block; changes go between blocks, after a blank line"
                )
            } else {
                format!("expected \"Name: bars…\", got \"{text}\"")
            };
            errors.push(TuneError::new(line_no, reason, Some(text_span)));
            continue;
        }
        if pending_first.is_none() {
            pending_first = Some((line_no, text_span));
        }
        match parse_directive(text, line_no, &mut cur, text_span) {
            Ok(()) => {
                let d = if word == "octave" {
                    format!("octave {}", text.split_whitespace().nth(1).unwrap_or(""))
                } else {
                    word.to_string()
                };
                if !pending.contains(&d) {
                    pending.push(d);
                }
            }
            Err(e) => errors.push(e),
        }
    }
    if !blocks.is_empty()
        && let Some((line, span)) = pending_first
    {
        errors.push(TuneError::new(
            line,
            "a change must be followed by a block",
            Some(span),
        ));
    }

    let mut voices: Vec<String> = vec![];
    for b in &blocks {
        for v in &b.voices {
            if !voices.contains(&v.name) {
                voices.push(v.name.clone());
            }
        }
    }

    // Lay every voice out on the timeline (SPEC §1.1, §3).
    let mut slots: Vec<Vec<Slot>> = voices.iter().map(|_| vec![]).collect();
    let mut block_infos: Vec<BlockInfo> = vec![];
    let mut changes: Vec<Change> = vec![];
    let mut block_start = q(0, 1);

    for (block_index, b) in blocks.iter().enumerate() {
        let h = &b.settings;
        let bar_len = q(h.time.0 as i64 * 4 * TICKS_PER_QUARTER, h.time.1 as i64);
        // A block whose every voice line failed to parse has no bars.
        let n_bars = b.voices.iter().map(|v| v.bars.len()).max().unwrap_or(0);
        for (vi, name) in voices.iter().enumerate() {
            let vl = b.voices.iter().find(|v| &v.name == name);
            let out = &mut slots[vi];
            for k in 0..n_bars {
                let start = block_start.add(bar_len.mul(k as i64));
                let at = Slot {
                    start,
                    end: start.add(bar_len),
                    text: ".".into(),
                    line: vl.map_or(0, |v| v.line),
                    block: block_index,
                    bar: k,
                    path: vec![],
                    editable: false,
                    span: None,
                };
                match vl.and_then(|v| v.bars.get(k)) {
                    Some(bar) => layout(bar, start, bar_len, &at, out),
                    None => out.push(at),
                }
            }
        }
        let block_end = block_start.add(bar_len.mul(n_bars as i64));

        if block_index > 0 {
            let prev = &blocks[block_index - 1].settings;
            let change = Change {
                tick: block_start.ticks(),
                time: (prev.time != h.time).then_some(h.time),
                key: (!same_key(prev, h)).then(|| h.key.clone()),
                tempo: (prev.tempo != h.tempo).then_some(h.tempo),
            };
            if change.time.is_some() || change.key.is_some() || change.tempo.is_some() {
                changes.push(change);
            }
        }
        block_infos.push(BlockInfo {
            start: block_start.ticks(),
            end: block_end.ticks(),
            line: b.line,
            key: h.key.clone(),
            scale: h.scale,
            tempo: h.tempo,
            time: h.time,
            octaves: h.octaves.clone(),
            directives: b.directives.clone(),
            voices: b.voices.iter().map(|v| v.name.clone()).collect(),
            bars: b.voices.iter().map(|v| v.bars.len()).collect(),
        });
        block_start = block_end;
    }

    // Turn slots into events.
    let mut events: Vec<NoteEvent> = vec![];
    let mut slot_infos: Vec<SlotInfo> = vec![];
    for (vi, name) in voices.iter().enumerate() {
        struct Sounding {
            start: Q,
            end: Q,
            notes: Vec<i32>,
            slot: usize,
        }
        let mut sounding: Option<Sounding> = None;
        let flush = |sounding: &mut Option<Sounding>, events: &mut Vec<NoteEvent>| {
            if let Some(s) = sounding.take() {
                let start = s.start.ticks();
                let duration = s.end.ticks() - start;
                for midi in s.notes {
                    events.push(NoteEvent {
                        start,
                        duration,
                        voice: name.clone(),
                        midi,
                        slot: s.slot,
                    });
                }
            }
        };

        for slot in &slots[vi] {
            let mut notes: Vec<i32> = vec![];
            match slot.text.as_str() {
                "-" => {
                    if let Some(s) = sounding.as_mut() {
                        s.end = slot.end;
                    }
                }
                "." => flush(&mut sounding, &mut events),
                text => {
                    let h = &blocks[slot.block].settings;
                    let place = Place {
                        tonic_midi: 12 * (h.octave(name) + 1) + h.tonic_pc as i64,
                        scale: h.scale,
                    };
                    match sound(text, slot.line, &place, slot.span) {
                        Ok(n) => notes = n,
                        Err(e) => errors.push(e),
                    }
                    flush(&mut sounding, &mut events);
                    sounding = Some(Sounding {
                        start: slot.start,
                        end: slot.end,
                        notes: notes.clone(),
                        slot: slot_infos.len(),
                    });
                }
            }
            slot_infos.push(SlotInfo {
                voice: name.clone(),
                block: slot.block,
                bar: slot.bar,
                path: slot.path.clone(),
                editable: slot.editable,
                text: slot.text.clone(),
                span: slot.span,
                kind: kind_of(&slot.text),
                start: slot.start.ticks(),
                end: slot.end.ticks(),
                midi: notes,
            });
        }
        flush(&mut sounding, &mut events);
    }

    if !errors.is_empty() {
        errors.sort_by_key(|e| (e.line, e.span.map_or(0, |s| s.0)));
        return Err(TuneErrors(errors));
    }

    let order = |v: &str| voices.iter().position(|x| x == v).unwrap_or(0);
    events.sort_by(|a, b| {
        (a.start, order(&a.voice), a.midi).cmp(&(b.start, order(&b.voice), b.midi))
    });

    let first = blocks.first().map_or(&cur, |b| &b.settings);
    Ok(Song {
        key: first.key.clone(),
        scale: first.scale,
        octaves: first.octaves.clone(),
        tempo: first.tempo,
        time: first.time,
        length: block_start.ticks(),
        voices,
        events,
        slots: slot_infos,
        blocks: block_infos,
        changes,
    })
}

/// The text form of a song: changes and note events (SPEC §6).
pub fn format_song(song: &Song) -> String {
    let mut lines: Vec<(i64, usize, String)> = vec![];
    for c in &song.changes {
        if let Some((n, d)) = c.time {
            lines.push((c.tick, 0, format!("{} time {n}/{d}", c.tick)));
        }
        if let Some(k) = &c.key {
            lines.push((c.tick, 1, format!("{} key {k}", c.tick)));
        }
        if let Some(t) = c.tempo {
            lines.push((c.tick, 2, format!("{} tempo {t}", c.tick)));
        }
    }
    for (i, e) in song.events.iter().enumerate() {
        lines.push((e.start, 3 + i, format_event(e)));
    }
    lines.sort_by_key(|l| (l.0, l.1));
    lines
        .into_iter()
        .map(|l| l.2)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_event(e: &NoteEvent) -> String {
    format!(
        "{} {} {} {}",
        e.start,
        e.duration,
        e.voice,
        pitch_name(e.midi)
    )
}

/// The text form of an event list (SPEC §6).
pub fn format_events(events: &[NoteEvent]) -> String {
    events
        .iter()
        .map(format_event)
        .collect::<Vec<_>>()
        .join("\n")
}
