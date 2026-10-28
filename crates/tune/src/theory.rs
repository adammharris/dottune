//! Scales, pitch names, and the grammar of the sounding tokens: a note
//! (SPEC §4), a stack (§5.1) and a chord (§5.2).

const IONIAN: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
/// The numerals a chord is written with, by degree; each also upper case.
pub const NUMERALS: [&str; 7] = ["i", "ii", "iii", "iv", "v", "vi", "vii"];
/// Every mode name, and how far up the major scale it starts.
pub const MODES: [(&str, usize); 9] = [
    ("major", 0),
    ("ionian", 0),
    ("dorian", 1),
    ("phrygian", 2),
    ("lydian", 3),
    ("mixolydian", 4),
    ("minor", 5),
    ("aeolian", 5),
    ("locrian", 6),
];

pub fn modulo(a: i64, m: i64) -> i64 {
    a.rem_euclid(m)
}

/// The rotation of the major scale a mode is.
pub fn mode_rotation(mode: &str) -> Option<usize> {
    MODES.iter().find(|(m, _)| *m == mode).map(|(_, r)| *r)
}

/// Semitones above the tonic of each degree of the mode `rotation` steps up the major scale.
pub fn mode_scale(rotation: usize) -> [i32; 7] {
    std::array::from_fn(|i| (IONIAN[(i + rotation) % 7] - IONIAN[rotation] + 12) % 12)
}

/// The pitch class of a letter `A`–`G`.
pub fn letter_pc(c: char) -> Option<i32> {
    Some(match c {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    })
}

pub fn accidental(s: &str) -> i32 {
    match s {
        "#" => 1,
        "b" => -1,
        _ => 0,
    }
}

/// The pitch class of a key's tonic: `"Eb major"` → 3.
pub fn tonic_of(key: &str) -> i32 {
    let mut chars = key.chars();
    let letter = chars.next().and_then(letter_pc).unwrap_or(0);
    let acc = match chars.next() {
        Some('#') => 1,
        Some('b') => -1,
        _ => 0,
    };
    (letter + acc).rem_euclid(12)
}

/// Scientific pitch name, sharps only: 60 → `C4`.
pub fn pitch_name(midi: i32) -> String {
    format!(
        "{}{}",
        NAMES[midi.rem_euclid(12) as usize],
        midi.div_euclid(12) - 1
    )
}

/// What `'` and `,` marks add up to, in octaves.
pub fn count_marks(s: &str) -> i64 {
    s.chars().map(|c| if c == '\'' { 1 } else { -1 }).sum()
}

pub fn marks_text(n: i64) -> String {
    if n > 0 {
        "'".repeat(n as usize)
    } else {
        ",".repeat((-n) as usize)
    }
}

/// Splits off the run of leading characters that satisfy `f`.
fn take_while(s: &str, f: impl Fn(char) -> bool) -> (&str, &str) {
    let end = s.find(|c: char| !f(c)).unwrap_or(s.len());
    s.split_at(end)
}

fn take_accidental(s: &str) -> (&str, &str) {
    if s.starts_with('#') || s.starts_with('b') {
        s.split_at(1)
    } else {
        ("", s)
    }
}

fn is_mark(c: char) -> bool {
    c == '\'' || c == ','
}

/// A note token: `[#b]? [1-7] [',]*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note<'a> {
    pub acc: &'a str,
    /// 1–7.
    pub degree: i64,
    pub marks: &'a str,
}

impl Note<'_> {
    /// Its diatonic index: 0 is the tonic of the voice's octave.
    pub fn index(&self) -> i64 {
        self.degree - 1 + 7 * count_marks(self.marks)
    }
}

pub fn read_note(text: &str) -> Option<Note<'_>> {
    let (acc, rest) = take_accidental(text);
    let mut chars = rest.chars();
    let d = chars.next()?;
    if !('1'..='7').contains(&d) {
        return None;
    }
    let marks = chars.as_str();
    if !marks.chars().all(is_mark) {
        return None;
    }
    Some(Note {
        acc,
        degree: (d as u8 - b'0') as i64,
        marks,
    })
}

/// A chord token taken apart (SPEC §5.2):
/// `[#b]? numeral [oh+]? (maj7|7)? [',]* (/[#b]?[1-7])?`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Chord {
    pub acc: String,
    pub numeral: String,
    pub quality: String,
    /// `""`, `"7"` or `"maj7"`.
    pub seventh: String,
    pub marks: String,
    pub bass_acc: String,
    /// `""`, or the slash bass's degree, `1`–`7`.
    pub bass_deg: String,
}

/// The numerals, longest first, as the grammar tries them.
const NUMERAL_ORDER: [&str; 14] = [
    "VII", "VI", "V", "IV", "III", "II", "I", "vii", "vi", "v", "iv", "iii", "ii", "i",
];

