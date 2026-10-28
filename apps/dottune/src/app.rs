//! What the parts of the page share: the source, the song as last compiled,
//! the selection, the view, the player, and the actions that change them.
//! Each part reaches the one [`App`] through context, so none needs
//! another's internals.

use std::rc::Rc;

use dioxus::prelude::*;
use tune::{EditError, SlotKey, Song, TuneError};

use crate::audio::Player;
use crate::geometry::{self, View};
use crate::history::History;
use crate::platform;

/// What the status line says.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Status {
    pub text: String,
    pub error: bool,
}

/// While a note's edge is dragged: the song it would make, and the note to
/// show selected in it.
#[derive(Clone, PartialEq)]
pub struct Preview {
    pub song: Rc<Song>,
    pub selected: Option<usize>,
}

#[derive(Clone, Copy)]
pub struct App {
    pub source: Signal<String>,
    /// Last song that compiled; kept playing while the source has errors.
    pub song: Signal<Option<Rc<Song>>>,
    /// The text `song` was compiled from; slot spans are only valid while it matches the source.
    pub compiled: Signal<String>,
    /// Every error in the source as of the last compile; empty when it compiled.
    pub errors: Signal<Vec<TuneError>>,
    /// The text `errors` were found in.
    pub error_source: Signal<String>,
    /// The selected slot, by where it is written, so it survives a recompile.
    pub selected: Signal<Option<SlotKey>>,
    pub status: Signal<Status>,
    pub name: Signal<String>,
    pub view: Signal<Option<View>>,
    pub lanes: Signal<bool>,
    pub text_open: Signal<bool>,
    pub looping: Signal<bool>,
    pub playing: Signal<bool>,
    /// The playhead, in ticks.
    pub position: Signal<f64>,
    pub preview: Signal<Option<Preview>>,
    pub player: CopyValue<Player>,
    pub history: CopyValue<History>,
    /// Bumped by every keystroke in the text pane; a recompile waits for it to settle.
    typed: CopyValue<u64>,
}

pub fn use_app() -> App {
    use_context::<App>()
}

/// A voice's colour on the roll, in its lane label, and in the status line.
pub fn voice_color(i: usize) -> &'static str {
    const COLORS: [&str; 6] = [
        "#7aa2f7", "#e0af68", "#9ece6a", "#f7768e", "#bb9af7", "#7dcfff",
    ];
    COLORS[i % COLORS.len()]
}

fn flag(key: &str, default: bool) -> bool {
    platform::load(key).map_or(default, |v| v.trim() != "0")
}

/// Makes the page's [`App`], opened on the tune it was given, else the last
/// one edited, else the first example.
pub fn use_app_provider() -> App {
    use_context_provider(|| {
        let startup = platform::startup();
        let stored = platform::load("source");
        let first = crate::examples::EXAMPLES[0];
        let source = stored.clone().unwrap_or_else(|| first.1.to_string());
        let name = platform::load("name").unwrap_or_else(|| {
            if stored.is_none() {
                first.0.to_string()
            } else {
                "tune".into()
            }
        });
        let app = App {
            source: Signal::new(source.clone()),
            song: Signal::new(None),
            compiled: Signal::new(String::new()),
            errors: Signal::new(vec![]),
            error_source: Signal::new(String::new()),
            selected: Signal::new(None),
            status: Signal::new(Status::default()),
            name: Signal::new(name),
            view: Signal::new(None),
            lanes: Signal::new(flag("lanes", true)),
            text_open: Signal::new(flag("text", true)),
            looping: Signal::new(true),
            playing: Signal::new(false),
            position: Signal::new(0.0),
            preview: Signal::new(None),
            player: CopyValue::new(Player::default()),
            history: CopyValue::new(History::default()),
            typed: CopyValue::new(0),
        };
        app.history.write_unchecked().reset(&source);
        app.recompile();
        if let Some(opened) = startup.opened {
            // What was here before stays one undo away.
            let replaced = stored.is_some_and(|s| s != opened.source);
            app.load(&opened.source, &opened.name);
            if replaced {
                app.say("Opened a shared tune · ⌘Z brings back what you had");
            }
        } else if startup.damaged_link {
            app.fail("That share link is damaged, so it could not be opened");
        }
        app
    })
}

impl App {
    // ── the source ──────────────────────────────────────────────────────

    /// Compiles the source and shows the result everywhere.
    pub fn recompile(mut self) {
        let source = self.source.peek().clone();
        platform::save("source", &source);
        match tune::compile(&source) {
            Ok(song) => {
                let song = Rc::new(song);
                self.player.write_unchecked().set_song(&song);
                self.compiled.set(source);
                self.errors.set(vec![]);
                self.song.set(Some(song));
                self.describe();
            }
            Err(errors) => {
                self.errors.set(errors.0);
                self.error_source.set(source);
                self.describe_errors();
            }
        }
    }

