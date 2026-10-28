//! Playback: a song scheduled on an audio clock, a little ahead of it, by a
//! [`Player`] that knows nothing of where the sound goes; a [`Synth`]
//! plays each note it is given. In a window that is a synthesizer of our
//! own on the OS's output (`native.rs`); in a browser, Web Audio
//! (`web.rs`).

use tune::{Song, TempoMap};

#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
use native::Output;

#[cfg(all(feature = "web", not(feature = "native")))]
mod web;
#[cfg(all(feature = "web", not(feature = "native")))]
use web::Output;

/// How far ahead of the clock notes are scheduled.
const LOOKAHEAD_SEC: f64 = 0.12;

/// Where notes sound.
pub trait Synth {
    /// The audio clock, in seconds.
    fn time(&self) -> f64;
    /// Plays `midi` from `when` on the clock, for `dur` seconds, in the
    /// voice of the song's `voice`th part (the first is a little louder).
    fn note(&mut self, midi: i32, when: f64, dur: f64, voice: usize);
    /// The same, outside playback, right away: feedback for an edit.
    fn preview(&mut self, midi: i32, dur: f64, voice: usize);
    /// Silences every note scheduled so far, quickly, without a click.
    fn fade_out(&mut self);
}

struct Timed {
    start: f64,
    duration: f64,
    midi: i32,
    voice: usize,
}

/// Schedules a song on the audio clock. Position is tracked in "unwrapped"
/// song seconds — laps × song length + seconds into the song — so looping
/// is modular arithmetic, tempo changes are the tempo map's business, and
/// the song can be swapped while playing (live editing) without restarting.
pub struct Player {
    out: Option<Output>,
    map: Option<TempoMap>,
    length: i64,
    timed: Vec<Timed>,
    playing: bool,
    /// Clock time at which unwrapped second `anchor_sec` plays.
    anchor_time: f64,
    anchor_sec: f64,
    /// Everything before this unwrapped second has been scheduled.
    cursor: f64,
    /// Where playback resumes, in ticks.
    paused_at: f64,
    pub looping: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            out: None,
            map: None,
            length: 0,
            timed: vec![],
            playing: false,
            anchor_time: 0.0,
            anchor_sec: 0.0,
            cursor: 0.0,
            paused_at: 0.0,
            looping: true,
        }
    }
}

impl Player {
    pub fn playing(&self) -> bool {
        self.playing
    }

    fn total(&self) -> f64 {
        self.map.as_ref().map_or(0.0, |m| m.total).max(0.001)
    }

    fn output(&mut self) -> Option<&mut Output> {
        if self.out.is_none() {
            self.out = Output::open();
        }
        self.out.as_mut()
    }

    /// The position within the song, in ticks.
    pub fn position(&self) -> f64 {
        match (&self.out, &self.map) {
            (Some(out), Some(map)) if self.playing => {
                let u = self.anchor_sec + (out.time() - self.anchor_time);
                let sec = if self.looping {
                    u.rem_euclid(self.total())
                } else {
                    u.min(self.total())
                };
                map.ticks(sec)
            }
            _ => self.paused_at,
        }
    }

    /// Plays `song` from now on, from where playback is, in its place.
    pub fn set_song(&mut self, song: &Song) {
        let pos = self.position();
        let map = TempoMap::new(song);
        self.timed = song
            .events
            .iter()
            .map(|e| {
                let start = map.seconds(e.start as f64);
                Timed {
                    start,
                    duration: map.seconds(e.end() as f64) - start,
                    midi: e.midi,
                    voice: song.voice_index(&e.voice).unwrap_or(0),
                }
            })
            .collect();
        let now = self.out.as_ref().map(|o| o.time());
        // Seconds already scheduled past now, under the old map: keep them.
        let ahead = match now {
            Some(t) if self.playing => self.cursor - (self.anchor_sec + t - self.anchor_time),
            _ => 0.0,
        };
        self.length = song.length;
        let tick = if pos >= song.length as f64 { 0.0 } else { pos };
        let anchor = map.seconds(tick);
        self.map = Some(map);
        match now {
            Some(t) if self.playing => {
                self.anchor_time = t;
                self.anchor_sec = anchor;
                self.cursor = anchor + ahead.max(0.0);
            }
            _ => self.paused_at = tick,
        }
    }

    pub fn play(&mut self) {
        if self.playing || self.map.is_none() {
            return;
        }
        let Some(t) = self.output().map(|o| o.time()) else {
            return;
        };
        self.restart_from(t, self.paused_at);
        self.playing = true;
        self.schedule();
    }

    pub fn stop(&mut self) {
        if !self.playing {
            return;
        }
        self.paused_at = self.position();
        self.playing = false;
        if let Some(out) = self.out.as_mut() {
            out.fade_out();
        }
    }

    /// Moves the playhead. Playback, if running, carries on from the new place.
    pub fn seek(&mut self, tick: f64) {
        self.paused_at = tick.min((self.length - 1).max(0) as f64).max(0.0);
        if !self.playing {
            return;
        }
        let Some(out) = self.out.as_mut() else { return };
        out.fade_out();
        let t = out.time();
        self.restart_from(t, self.paused_at);
        self.schedule();
    }

    fn restart_from(&mut self, now: f64, tick: f64) {
        self.anchor_time = now + 0.05;
        self.anchor_sec = self.map.as_ref().map_or(0.0, |m| m.seconds(tick));
        self.cursor = self.anchor_sec;
    }

    /// Schedules what falls within the lookahead; stops at the end unless
    /// looping. Called every few milliseconds while playing.
    pub fn schedule(&mut self) {
        if !self.playing {
            return;
        }
        let total = self.total();
        let Some(now) = self.out.as_ref().map(|o| o.time()) else {
            return;
        };
        let end = self.anchor_sec + (now + LOOKAHEAD_SEC - self.anchor_time);
        if !self.looping && self.cursor >= total {
            if self.anchor_sec + (now - self.anchor_time) >= total {
                self.stop();
                self.paused_at = 0.0;
            }
            return;
        }
        let limit = if self.looping { end } else { end.min(total) };
        let mut lap = (self.cursor / total).floor();
        let out = self.out.as_mut().unwrap();
        while lap * total < limit {
            let offset = lap * total;
            for e in &self.timed {
                let u = e.start + offset;
                if u < self.cursor || u >= limit {
                    continue;
                }
                out.note(
                    e.midi,
                    self.anchor_time + (u - self.anchor_sec),
                    e.duration,
                    e.voice,
                );
            }
            lap += 1.0;
        }
        self.cursor = self.cursor.max(limit);
    }

    /// Sounds notes right away, outside playback: feedback for an edit.
    pub fn preview(&mut self, midis: &[i32], voice: usize) {
        if let Some(out) = self.output() {
            for &m in midis {
                out.preview(m, 0.4, voice);
            }
        }
    }
}
