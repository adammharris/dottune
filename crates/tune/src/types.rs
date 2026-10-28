//! What compiling a tune makes: the song, its events, and where each token
//! sits and sounds.

use std::fmt;

pub const TICKS_PER_QUARTER: i64 = 480;

/// A time signature: beats to the bar, and the note value of a beat.
pub type Time = (u32, u32);

/// Byte offsets in the source, `[start, end)`.
pub type Span = (usize, usize);

#[derive(Clone, Debug, PartialEq)]
pub struct NoteEvent {
    /// Start, in ticks.
    pub start: i64,
    /// Length, in ticks.
    pub duration: i64,
    pub voice: String,
    /// MIDI note number; C4 = 60.
    pub midi: i32,
    /// Index into [`Song::slots`] of the token that started this note.
    pub slot: usize,
}

impl NoteEvent {
    pub fn end(&self) -> i64 {
        self.start + self.duration
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Note,
    Stack,
    Chord,
    Rest,
    Hold,
}

/// One token as written, where it sits, and how it was placed.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotInfo {
    pub voice: String,
    /// Position in the source: block index, bar index, then indices into nested groups.
    pub block: usize,
    pub bar: usize,
    pub path: Vec<usize>,
    /// False for the rests that pad a short voice; they have no text to edit.
    pub editable: bool,
    pub text: String,
    /// Where `text` sits in the source; `None` for padding rests.
    pub span: Option<Span>,
    pub kind: SlotKind,
    pub start: i64,
    pub end: i64,
    pub midi: Vec<i32>,
}

impl SlotInfo {
    /// Whether the slot starts a note, stack, or chord.
    pub fn sounding(&self) -> bool {
        matches!(
            self.kind,
            SlotKind::Note | SlotKind::Stack | SlotKind::Chord
        )
    }

    /// The slot's address, which survives a recompile while the text around
    /// it keeps its shape.
    pub fn key(&self) -> SlotKey {
        SlotKey {
            voice: self.voice.clone(),
            block: self.block,
            bar: self.bar,
            path: self.path.clone(),
        }
    }
}

/// A slot by where it is written: its voice, block, bar and path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlotKey {
    pub voice: String,
    pub block: usize,
    pub bar: usize,
    pub path: Vec<usize>,
}

/// A song. `key`, `scale`, `octaves`, `tempo`, and `time` are the settings at the start.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub key: String,
    /// Semitones above the tonic for each scale degree.
    pub scale: [i32; 7],
    /// Voices with an explicit `octave` directive.
    pub octaves: Vec<(String, i64)>,
    pub tempo: f64,
    pub time: Time,
    /// Total length in ticks, including trailing rests.
    pub length: i64,
    /// Voice names in order of first appearance.
    pub voices: Vec<String>,
    pub events: Vec<NoteEvent>,
    /// Every token of every voice, grouped by voice in `voices` order, in time order.
    pub slots: Vec<SlotInfo>,
    pub blocks: Vec<BlockInfo>,
    /// Key, tempo, and time changes between blocks, in time order (SPEC §2.1).
    pub changes: Vec<Change>,
}

impl Song {
    pub fn voice_index(&self, voice: &str) -> Option<usize> {
        self.voices.iter().position(|v| v == voice)
    }

    /// The slot written at `key`, if there still is one.
    pub fn slot_at(&self, key: &SlotKey) -> Option<usize> {
        self.slots.iter().position(|s| {
            s.voice == key.voice && s.block == key.block && s.bar == key.bar && s.path == key.path
        })
    }

    /// The settings in force at a tick.
    pub fn settings_at(&self, tick: i64) -> Option<&BlockInfo> {
        let mut found = self.blocks.first();
        for b in &self.blocks {
            if b.start <= tick {
                found = Some(b);
            }
        }
        found
    }
}

/// A block, where it plays, and the settings in force for it.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockInfo {
    pub start: i64,
    pub end: i64,
    /// First voice line, 1-based.
    pub line: usize,
    pub key: String,
    pub scale: [i32; 7],
    pub tempo: f64,
    pub time: Time,
    /// Explicit `octave` settings in force.
    pub octaves: Vec<(String, i64)>,
    /// Directives written for this block: the header's for the first block,
    /// the change's before any other. Each is its directive word, or
    /// `octave <voice>`.
    pub directives: Vec<String>,
    /// Voices with a line in this block, in the order written.
    pub voices: Vec<String>,
    /// Bars each voice line holds, in `voices` order; the block is as long as the longest.
    pub bars: Vec<usize>,
}

impl BlockInfo {
    /// How many bars the block lasts.
    pub fn len(&self) -> usize {
        self.bars.iter().copied().max().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The bars `voice`'s line holds; 0 if it has none here.
    pub fn bars_of(&self, voice: &str) -> usize {
        self.voices
            .iter()
            .position(|v| v == voice)
            .map_or(0, |i| self.bars[i])
    }

    /// `voice`'s explicit octave here, if one is set.
    pub fn octave_set(&self, voice: &str) -> Option<i64> {
        self.octaves
            .iter()
            .find(|(v, _)| v == voice)
            .map(|(_, n)| *n)
    }

    /// `voice`'s octave here (SPEC §4.2).
    pub fn octave(&self, voice: &str) -> i64 {
        self.octave_set(voice)
            .unwrap_or_else(|| crate::default_octave(voice))
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Change {
    pub tick: i64,
    pub key: Option<String>,
    pub tempo: Option<f64>,
    pub time: Option<Time>,
}

/// An error in a tune, with its 1-based line and, where known, the text at fault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TuneError {
    pub line: usize,
    pub reason: String,
    pub span: Option<Span>,
}

impl TuneError {
    pub fn new(line: usize, reason: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            line,
            reason: reason.into(),
            span,
        }
    }
}

impl fmt::Display for TuneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for TuneError {}

/// Every error in a source, in order of position: the first is the one on
/// the lowest line (SPEC §7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TuneErrors(pub Vec<TuneError>);

impl TuneErrors {
    pub fn first(&self) -> &TuneError {
        &self.0[0]
    }
}

impl fmt::Display for TuneErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.first().fmt(f)
    }
}

impl std::error::Error for TuneErrors {}