    /// The source was typed in the text pane.
    pub fn typed(mut self, text: String) {
        self.history.write_unchecked().typed(&text, platform::now());
        self.source.set(text);
        let n = {
            let mut t = self.typed.write_unchecked();
            *t += 1;
            *t
        };
        spawn(async move {
            platform::sleep(150).await;
            if *self.typed.peek() == n {
                self.recompile();
            }
        });
    }

    /// Replaces the source with `next`, as the text pane will show it, and recompiles.
    fn replace(mut self, next: String) {
        self.source.set(next);
        self.recompile();
    }

    /// Replaces the source with the result of an edit, recording it for undo.
    pub fn commit(self, next: String) {
        let current = self.source.peek().clone();
        if self.history.write_unchecked().commit(&current, &next) {
            self.replace(next);
        }
    }

    pub fn undo(self) {
        let current = self.source.peek().clone();
        if let Some(prev) = self.history.write_unchecked().undo(&current) {
            self.replace(prev);
        }
    }

    pub fn redo(self) {
        let current = self.source.peek().clone();
        if let Some(next) = self.history.write_unchecked().redo(&current) {
            self.replace(next);
        }
    }

    /// Whether the song on show is what the source says, so its slots can be edited.
    pub fn current(&self) -> bool {
        *self.compiled.peek() == *self.source.peek()
    }

    /// The song, if the source has compiled.
    pub fn song(&self) -> Option<Rc<Song>> {
        self.song.peek().clone()
    }

    /// Runs an edit of the current song and records it for undo; a refusal
    /// (fig rolled it back) is reported, not returned.
    pub fn try_edit(self, f: impl FnOnce(&str, &Song) -> Result<String, EditError>) -> bool {
        let Some(song) = self.song() else {
            return false;
        };
        if !self.current() {
            self.fail("The text has errors, so it cannot be edited from here");
            return false;
        }
        let source = self.source.peek().clone();
        match f(&source, &song) {
            Ok(next) => {
                self.commit(next);
                true
            }
            Err(e) => {
                self.fail(format!("edit refused: {e}"));
                false
            }
        }
    }

    /// Replaces the tune with another, as one undoable step.
    pub fn load(self, text: &str, name: &str) {
        self.select(None);
        self.commit(text.to_string());
        self.set_name(name);
    }

    pub fn set_name(mut self, name: &str) {
        let name = name
            .trim_end_matches(".tune")
            .trim_end_matches(".txt")
            .to_string();
        platform::save("name", &name);
        self.name.set(name);
    }

    /// The name Save and MIDI export use, without an extension.
    pub fn file_name(&self) -> String {
        let cleaned: String = self
            .name
            .peek()
            .trim()
            .chars()
            .map(|c| if "\\/:*?\"<>|".contains(c) { '-' } else { c })
            .collect();
        if cleaned.is_empty() {
            "tune".into()
        } else {
            cleaned
        }
    }

    // ── the selection ───────────────────────────────────────────────────

    /// The selected slot, as an index into the song's slots.
    pub fn selected_index(&self) -> Option<usize> {
        let song = self.song.peek();
        let key = self.selected.peek();
        song.as_ref()?.slot_at(key.as_ref()?)
    }

    /// The same, read so a component redraws when it changes.
    pub fn read_selected(&self) -> Option<usize> {
        let song = self.song.read();
        let key = self.selected.read();
        song.as_ref()?.slot_at(key.as_ref()?)
    }

    pub fn select(mut self, index: Option<usize>) {
        let song = self.song();
        let key = index.zip(song.as_ref()).map(|(i, s)| s.slots[i].key());
        if *self.selected.peek() != key {
            self.selected.set(key);
        }
        if let (Some(i), Some(s)) = (index, &song) {
            let slot = &s.slots[i];
            let length = s.length as f64;
            let v = geometry::reveal(
                *self.view.peek(),
                length,
                slot.start as f64,
                slot.end as f64,
            );
            if v != *self.view.peek() {
                self.view.set(v);
            }
        }
        self.describe();
    }

    /// Selects the slot written at `key`, if there still is one.
    pub fn select_key(self, key: &SlotKey) {
        let i = self.song().and_then(|s| s.slot_at(key));
        self.select(i);
    }

    // ── the status line ─────────────────────────────────────────────────

    pub fn say(mut self, text: impl Into<String>) {
        self.status.set(Status {
            text: text.into(),
            error: false,
        });
    }

    pub fn fail(mut self, text: impl Into<String>) {
        self.status.set(Status {
            text: text.into(),
            error: true,
        });
    }

