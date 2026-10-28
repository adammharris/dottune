//! Standard MIDI Files (SMF): a format-1 writer for songs, and a small reader.
//!
//! Track 0 is the conductor (tempo, time and key signature); each voice gets
//! its own track, named after the voice, and its own channel while they last.
//! MIDI has 15 melodic channels, so from the 16th voice on, voices share them
//! in turn. Ticks are the event list's own: 480 per quarter note.

use crate::theory::mode_rotation;
use crate::types::{Song, TICKS_PER_QUARTER, Time};

const VELOCITY: u8 = 80;
/// Every channel but 10 (index 9), the drum channel.
const CHANNELS: [u8; 15] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];
const LETTERS: &str = "CDEFGAB";
const NATURAL: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Sharps (+) or flats (−) in each spelled major key.
const MAJOR_SIG: [(&str, i8); 15] = [
    ("Cb", -7),
    ("Gb", -6),
    ("Db", -5),
    ("Ab", -4),
    ("Eb", -3),
    ("Bb", -2),
    ("F", -1),
    ("C", 0),
    ("G", 1),
    ("D", 2),
    ("A", 3),
    ("E", 4),
    ("B", 5),
    ("F#", 6),
    ("C#", 7),
];

/// The key signature of a key such as "D dorian": sharps (+) or flats (−),
/// and whether it is minor.
pub fn key_signature(key: &str) -> (i8, bool) {
    let mut parts = key.split(' ');
    let tonic = parts.next().filter(|t| !t.is_empty()).unwrap_or("C");
    let mode = parts.next().unwrap_or("major");
    let rotation = mode_rotation(mode).unwrap_or(0);
    let tonic_letter = LETTERS
        .find(tonic.chars().next().unwrap_or('C'))
        .unwrap_or(0);
    // The major key this mode is a rotation of, spelled from the tonic's letter.
    let letter = (tonic_letter + 7 - rotation) % 7;
    let acc = match tonic.chars().nth(1) {
        Some('#') => 1,
        Some('b') => -1,
        _ => 0,
    };
    let tonic_pc = NATURAL[tonic_letter] + acc;
    let parent_pc = tonic_pc - NATURAL[rotation];
    let off = ((parent_pc - NATURAL[letter]) % 12 + 18) % 12 - 6;
    let name = format!(
        "{}{}",
        &LETTERS[letter..letter + 1],
        if off > 0 {
            "#".repeat(off as usize)
        } else {
            "b".repeat((-off) as usize)
        }
    );
    // A parent key outside the table (e.g. G# major) has no signature; fall back to none.
    let sf = MAJOR_SIG
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0, |(_, s)| *s);
    (sf, rotation == 5)
}

// ── writing ─────────────────────────────────────────────────────────────────

fn vlq(mut n: u32) -> Vec<u8> {
    let mut out = vec![(n & 0x7f) as u8];
    n >>= 7;
    while n > 0 {
        out.insert(0, ((n & 0x7f) as u8) | 0x80);
        n >>= 7;
    }
    out
}

fn u32be(n: u32) -> [u8; 4] {
    n.to_be_bytes()
}

struct TimedEvent {
    tick: i64,
    /// Sorts events at the same tick: note-offs before note-ons, end of track last.
    order: u8,
    bytes: Vec<u8>,
}

fn track(mut events: Vec<TimedEvent>) -> Vec<u8> {
    events.sort_by_key(|e| (e.tick, e.order));
    let mut body = vec![];
    let mut now = 0;
    for e in events {
        body.extend(vlq((e.tick - now) as u32));
        body.extend(e.bytes);
        now = e.tick;
    }
    let mut out = b"MTrk".to_vec();
    out.extend(u32be(body.len() as u32));
    out.extend(body);
    out
}

fn meta(tick: i64, kind: u8, data: &[u8], order: u8) -> TimedEvent {
    let mut bytes = vec![0xff, kind];
    bytes.extend(vlq(data.len() as u32));
    bytes.extend(data);
    TimedEvent { tick, order, bytes }
}

fn time_sig(tick: i64, (num, den): Time) -> TimedEvent {
    meta(
        tick,
        0x58,
        &[num as u8, den.trailing_zeros() as u8, 24, 8],
        0,
    )
}

fn key_sig(tick: i64, key: &str) -> TimedEvent {
    let (sf, minor) = key_signature(key);
    meta(tick, 0x59, &[sf as u8, minor as u8], 0)
}

fn tempo(tick: i64, bpm: f64) -> TimedEvent {
    let us = (60_000_000.0 / bpm).round() as u32;
    meta(
        tick,
        0x51,
        &[(us >> 16) as u8, (us >> 8) as u8, us as u8],
        0,
    )
}

