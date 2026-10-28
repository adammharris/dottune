//! The header: playing, the view, files, and the examples on one row; on
//! the next, the settings — key, tempo, time, and each voice's octave — for
//! the block the selected note is in (else the first). Changing one writes
//! its directive; × beside one the block writes itself removes it, so the
//! block takes the setting before it (or the default).

use dioxus::prelude::*;
use fig::Value;
use tune::structure::{clear_directive, number, set_directive};
use tune::{Path, Song, path};
use unit::prelude::*;
use unit::{Dialog, Entry};

use crate::app::{App, use_app, voice_color};
use crate::examples::EXAMPLES;

const TONICS: [&str; 14] = [
    "C", "C#", "Db", "D", "Eb", "E", "F", "F#", "Gb", "G", "Ab", "A", "Bb", "B",
];
const MODES: [&str; 9] = [
    "major",
    "minor",
    "dorian",
    "phrygian",
    "lydian",
    "mixolydian",
    "locrian",
    "ionian",
    "aeolian",
];
const TIMES: [&str; 8] = ["2/4", "3/4", "4/4", "5/4", "6/8", "7/8", "9/8", "12/8"];

const KEYS_HELP: &str = "On the roll — press a note to select it and play from there; press twice to write a note there.\n\
↑ ↓ step · ⇧↑ ⇧↓ octave · 1–7 degree · # b accidental\n\
C chord · Q quality · V inversion · + add a third\n\
. rest · - hold · S T split in 2 or 3 · J join · I X insert or delete a slot · ⌥1–⌥9 divide the bar\n\
Drag a note's edges, or ⇧← ⇧→, to change its length · ← → Tab move\n\
Space play · ⌘↵ play from anywhere · ⌘Z ⌘⇧Z undo, redo\n\
⌘-scroll zoom · [ ] block by block · 0 the whole song · L lanes · ⌘\\ fold the text\n\
⇧-press moves the playhead only · ⌥ with the Bar, Block and Voice keys shown on their buttons\n\
⌘S save · ⌘O open";

/// The block the settings edit: the selected note's, else the first.
fn settings_block(app: &App) -> usize {
    match (app.song(), app.selected_index()) {
        (Some(s), Some(i)) => s.slots[i].block,
        _ => 0,
    }
}

fn set_setting(app: App, path: Path, value: Value) {
    let block = settings_block(&app);
    app.try_edit(|src, song| set_directive(src, song, block, &path, value));
}

fn clear(app: App, directive: String) {
    let block = settings_block(&app);
    if app.song().is_some_and(|s| !s.blocks.is_empty()) {
        app.try_edit(|src, song| clear_directive(src, song, block, &directive));
    }
}

/// The settings in force for the settings block: key, tempo, time, and the
/// directives that block writes itself.
struct Settings {
    tonic: String,
    mode: String,
    tempo: f64,
    time: String,
    octaves: Vec<(String, i64)>,
    written: Vec<String>,
    scope: String,
}

fn settings(song: &Song, block: usize) -> Settings {
    let b = song.blocks.get(block);
    let key = b.map_or(song.key.as_str(), |b| b.key.as_str());
    let (tonic, mode) = key.split_once(' ').unwrap_or((key, "major"));
    let time = b.map_or(song.time, |b| b.time);
    let octave = |v: &str| b.map_or_else(|| tune::default_octave(v), |b| b.octave(v));
    Settings {
        tonic: tonic.into(),
        mode: mode.into(),
        tempo: b.map_or(song.tempo, |b| b.tempo),
        time: format!("{}/{}", time.0, time.1),
        octaves: song.voices.iter().map(|v| (v.clone(), octave(v))).collect(),
        written: b.map_or(vec![], |b| b.directives.clone()),
        scope: match song.blocks.len() {
            0 | 1 => String::new(),
            _ if block == 0 => "from the start:".into(),
            _ => format!("from block {}:", block + 1),
        },
    }
}

