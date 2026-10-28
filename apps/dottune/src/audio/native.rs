//! In a window: a small synthesizer on the OS's audio output, through cpal.
//! The voice is the browser's: a triangle and two sines through a lowpass
//! that closes as the note sounds, a soft electric piano.

use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

use super::Synth;

const ATTACK: f32 = 0.006;
const RELEASE: f32 = 0.25;
/// How fast a fade-out silences what was playing.
const FADE: f32 = 0.02;
const MASTER: f32 = 0.5;
const PREVIEW: f32 = 0.4;

struct Voice {
    freq: f32,
    /// When it starts and lets go, in samples on the output's clock.
    start: u64,
    off: u64,
    peak: f32,
    gain: f32,
    /// Part of playback (faded out on a stop or seek), not a preview.
    playback: bool,
    /// Faded out from this sample, by a stop or seek.
    killed: Option<u64>,
    phase: [f32; 3],
    env: f32,
    lowpass: f32,
}

struct State {
    rate: f32,
    /// Samples written to the output so far.
    clock: u64,
    /// When the clock last moved, to read it between callbacks.
    at: Instant,
    voices: Vec<Voice>,
}

impl State {
    fn time(&self) -> f64 {
        let since = self.at.elapsed().as_secs_f64().min(0.05);
        self.clock as f64 / self.rate as f64 + since
    }

    fn sample_at(&self, t: f64) -> u64 {
        (t.max(0.0) * self.rate as f64) as u64
    }

    fn add(&mut self, midi: i32, when: f64, dur: f64, voice: usize, gain: f32, playback: bool) {
        let start = self.sample_at(when).max(self.clock);
        self.voices.push(Voice {
            freq: 440.0 * 2f32.powf((midi - 69) as f32 / 12.0),
            start,
            off: start + (dur.max(0.0) * self.rate as f64) as u64,
            peak: if voice == 0 { 0.22 } else { 0.15 },
            gain,
            playback,
            killed: None,
            phase: [0.0; 3],
            env: 0.0,
            lowpass: 0.0,
        });
    }

    /// The next sample of every voice, mixed; voices that have died away are dropped.
    fn next(&mut self) -> f32 {
        let rate = self.rate;
        let now = self.clock;
        let decay = (-1.0 / (0.6 * rate)).exp();
        let release = (-1.0 / (RELEASE / 4.0 * rate)).exp();
        let fade = (-1.0 / (FADE / 4.0 * rate)).exp();
        let mut out = 0.0;
        for v in &mut self.voices {
            if now < v.start {
                continue;
            }
            let t = (now - v.start) as f32 / rate;
            v.env = if v.killed.is_some() {
                v.env * fade
            } else if now >= v.off {
                v.env * release
            } else if t < ATTACK {
                v.peak * t / ATTACK
            } else {
                0.05 + (v.env - 0.05) * decay
            };
            let tri = 1.0 - 4.0 * (v.phase[0] - 0.5).abs();
            let s2 = (v.phase[1] * TAU).sin() * 0.25;
            let s3 = (v.phase[2] * TAU).sin() * 0.4;
            for (p, ratio) in v.phase.iter_mut().zip([1.0, 2.0, 1.003]) {
                *p = (*p + v.freq * ratio / rate).fract();
            }
            let hi = (v.freq * 8.0).min(9000.0);
            let lo = (v.freq * 3.0).min(4000.0);
            let cutoff = lo + (hi - lo) * (-t / 0.3).exp();
            let a = 1.0 - (-TAU * cutoff / rate).exp();
            v.lowpass += a * (tri + s2 + s3 - v.lowpass);
            out += v.lowpass * v.env * v.gain;
        }
        self.voices.retain(|v| {
            let gone = v
                .killed
                .is_some_and(|k| now > k + (FADE * 4.0 * rate) as u64);
            let ended = now > v.off + (RELEASE * 2.0 * rate) as u64;
            !(gone || ended) || now < v.start
        });
        self.clock += 1;
        // A soft limit, for the compressor the browser's voice goes through.
        out.tanh()
    }
}

pub struct Output {
    state: Arc<Mutex<State>>,
    _stream: cpal::Stream,
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    state: Arc<Mutex<State>>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let mut s = state.lock().unwrap();
            for frame in data.chunks_mut(channels) {
                let v = T::from_sample(s.next());
                frame.fill(v);
            }
            s.at = Instant::now();
        },
        |e| eprintln!("audio: {e}"),
        None,
    )
}

impl Output {
    /// The default output device, or `None` if there is none to play on.
    pub fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let state = Arc::new(Mutex::new(State {
            rate: config.sample_rate.0 as f32,
            clock: 0,
            at: Instant::now(),
            voices: vec![],
        }));
        let s = state.clone();
        let stream = match format {
            cpal::SampleFormat::F32 => stream::<f32>(&device, &config, s),
            cpal::SampleFormat::I16 => stream::<i16>(&device, &config, s),
            cpal::SampleFormat::U16 => stream::<u16>(&device, &config, s),
            cpal::SampleFormat::I32 => stream::<i32>(&device, &config, s),
            other => {
                eprintln!("audio: no support for {other} samples");
                return None;
            }
        }
        .map_err(|e| eprintln!("audio: {e}"))
        .ok()?;
        stream.play().map_err(|e| eprintln!("audio: {e}")).ok()?;
        Some(Self {
            state,
            _stream: stream,
        })
    }
}

impl Synth for Output {
    fn time(&self) -> f64 {
        self.state.lock().unwrap().time()
    }

    fn note(&mut self, midi: i32, when: f64, dur: f64, voice: usize) {
        self.state
            .lock()
            .unwrap()
            .add(midi, when, dur, voice, MASTER, true);
    }

    fn preview(&mut self, midi: i32, dur: f64, voice: usize) {
        let mut s = self.state.lock().unwrap();
        let now = s.time();
        s.add(midi, now, dur, voice, PREVIEW, false);
    }

    fn fade_out(&mut self) {
        let mut s = self.state.lock().unwrap();
        let now = s.clock;
        // What has not started yet never will; what has fades away.
        s.voices.retain(|v| !v.playback || v.start <= now);
        for v in s.voices.iter_mut().filter(|v| v.playback) {
            v.killed.get_or_insert(now);
        }
    }
}
