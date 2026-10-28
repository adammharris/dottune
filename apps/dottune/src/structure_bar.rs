//! The structure bar: bars, blocks, and voices, for wherever the selected
//! note is. Each button (and its key on the roll, with ⌥) runs one of
//! `tune::structure`'s edits and then selects what the edit made or moved.

use dioxus::prelude::*;
use tune::structure as st;
use tune::{EditError, Song};
use unit::Dialog;
use unit::prelude::*;

use crate::app::{App, use_app};

/// Where the selection is: its voice, block, bar, and path.
#[derive(Clone, Debug, PartialEq)]
struct Here {
    voice: String,
    block: usize,
    bar: usize,
    path: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    BarBefore,
    BarAfter,
    BarDup,
    BarLeft,
    BarRight,
    BarDel,
    BlockNew,
    BlockDup,
    BlockUp,
    BlockDown,
    BlockSplit,
    BlockJoin,
    BlockDel,
    VoiceUp,
    VoiceDown,
    VoiceDel,
}

/// What to select after an edit, relative to where it was made: `None`, nothing.
type Then = Option<Here>;

fn n_bars(song: &Song, b: usize) -> usize {
    song.blocks.get(b).map_or(0, |i| i.len())
}

impl Act {
    /// Whether it needs no selection, only a tune.
    fn free(self) -> bool {
        self == Act::BlockNew
    }

    /// Whether it can run here.
    fn can(self, h: &Here, song: &Song) -> bool {
        let vi = song.voice_index(&h.voice).unwrap_or(0);
        match self {
            Act::BarLeft => h.bar > 0,
            Act::BarRight => h.bar + 1 < n_bars(song, h.block),
            Act::BlockUp => h.block > 0,
            Act::BlockDown | Act::BlockJoin => h.block + 1 < song.blocks.len(),
            Act::BlockSplit => h.bar > 0,
            Act::VoiceUp => vi > 0,
            Act::VoiceDown => vi + 1 < song.voices.len(),
            _ => true,
        }
    }

    fn run(self, s: &str, g: &Song, h: &Here) -> Result<String, EditError> {
        let (b, k, v) = (h.block, h.bar, h.voice.as_str());
        match self {
            Act::BarBefore => st::insert_bar(s, g, b, k),
            Act::BarAfter => st::insert_bar(s, g, b, k + 1),
            Act::BarDup => st::duplicate_bar(s, g, b, k),
            Act::BarLeft => st::move_bar(s, g, b, k, -1),
            Act::BarRight => st::move_bar(s, g, b, k, 1),
            Act::BarDel => st::delete_bar(s, g, b, k),
            Act::BlockNew => st::insert_block(s, g, b),
            Act::BlockDup => st::duplicate_block(s, g, b),
            Act::BlockUp => st::move_block(s, g, b, -1),
            Act::BlockDown => st::move_block(s, g, b, 1),
            Act::BlockSplit => st::split_block(s, g, b, k),
            Act::BlockJoin => st::join_blocks(s, g, b),
            Act::BlockDel => st::delete_block(s, g, b),
            Act::VoiceUp => st::move_voice(s, g, v, -1),
            Act::VoiceDown => st::move_voice(s, g, v, 1),
            Act::VoiceDel => st::delete_voice(s, g, v, Some(b)),
        }
    }

    /// Where to select afterwards, from `h` in the song before the edit.
    fn then(self, h: &Here, song: &Song) -> Then {
        let at = |f: &dyn Fn(&mut Here)| {
            let mut n = h.clone();
            f(&mut n);
            Some(n)
        };
        match self {
            Act::BarBefore => at(&|n| n.path = vec![0]),
            Act::BarAfter => at(&|n| {
                n.bar += 1;
                n.path = vec![0];
            }),
            Act::BarDup | Act::BarRight => at(&|n| n.bar += 1),
            Act::BarLeft => at(&|n| n.bar -= 1),
            Act::BarDel => {
                let len = n_bars(song, h.block);
                (len > 1).then(|| Here {
                    bar: h.bar.min(len - 2),
                    path: vec![0],
                    ..h.clone()
                })
            }
            Act::BlockNew => at(&|n| {
                n.block = if song.blocks.is_empty() {
                    0
                } else {
                    h.block + 1
                };
                n.bar = 0;
                n.path = vec![0];
            }),
            Act::BlockDup | Act::BlockDown => at(&|n| n.block += 1),
            Act::BlockUp => at(&|n| n.block -= 1),
            Act::BlockSplit => at(&|n| {
                n.block += 1;
                n.bar = 0;
            }),
            Act::BlockDel | Act::VoiceDel => None,
            Act::BlockJoin | Act::VoiceUp | Act::VoiceDown => Some(h.clone()),
        }
    }
}