/// `options`, with `value` added if it is not among them, and where `value` is.
fn choices(options: &[&str], value: &str) -> (Vec<Choice>, Option<usize>) {
    let mut list: Vec<String> = options.iter().map(|s| s.to_string()).collect();
    if !list.iter().any(|o| o == value) {
        list.push(value.to_string());
    }
    let at = list.iter().position(|o| o == value);
    (list.into_iter().map(Choice::new).collect(), at)
}

#[component]
fn Clear(directive: String, block: usize) -> Element {
    let app = use_app();
    let title = if block == 0 {
        "Remove from the header: back to the default".to_string()
    } else {
        format!(
            "Remove from block {}'s change: back to the block before",
            block + 1
        )
    };
    let d = directive.clone();
    rsx! {
        span { title: "{title}",
            Button {
                variant: Variant::Ghost,
                refuses_keys: true,
                label: format!("Remove {directive}"),
                onpress: move |_| clear(app, d.clone()),
                "×"
            }
        }
    }
}

#[component]
fn SettingsRow() -> Element {
    let app = use_app();
    let song = app.song.read().clone();
    let _ = app.selected.read();
    let block = settings_block(&app);
    let s = song.as_ref().map(|s| settings(s, block));
    let mut tempo_text = use_signal(String::new);
    let tempo_now = s.as_ref().map(|s| s.tempo);
    use_effect(use_reactive!(|tempo_now| {
        if let Some(t) = tempo_now {
            tempo_text.set(t.to_string());
        }
    }));
    let Some(s) = s else {
        return rsx! { div { class: "toolbar settings" } };
    };
    let written = |d: &str| s.written.iter().any(|w| w == d);
    let (tonics, tonic_at) = choices(&TONICS, &s.tonic);
    let (modes, mode_at) = choices(&MODES, &s.mode);
    let (times, time_at) = choices(&TIMES, &s.time);
    let tonic_list: Vec<String> = tonics.iter().map(|c| c.label.clone()).collect();
    let mode_list: Vec<String> = modes.iter().map(|c| c.label.clone()).collect();
    let time_list: Vec<String> = times.iter().map(|c| c.label.clone()).collect();
    let (mode, tonic) = (s.mode.clone(), s.tonic.clone());
    let voices = song.as_ref().map_or(vec![], |g| g.voices.clone());

    rsx! {
        div { class: "toolbar settings",
            if !s.scope.is_empty() {
                span { class: "scope", title: "Select a note to edit the settings for its block", "{s.scope}" }
            }
            div { class: "setting",
                span { class: "setting-label", "Key" }
                Select {
                    label: "Tonic",
                    choices: tonics,
                    selected: tonic_at,
                    onpick: move |i: usize| set_setting(app, path!["key"], Value::Str(format!("{} {}", tonic_list[i], mode))),
                }
                Select {
                    label: "Mode",
                    choices: modes,
                    selected: mode_at,
                    onpick: move |i: usize| set_setting(app, path!["key"], Value::Str(format!("{} {}", tonic, mode_list[i]))),
                }
                if written("key") {
                    Clear { directive: "key", block }
                }
            }
            div { class: "setting tempo",
                span { class: "setting-label", "Tempo" }
                TextField {
                    label: "Tempo",
                    kind: InputKind::Number,
                    inline: true,
                    value: tempo_text(),
                    oninput: move |v: String| tempo_text.set(v),
                    onsubmit: move |v: String| match v.trim().parse::<f64>() {
                        Ok(bpm) if bpm > 0.0 => set_setting(app, path!["tempo"], number(bpm)),
                        _ => app.fail(format!("\"{v}\" is not a tempo")),
                    },
                }
                if written("tempo") {
                    Clear { directive: "tempo", block }
                }
            }
            div { class: "setting",
                span { class: "setting-label", "Time" }
                Select {
                    label: "Time signature",
                    choices: times,
                    selected: time_at,
                    onpick: move |i: usize| set_setting(app, path!["time"], Value::Str(time_list[i].clone())),
                }
                if written("time") {
                    Clear { directive: "time", block }
                }
            }
            div { class: "octaves", title: "Octave each part's first note starts in",
                for (i, (v, n)) in s.octaves.iter().enumerate() {
                    div { key: "{v}", class: "setting octave",
                        span { class: "setting-label", style: "color: {voice_color(i)};", "{v}" }
                        Stepper {
                            label: format!("{v}'s octave"),
                            value_text: n.to_string(),
                            can_decrement: *n > -1,
                            can_increment: *n < 9,
                            ondecrement: {
                                let (v, n) = (v.clone(), *n);
                                move |_| set_setting(app, path!["octave", &v], Value::Int(n - 1))
                            },
                            onincrement: {
                                let (v, n) = (v.clone(), *n);
                                move |_| set_setting(app, path!["octave", &v], Value::Int(n + 1))
                            },
                        }
                        span { class: "octave-value", "{n}" }
                        if written(&format!("octave {v}")) {
                            Clear { directive: format!("octave {v}"), block }
                        }
                    }
                }
            }
            if voices.is_empty() {
                span { class: "dim", "No voices yet" }
            }
        }
    }
}

