//! The tune format (SPEC.md): a plain-text music format where spacing is
//! time, notes are scale degrees, and every note says its own octave.
//!
//! - [`compile`] turns a tune into a [`Song`]: its event list (SPEC §6), and
//!   where every token sits and sounds.
//! - [`edit`] and [`structure`] are the edits an editor makes, as fig
//!   splices ([`splice`], [`figlang`]), so the text keeps its shape.
//! - [`midi`] writes a song as a Standard MIDI File; [`tempo`] maps ticks to
//!   seconds; [`share`] puts a tune in a link.

pub mod compile;
pub mod edit;
pub mod figlang;
pub mod midi;
pub mod share;
pub mod splice;
pub mod structure;
pub mod tempo;
pub mod theory;
pub mod types;

pub use compile::{compile, default_octave, format_events, format_song, is_voice_name};
pub use splice::{EditError, Path, Seg, TokenEdit};
pub use tempo::TempoMap;
pub use theory::pitch_name;
pub use types::*;
