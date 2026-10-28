//! Ticks to seconds and back, for a song whose tempo changes between blocks.

use crate::types::{Song, TICKS_PER_QUARTER};

#[derive(Clone, Copy, Debug)]
struct Segment {
    tick: f64,
    sec: f64,
    ticks_per_sec: f64,
}

#[derive(Clone, Debug)]
pub struct TempoMap {
    segments: Vec<Segment>,
    /// Seconds from the start to the end of the song.
    pub total: f64,
}

impl TempoMap {
    pub fn new(song: &Song) -> Self {
        let tempos: Vec<(i64, f64)> = if song.blocks.is_empty() {
            vec![(0, song.tempo)]
        } else {
            song.blocks.iter().map(|b| (b.start, b.tempo)).collect()
        };
        let mut segments: Vec<Segment> = vec![];
        let mut sec = 0.0;
        for (tick, bpm) in tempos {
            let ticks_per_sec = bpm / 60.0 * TICKS_PER_QUARTER as f64;
            if let Some(prev) = segments.last() {
                if ticks_per_sec == prev.ticks_per_sec {
                    continue;
                }
                sec = prev.sec + (tick as f64 - prev.tick) / prev.ticks_per_sec;
            }
            segments.push(Segment {
                tick: tick as f64,
                sec,
                ticks_per_sec,
            });
        }
        let mut map = TempoMap {
            segments,
            total: 0.0,
        };
        map.total = map.seconds(song.length as f64);
        map
    }

    pub fn seconds(&self, tick: f64) -> f64 {
        let s = self
            .segments
            .iter()
            .rev()
            .find(|s| s.tick <= tick)
            .unwrap_or(&self.segments[0]);
        s.sec + (tick - s.tick) / s.ticks_per_sec
    }

    pub fn ticks(&self, sec: f64) -> f64 {
        let s = self
            .segments
            .iter()
            .rev()
            .find(|s| s.sec <= sec)
            .unwrap_or(&self.segments[0]);
        s.tick + (sec - s.sec) * s.ticks_per_sec
    }
}