#[component]
pub fn Toolbar() -> Element {
    let app = use_app();
    let unit = use_unit();
    let playing = *app.playing.read();
    let looping = *app.looping.read();
    let lanes = *app.lanes.read();
    let text_open = *app.text_open.read();
    let name = app.name.read().clone();
    let has_song = app.song.read().is_some();
    let examples: Vec<Entry> = EXAMPLES
        .iter()
        .enumerate()
        .map(|(i, (n, _))| unit::Item::new(i, *n).into())
        .collect();

    rsx! {
        header { class: "header",
            div { class: "toolbar",
                span { title: "Play / stop (Space on the roll, ⌘↵ anywhere)",
                    Button {
                        variant: if playing { Variant::Primary } else { Variant::Default },
                        refuses_keys: true,
                        disabled: !has_song,
                        onpress: move |_| app.toggle_play(),
                        if playing { "■ Stop" } else { "▶ Play" }
                    }
                }
                ToggleButton { pressed: looping, refuses_keys: true, onchange: move |on| app.set_looping(on), "Loop" }
                span { title: "A lane for each voice, with its own pitch range (L)",
                    ToggleButton { pressed: lanes, refuses_keys: true, onchange: move |on| app.set_lanes(on), "Lanes" }
                }
                span { title: "Show the text of the tune beside the roll (⌘\\)",
                    ToggleButton { pressed: text_open, refuses_keys: true, onchange: move |on| app.set_text_open(on), "Text" }
                }
                span { class: "sep" }
                div { class: "name", title: "Name for Save and MIDI export",
                    TextField { label: "File name", inline: true, value: name, oninput: move |v: String| app.set_name(&v) }
                }
                span { title: "Open a .tune file (⌘O)",
                    Button { refuses_keys: true, onpress: move |_| app.open(), "Open" }
                }
                span { title: "Save as a .tune file (⌘S)",
                    Button { refuses_keys: true, onpress: move |_| app.save(), "Save" }
                }
                span { title: "Save as a Standard MIDI File",
                    Button { refuses_keys: true, disabled: !has_song, onpress: move |_| app.export_midi(), "⬇ MIDI" }
                }
                span { title: "Copy a link that opens this tune",
                    Button { refuses_keys: true, onpress: move |_| app.share(), "Share" }
                }
                MenuButton {
                    entries: examples,
                    refuses_keys: true,
                    onpick: move |i: usize| {
                        let (n, text) = EXAMPLES[i];
                        app.load(text, n);
                    },
                    "Examples"
                }
                span { class: "spacer" }
                Button {
                    variant: Variant::Ghost,
                    refuses_keys: true,
                    label: "Keys",
                    onpress: move |_| unit.open_dialog(Dialog::new("Keys").message(KEYS_HELP).alert("Done"), |_| {}),
                    "Keys"
                }
            }
            SettingsRow {}
        }
    }
}