/// Keys on the roll, with ⌥: the key's code, ⇧, and what it does.
const KEYS: [(Code, bool, Act); 15] = [
    (Code::KeyB, false, Act::BarAfter),
    (Code::KeyB, true, Act::BarBefore),
    (Code::KeyD, false, Act::BarDup),
    (Code::ArrowLeft, false, Act::BarLeft),
    (Code::ArrowRight, false, Act::BarRight),
    (Code::Backspace, false, Act::BarDel),
    (Code::KeyN, false, Act::BlockNew),
    (Code::KeyD, true, Act::BlockDup),
    (Code::ArrowUp, false, Act::BlockUp),
    (Code::ArrowDown, false, Act::BlockDown),
    (Code::KeyX, false, Act::BlockSplit),
    (Code::KeyJ, false, Act::BlockJoin),
    (Code::Backspace, true, Act::BlockDel),
    (Code::ArrowUp, true, Act::VoiceUp),
    (Code::ArrowDown, true, Act::VoiceDown),
];

fn here(app: &App) -> Option<Here> {
    let song = app.song()?;
    if !app.current() {
        return None;
    }
    let s = &song.slots[app.selected_index()?];
    Some(Here {
        voice: s.voice.clone(),
        block: s.block,
        bar: s.bar,
        path: s.path.clone(),
    })
}

/// Where the free actions apply without a selection: the last block.
fn anywhere(song: &Song) -> Here {
    Here {
        voice: song.voices.first().cloned().unwrap_or_else(|| "RH".into()),
        block: song.blocks.len().saturating_sub(1),
        bar: 0,
        path: vec![0],
    }
}

/// Selects the slot at `h`, or the first in its bar.
fn select_at(app: App, h: &Here) {
    let Some(song) = app.song() else { return };
    let same = |s: &tune::SlotInfo| s.voice == h.voice && s.block == h.block && s.bar == h.bar;
    let i = song
        .slots
        .iter()
        .position(|s| same(s) && s.editable && s.path == h.path)
        .or_else(|| song.slots.iter().position(|s| same(s) && s.editable))
        .or_else(|| song.slots.iter().position(same));
    app.select(i);
}

/// Runs an edit and selects `then` in the result.
fn edit(app: App, op: impl FnOnce(&str, &Song) -> Result<String, EditError>, then: Then) {
    if !app.try_edit(op) {
        return;
    }
    match then {
        Some(h) => select_at(app, &h),
        None => app.select(None),
    }
}

pub fn run(app: App, act: Act) {
    let Some(song) = app.song() else { return };
    if !app.current() {
        return;
    }
    let Some(h) = here(&app).or_else(|| act.free().then(|| anywhere(&song))) else {
        return;
    };
    if !act.can(&h, &song) {
        return;
    }
    let then = act.then(&h, &song);
    edit(app, |s, g| act.run(s, g, &h), then);
}

/// Runs the edit a key on the roll stands for; true if it was one.
pub fn key(app: App, code: &Code, mods: Modifiers) -> bool {
    if !mods.contains(Modifiers::ALT)
        || mods.contains(Modifiers::META)
        || mods.contains(Modifiers::CONTROL)
    {
        return false;
    }
    let shift = mods.contains(Modifiers::SHIFT);
    let Some(&(_, _, act)) = KEYS.iter().find(|(c, s, _)| c == code && *s == shift) else {
        return false;
    };
    run(app, act);
    true
}

/// A name for a new voice: LH or RH if free, else the first of V2, V3… that is.
fn next_voice_name(song: &Song) -> String {
    for v in ["RH", "LH"] {
        if !song.voices.iter().any(|x| x == v) {
            return v.into();
        }
    }
    (2..)
        .map(|i| format!("V{i}"))
        .find(|n| !song.voices.contains(n))
        .unwrap()
}

fn add_voice(app: App, unit: Unit) {
    let Some(song) = app.song() else { return };
    let h = here(&app).unwrap_or_else(|| anywhere(&song));
    let dialog = Dialog::new(format!("New voice in block {}", h.block + 1))
        .message("A voice is a line of the tune: RH, LH, Bass, Melody…")
        .field(next_voice_name(&song), "Name")
        .buttons("Add", "Cancel");
    unit.open_dialog(dialog, move |answer| {
        let unit::Answer::Ok(name) = answer else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let then = Some(Here {
            voice: name.clone(),
            bar: 0,
            path: vec![0],
            ..h.clone()
        });
        let block = h.block;
        edit(app, move |s, g| st::add_voice(s, g, block, &name), then);
    });
}

fn rename_voice(app: App, unit: Unit) {
    let Some(h) = here(&app) else { return };
    let dialog = Dialog::new(format!("Rename {} everywhere", h.voice))
        .field(h.voice.clone(), "Name")
        .buttons("Rename", "Cancel");
    unit.open_dialog(dialog, move |answer| {
        let unit::Answer::Ok(name) = answer else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() || name == h.voice {
            return;
        }
        let then = Some(Here {
            voice: name.clone(),
            ..h.clone()
        });
        let from = h.voice.clone();
        edit(app, move |s, g| st::rename_voice(s, g, &from, &name), then);
    });
}

