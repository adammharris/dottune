//! In a browser: Web Audio. Each note is a few oscillators through a
//! closing lowpass and an envelope, into an output that a stop or seek
//! replaces, so what was scheduled on the old one can be faded out.

use web_sys::{AudioContext, AudioNode, BiquadFilterType, GainNode, OscillatorType};

use super::Synth;

const RELEASE: f64 = 0.25;

pub struct Output {
    ctx: AudioContext,
    compressor: AudioNode,
    /// Where playback goes; replaced on every fade-out.
    master: Option<GainNode>,
    preview: GainNode,
}

impl Output {
    pub fn open() -> Option<Self> {
        let ctx = AudioContext::new().ok()?;
        let _ = ctx.resume();
        let compressor = ctx.create_dynamics_compressor().ok()?;
        compressor
            .connect_with_audio_node(&ctx.destination())
            .ok()?;
        let preview = ctx.create_gain().ok()?;
        preview.gain().set_value(0.4);
        preview.connect_with_audio_node(&ctx.destination()).ok()?;
        Some(Self {
            ctx,
            compressor: compressor.into(),
            master: None,
            preview,
        })
    }

    fn master(&mut self) -> Option<GainNode> {
        if self.master.is_none() {
            let g = self.ctx.create_gain().ok()?;
            g.gain().set_value(0.5);
            g.connect_with_audio_node(&self.compressor).ok()?;
            self.master = Some(g);
        }
        self.master.clone()
    }

    /// A soft electric-piano-ish voice: triangle + octave sine through a closing lowpass.
    fn sound(&self, out: &AudioNode, midi: i32, when: f64, dur: f64, voice: usize) -> Option<()> {
        let ctx = &self.ctx;
        let t = when.max(ctx.current_time());
        let freq = 440.0 * 2f64.powf((midi - 69) as f64 / 12.0);

        let env = ctx.create_gain().ok()?;
        let g = env.gain();
        g.set_value_at_time(0.0, t).ok()?;
        g.linear_ramp_to_value_at_time(if voice == 0 { 0.22 } else { 0.15 }, t + 0.006)
            .ok()?;
        g.set_target_at_time(0.05, t + 0.006, 0.6).ok()?;
        g.set_target_at_time(0.0, t + dur, RELEASE / 4.0).ok()?;

        let filter = ctx.create_biquad_filter().ok()?;
        filter.set_type(BiquadFilterType::Lowpass);
        filter
            .frequency()
            .set_value_at_time((freq * 8.0).min(9000.0) as f32, t)
            .ok()?;
        filter
            .frequency()
            .set_target_at_time((freq * 3.0).min(4000.0) as f32, t, 0.3)
            .ok()?;
        filter.connect_with_audio_node(&env).ok()?;
        env.connect_with_audio_node(out).ok()?;

        for (kind, ratio, level) in [
            (OscillatorType::Triangle, 1.0, 1.0),
            (OscillatorType::Sine, 2.0, 0.25),
            (OscillatorType::Sine, 1.003, 0.4),
        ] {
            let osc = ctx.create_oscillator().ok()?;
            osc.set_type(kind);
            osc.frequency().set_value((freq * ratio) as f32);
            let level_node = ctx.create_gain().ok()?;
            level_node.gain().set_value(level);
            osc.connect_with_audio_node(&level_node).ok()?;
            level_node.connect_with_audio_node(&filter).ok()?;
            osc.start_with_when(t).ok()?;
            osc.stop_with_when(t + dur + RELEASE).ok()?;
        }
        Some(())
    }
}

impl Synth for Output {
    fn time(&self) -> f64 {
        self.ctx.current_time()
    }

    fn note(&mut self, midi: i32, when: f64, dur: f64, voice: usize) {
        if let Some(master) = self.master() {
            self.sound(&master, midi, when, dur, voice);
        }
    }

    fn preview(&mut self, midi: i32, dur: f64, voice: usize) {
        let _ = self.ctx.resume();
        let out: AudioNode = self.preview.clone().into();
        let now = self.ctx.current_time();
        self.sound(&out, midi, now, dur, voice);
    }

    fn fade_out(&mut self) {
        if let Some(master) = self.master.take() {
            let now = self.ctx.current_time();
            let _ = master.gain().set_target_at_time(0.0, now, 0.02);
            // Disconnected once the gain is down: at once would click.
            let later = wasm_bindgen::closure::Closure::once_into_js(move || {
                let _ = master.disconnect();
            });
            if let Some(w) = web_sys::window() {
                use wasm_bindgen::JsCast;
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    later.unchecked_ref(),
                    200,
                );
            }
        }
    }
}