pub fn write_midi(song: &Song) -> Vec<u8> {
    let end = song
        .events
        .iter()
        .map(|e| e.end())
        .fold(song.length, i64::max);

    let mut conductor = vec![
        time_sig(0, song.time),
        key_sig(0, &song.key),
        tempo(0, song.tempo),
    ];
    for c in &song.changes {
        if let Some(t) = c.time {
            conductor.push(time_sig(c.tick, t));
        }
        if let Some(k) = &c.key {
            conductor.push(key_sig(c.tick, k));
        }
        if let Some(t) = c.tempo {
            conductor.push(tempo(c.tick, t));
        }
    }
    conductor.push(meta(end, 0x2f, &[], 2));

    let mut out = b"MThd".to_vec();
    out.extend(u32be(6));
    out.extend([0, 1]);
    out.extend(((song.voices.len() + 1) as u16).to_be_bytes());
    out.extend((TICKS_PER_QUARTER as u16).to_be_bytes());
    out.extend(track(conductor));

    for (i, voice) in song.voices.iter().enumerate() {
        let channel = CHANNELS[i % CHANNELS.len()];
        let mut events = vec![meta(0, 0x03, voice.as_bytes(), 0)];
        for e in song.events.iter().filter(|e| &e.voice == voice) {
            events.push(TimedEvent {
                tick: e.start,
                order: 1,
                bytes: vec![0x90 | channel, e.midi as u8, VELOCITY],
            });
            events.push(TimedEvent {
                tick: e.end(),
                order: 0,
                bytes: vec![0x80 | channel, e.midi as u8, 64],
            });
        }
        events.push(meta(end, 0x2f, &[], 2));
        out.extend(track(events));
    }
    out
}

// ── reading ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct MidiNote {
    pub start: u64,
    pub duration: u64,
    pub midi: u8,
    pub velocity: u8,
    pub channel: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiTrack {
    pub name: String,
    pub notes: Vec<MidiNote>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiFile {
    pub format: u16,
    pub division: u16,
    /// Every tempo (µs per quarter note), time, and key signature event, in file order.
    pub tempos: Vec<(u64, u32)>,
    pub times: Vec<(u64, Time)>,
    pub keys: Vec<(u64, i8, bool)>,
    pub length: u64,
    pub tracks: Vec<MidiTrack>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.bytes.get(self.p).ok_or("the file ends early")?;
        self.p += 1;
        Ok(b)
    }
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self
            .bytes
            .get(self.p..self.p + n)
            .ok_or("the file ends early")?;
        self.p += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn vlq(&mut self) -> Result<u32, String> {
        let mut n = 0u32;
        loop {
            let b = self.byte()?;
            n = (n << 7) | (b & 0x7f) as u32;
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
    }
}

/// Reads notes and the tempo, time, and key signature events. Ticks are the file's own.
pub fn read_midi(bytes: &[u8]) -> Result<MidiFile, String> {
    let mut r = Reader { bytes, p: 0 };
    if r.take(4)? != b"MThd" {
        return Err("not a MIDI file".into());
    }
    let header_len = r.u32()? as usize;
    let format = r.u16()?;
    let ntrks = r.u16()?;
    let division = r.u16()?;
    r.p += header_len.saturating_sub(6);
    if division & 0x8000 != 0 {
        return Err("SMPTE time division is not supported".into());
    }
    let mut file = MidiFile {
        format,
        division,
        ..Default::default()
    };
    for t in 0..ntrks {
        if r.p >= bytes.len() {
            break;
        }
        if r.take(4)? != b"MTrk" {
            return Err(format!("track {t}: missing MTrk"));
        }
        let len = r.u32()? as usize;
        let end = r.p + len;
        let mut out = MidiTrack::default();
        let mut open: Vec<(u16, u64, u8)> = vec![];
        let mut tick = 0u64;
        let mut running = 0u8;
        while r.p < end {
            tick += r.vlq()? as u64;
            let mut status = *bytes.get(r.p).ok_or("the file ends early")?;
            if status & 0x80 != 0 {
                r.p += 1;
            } else {
                status = running;
            }
            if status == 0xff {
                let kind = r.byte()?;
                let len = r.vlq()? as usize;
                let data = r.take(len)?;
                match kind {
                    0x03 if out.name.is_empty() => out.name = String::from_utf8_lossy(data).into(),
                    0x51 if data.len() >= 3 => file.tempos.push((
                        tick,
                        (data[0] as u32) << 16 | (data[1] as u32) << 8 | data[2] as u32,
                    )),
                    0x58 if data.len() >= 2 => {
                        file.times.push((tick, (data[0] as u32, 1u32 << data[1])))
                    }
                    0x59 if data.len() >= 2 => file.keys.push((tick, data[0] as i8, data[1] == 1)),
                    0x2f => file.length = file.length.max(tick),
                    _ => {}
                }
            } else if status == 0xf0 || status == 0xf7 {
                let n = r.vlq()? as usize;
                r.p += n;
            } else {
                running = status;
                let kind = status & 0xf0;
                let channel = status & 0x0f;
                let a = r.byte()?;
                let b = if kind == 0xc0 || kind == 0xd0 {
                    0
                } else {
                    r.byte()?
                };
                let id = (channel as u16) << 8 | a as u16;
                if kind == 0x90 && b > 0 {
                    open.push((id, tick, b));
                } else if (kind == 0x80 || kind == 0x90)
                    && let Some(i) = open.iter().position(|o| o.0 == id)
                {
                    let (_, start, velocity) = open.remove(i);
                    out.notes.push(MidiNote {
                        start,
                        duration: tick - start,
                        midi: a,
                        velocity,
                        channel,
                    });
                }
            }
        }
        r.p = end;
        out.notes.sort_by_key(|n| (n.start, n.midi));
        file.tracks.push(out);
    }
    Ok(file)
}