pub fn read_chord(text: &str) -> Option<Chord> {
    let (acc, rest) = take_accidental(text);
    let numeral = NUMERAL_ORDER.iter().find(|n| rest.starts_with(**n))?;
    let rest = &rest[numeral.len()..];
    let (quality, rest) = match rest.chars().next() {
        Some(c @ ('o' | 'h' | '+')) => (&rest[..c.len_utf8()], &rest[c.len_utf8()..]),
        _ => ("", rest),
    };
    let (seventh, rest) = if let Some(r) = rest.strip_prefix("maj7") {
        ("maj7", r)
    } else if let Some(r) = rest.strip_prefix('7') {
        ("7", r)
    } else {
        ("", rest)
    };
    let (marks, rest) = take_while(rest, is_mark);
    let (bass_acc, bass_deg) = if rest.is_empty() {
        ("", "")
    } else {
        let r = rest.strip_prefix('/')?;
        let (a, d) = take_accidental(r);
        if d.len() != 1 || !('1'..='7').contains(&d.chars().next()?) {
            return None;
        }
        (a, d)
    };
    Some(Chord {
        acc: acc.into(),
        numeral: (*numeral).into(),
        quality: quality.into(),
        seventh: seventh.into(),
        marks: marks.into(),
        bass_acc: bass_acc.into(),
        bass_deg: bass_deg.into(),
    })
}

impl Chord {
    pub fn write(&self) -> String {
        let bass = if self.bass_deg.is_empty() {
            String::new()
        } else {
            format!("/{}{}", self.bass_acc, self.bass_deg)
        };
        format!(
            "{}{}{}{}{}{}",
            self.acc, self.numeral, self.quality, self.seventh, self.marks, bass
        )
    }

    pub fn upper(&self) -> bool {
        self.numeral == self.numeral.to_uppercase()
    }

    /// The scale degree (0-based) it is built on.
    pub fn root(&self) -> i64 {
        let lower = self.numeral.to_lowercase();
        NUMERALS.iter().position(|n| *n == lower).unwrap_or(0) as i64
    }

    /// Its root's diatonic index.
    pub fn index(&self) -> i64 {
        self.root() + 7 * count_marks(&self.marks)
    }

    /// Semitones above the root of each chord tone.
    pub fn tones(&self) -> Vec<i32> {
        let mut tones = match self.quality.as_str() {
            "o" | "h" => vec![0, 3, 6],
            "+" => vec![0, 4, 8],
            _ if self.upper() => vec![0, 4, 7],
            _ => vec![0, 3, 7],
        };
        match self.seventh.as_str() {
            "maj7" => tones.push(11),
            "7" => tones.push(if self.quality == "o" { 9 } else { 10 }),
            _ => {}
        }
        tones
    }
}

/// The members of a stack, if `text` is one: two or more notes joined by `+`.
pub fn read_stack(text: &str) -> Option<Vec<Note<'_>>> {
    if !text.contains('+') {
        return None;
    }
    text.split('+').map(read_note).collect()
}

/// The diatonic index of a note, of a chord's root, or of a stack's first member.
pub fn index_of(text: &str) -> i64 {
    if let Some(n) = read_note(text) {
        return n.index();
    }
    if let Some(c) = read_chord(text) {
        return c.index();
    }
    text.split('+')
        .next()
        .and_then(read_note)
        .map_or(0, |n| n.index())
}

/// A degree (1–7) with the marks that put it at diatonic index `idx`.
pub fn degree_at(idx: i64) -> String {
    format!("{}{}", modulo(idx, 7) + 1, marks_text(idx.div_euclid(7)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_read_as_the_grammar_says() {
        let c = read_chord("bVIImaj7'/#3").unwrap();
        assert_eq!(
            (
                c.acc.as_str(),
                c.numeral.as_str(),
                c.seventh.as_str(),
                c.marks.as_str()
            ),
            ("b", "VII", "maj7", "'")
        );
        assert_eq!((c.bass_acc.as_str(), c.bass_deg.as_str()), ("#", "3"));
        assert_eq!(read_chord("viio,").unwrap().quality, "o");
        assert_eq!(read_chord("IV").unwrap().numeral, "IV");
        assert!(read_chord("Iv").is_none());
        assert!(read_chord("I/8").is_none());
        assert!(read_chord("x").is_none());
        assert_eq!(read_chord("bVII,").unwrap().write(), "bVII,");
    }

    #[test]
    fn notes_and_stacks() {
        assert_eq!(read_note("#4''").unwrap().index(), 3 + 14);
        assert!(read_note("8").is_none());
        assert!(read_note("1x").is_none());
        assert_eq!(read_stack("1+3+5").unwrap().len(), 3);
        assert!(read_stack("1+I").is_none());
        assert_eq!(degree_at(-1), "7,");
    }

    #[test]
    fn scales_and_names() {
        assert_eq!(mode_scale(5), [0, 2, 3, 5, 7, 8, 10]);
        assert_eq!(pitch_name(60), "C4");
        assert_eq!(pitch_name(0), "C-1");
        assert_eq!(tonic_of("Eb major"), 3);
    }
}