    /// Puts the selection, or the song's summary, in the status line.
    pub fn describe(self) {
        if !self.errors.peek().is_empty() {
            return self.describe_errors();
        }
        let Some(song) = self.song() else { return };
        match self.selected_index() {
            Some(i) => {
                let s = &song.slots[i];
                let mut where_ = format!("{} · block {} · bar {}", s.voice, s.block + 1, s.bar + 1);
                if s.path.len() > 1 {
                    let p: Vec<String> = s.path.iter().map(|p| (p + 1).to_string()).collect();
                    where_ += &format!(" · {}", p.join("."));
                }
                self.say(format!("{}   {where_}", s.text));
            }
            None => self.say(format!(
                "{} notes · {}",
                song.events.len(),
                song.voices.join(" ")
            )),
        }
    }

    /// Where an error is, as "line 3, col 7".
    pub fn where_is(&self, e: &TuneError) -> String {
        match e.span {
            Some((start, _)) => {
                let src = self.error_source.peek();
                let line_start = src[..start.min(src.len())].rfind('\n').map_or(0, |i| i + 1);
                let col = src[line_start..start.min(src.len())].chars().count() + 1;
                format!("line {}, col {col}", e.line)
            }
            None => format!("line {}", e.line),
        }
    }

    /// Puts the first error, and how many more there are, in the status line.
    pub fn describe_errors(self) {
        let errors = self.errors.peek().clone();
        let Some(first) = errors.first() else { return };
        let more = if errors.len() > 1 {
            format!("   (+{} more in the text)", errors.len() - 1)
        } else {
            String::new()
        };
        self.fail(format!("{}: {}{more}", self.where_is(first), first.reason));
    }

    // ── playback ────────────────────────────────────────────────────────

    pub fn toggle_play(mut self) {
        if *self.playing.peek() {
            self.player.write_unchecked().stop();
            self.playing.set(false);
            return;
        }
        self.player.write_unchecked().play();
        let playing = self.player.peek().playing();
        self.playing.set(playing);
        if !playing {
            self.fail("There is no audio output to play on");
            return;
        }
        // While it plays: schedule ahead, move the playhead, turn the page.
        spawn(async move {
            let mut last = *self.position.peek();
            while *self.playing.peek() {
                let (tick, still) = {
                    let mut p = self.player.write_unchecked();
                    p.schedule();
                    (p.position(), p.playing())
                };
                if !still {
                    self.playing.set(false);
                }
                self.position.set(tick);
                if let Some(song) = self.song() {
                    let v = geometry::follow(*self.view.peek(), song.length as f64, last, tick);
                    if v != *self.view.peek() {
                        self.view.set(v);
                    }
                }
                last = tick;
                platform::sleep(16).await;
            }
            let tick = self.player.peek().position();
            self.position.set(tick);
        });
    }

    pub fn seek(mut self, tick: f64) {
        self.player.write_unchecked().seek(tick);
        let tick = self.player.peek().position();
        self.position.set(tick);
    }

    pub fn set_looping(mut self, on: bool) {
        self.player.write_unchecked().looping = on;
        self.looping.set(on);
    }

    /// Sounds a slot's notes, unless playback is sounding everything already.
    pub fn audition(self, index: usize) {
        let Some(song) = self.song() else { return };
        let s = &song.slots[index];
        if !s.midi.is_empty() && !*self.playing.peek() {
            let voice = song.voice_index(&s.voice).unwrap_or(0);
            self.player.write_unchecked().preview(&s.midi, voice);
        }
    }

    // ── the view ────────────────────────────────────────────────────────

    pub fn set_lanes(mut self, on: bool) {
        platform::save("lanes", if on { "1" } else { "0" });
        self.lanes.set(on);
    }

    pub fn set_text_open(mut self, open: bool) {
        platform::save("text", if open { "1" } else { "0" });
        self.text_open.set(open);
    }

    // ── files ───────────────────────────────────────────────────────────

    pub fn open(self) {
        platform::open_file(move |opened| self.load(&opened.source, &opened.name));
    }

    pub fn save(self) {
        let text = self.source.peek().clone();
        if let Err(e) = platform::save_file(
            &format!("{}.tune", self.file_name()),
            "text/plain",
            text.into_bytes(),
        ) {
            self.fail(e);
        }
    }

    pub fn export_midi(self) {
        let Some(song) = self.song() else { return };
        let bytes = tune::midi::write_midi(&song);
        if let Err(e) =
            platform::save_file(&format!("{}.mid", self.file_name()), "audio/midi", bytes)
        {
            self.fail(e);
        }
    }

    pub fn share(self) {
        let hash = tune::share::share_hash(&self.source.peek(), &self.file_name());
        let url = platform::share_url(&hash);
        match platform::copy_text(&url) {
            Ok(()) if platform::COPIES_LINKS => self.say("Link copied to the clipboard"),
            _ => self.say(format!("Share this link: {url}")),
        }
    }
}