#[component]
fn ActButton(act: Act, label: String, title: String, glyph: String) -> Element {
    let app = use_app();
    // Read, so the bar follows the selection and the song.
    let song = app.song.read().clone();
    let _ = app.selected.read();
    let _ = app.source.read();
    let at = song
        .as_ref()
        .and_then(|s| here(&app).or_else(|| act.free().then(|| anywhere(s))));
    let enabled = match (&song, &at) {
        (Some(s), Some(h)) => act.can(h, s),
        _ => false,
    };
    rsx! {
        span { title: "{title}",
            Button {
                variant: Variant::Ghost,
                disabled: !enabled,
                refuses_keys: true,
                label: label.clone(),
                onpress: move |_| run(app, act),
                "{glyph}"
            }
        }
    }
}

#[component]
pub fn StructureBar() -> Element {
    let app = use_app();
    let unit = use_unit();
    let song = app.song.read().clone();
    let _ = app.selected.read();
    let _ = app.source.read();
    let h = here(&app);
    let bar_label = h
        .as_ref()
        .map_or("Bar".to_string(), |h| format!("Bar {}", h.bar + 1));
    let block_label = match (&h, &song) {
        (Some(h), Some(s)) if s.blocks.len() > 1 => format!("Block {}", h.block + 1),
        _ => "Block".to_string(),
    };
    let voice_label = h.as_ref().map_or("Voice".to_string(), |h| h.voice.clone());
    let has_song = song.is_some();
    let has_here = h.is_some();

    rsx! {
        div { class: "structure", role: "toolbar", "aria-label": "Structure",
            div { class: "group",
                span { class: "label", "{bar_label}" }
                ActButton { act: Act::BarBefore, glyph: "+◂", label: "Insert a bar before", title: "Insert a bar of rests before this one (⌥⇧B)" }
                ActButton { act: Act::BarAfter, glyph: "▸+", label: "Insert a bar after", title: "Insert a bar of rests after this one (⌥B)" }
                ActButton { act: Act::BarDup, glyph: "⧉", label: "Duplicate the bar", title: "Duplicate this bar, in every voice (⌥D)" }
                ActButton { act: Act::BarLeft, glyph: "◂", label: "Move the bar earlier", title: "Move this bar earlier (⌥←)" }
                ActButton { act: Act::BarRight, glyph: "▸", label: "Move the bar later", title: "Move this bar later (⌥→)" }
                ActButton { act: Act::BarDel, glyph: "✕", label: "Delete the bar", title: "Delete this bar, in every voice (⌥⌫)" }
            }
            div { class: "group",
                span { class: "label", "{block_label}" }
                ActButton { act: Act::BlockNew, glyph: "+", label: "New block", title: "New block after this one (⌥N)" }
                ActButton { act: Act::BlockDup, glyph: "⧉", label: "Duplicate the block", title: "Duplicate this block, change and all (⌥⇧D)" }
                ActButton { act: Act::BlockUp, glyph: "▴", label: "Move the block earlier", title: "Move this block earlier (⌥↑)" }
                ActButton { act: Act::BlockDown, glyph: "▾", label: "Move the block later", title: "Move this block later (⌥↓)" }
                ActButton { act: Act::BlockSplit, glyph: "✂", label: "Split the block", title: "Split the block before this bar (⌥X)" }
                ActButton { act: Act::BlockJoin, glyph: "⊕", label: "Join the next block", title: "Join the next block onto this one (⌥J)" }
                ActButton { act: Act::BlockDel, glyph: "✕", label: "Delete the block", title: "Delete this block (⌥⇧⌫)" }
            }
            div { class: "group",
                span { class: "label", "{voice_label}" }
                span { title: "Add a voice to this block",
                    Button { variant: Variant::Ghost, disabled: !has_song, refuses_keys: true, label: "Add a voice", onpress: move |_| add_voice(app, unit), "+" }
                }
                span { title: "Rename this voice everywhere",
                    Button { variant: Variant::Ghost, disabled: !has_here, refuses_keys: true, label: "Rename the voice", onpress: move |_| rename_voice(app, unit), "Aa" }
                }
                ActButton { act: Act::VoiceUp, glyph: "▴", label: "Move the voice up", title: "Move this voice up (⌥⇧↑)" }
                ActButton { act: Act::VoiceDown, glyph: "▾", label: "Move the voice down", title: "Move this voice down (⌥⇧↓)" }
                ActButton { act: Act::VoiceDel, glyph: "✕", label: "Delete the voice here", title: "Delete this voice from this block" }
            }
        }
    }
}
