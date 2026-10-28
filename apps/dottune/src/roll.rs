//! The piano roll: a keyboard down the left edge, an overview strip across
//! the top, and the song's notes, in a lane per voice or one for all. Drawn
//! as positioned elements, so it is the same in a window and a browser, at
//! the size the page gives it; where things are is `geometry`'s. One layer
//! over it all takes the pointer and the keys: selecting and editing notes,
//! dragging their edges, and moving the view.

use std::rc::Rc;

use dioxus::prelude::*;
use tune::edit::{self, Cut};
use tune::splice::apply_token_edits;
use tune::{SlotInfo, SlotKey, Song, TICKS_PER_QUARTER, TokenEdit};
use unit::prelude::*;

use crate::app::{App, Preview, use_app, voice_color};
use crate::geometry::{self, KEYBOARD_W, LANE_GAP, Layout, STRIP_H, Side};
use crate::platform;
use crate::structure_bar;

const BLACK: [i32; 5] = [1, 3, 6, 8, 10];
const ACCENT: &str = "#ff9e64";
const CHANGE: &str = "#bb9af7";

/// The overview strip's shade for block `i`.
fn band(i: usize) -> &'static str {
    if i % 2 == 1 { "#1f2030" } else { "#24263a" }
}

/// A key's colour: the voice sounding it, else white or black.
fn key_color(lit: Option<usize>, m: i32) -> &'static str {
    match lit {
        Some(v) => voice_color(v),
        None if black(m) => "#2a2b3a",
        None => "#c0caf5",
    }
}

fn black(m: i32) -> bool {
    BLACK.contains(&m.rem_euclid(12))
}

/// A drag in progress.
#[derive(Clone)]
enum Drag {
    /// The overview strip: the view follows the pointer, `span` wide.
    Strip { span: f64 },
    /// A note's edge.
    Resize(Box<Resize>),
}

#[derive(Clone)]
struct Resize {
    song: Rc<Song>,
    src: String,
    index: usize,
    voice: String,
    side: Side,
    /// Where the note starts, as the drag has it.
    start: f64,
    /// The source the drag would make.
    next: Option<String>,
}

/// The roll at `width` × `height` CSS pixels.
#[component]
pub fn Roll(width: f64, height: f64) -> Element {
    let app = use_app();
    let unit = use_unit();
    let focus = unit::use_focus(unit::Kind::Widget, |_, _| false);
    let mut drag = use_hook(|| CopyValue::new(None::<Drag>));
    let mut cursor = use_signal(|| "default");
    // The last press, for a double one: when and where.
    let mut last_press = use_hook(|| CopyValue::new((f64::NEG_INFINITY, 0.0, 0.0)));

    let preview = app.preview.read().clone();
    let shown_song = preview
        .as_ref()
        .map(|p| p.song.clone())
        .or_else(|| app.song.read().clone());
    let stale = preview.is_none() && !app.errors.read().is_empty();
    let selected = match &preview {
        Some(p) => p.selected,
        None => app.read_selected(),
    };
    let lanes_on = *app.lanes.read();
    let view = *app.view.read();

    // Which notes sound at the playhead, by index: redrawn only when that changes.
    let sounding = use_memo(move || {
        let tick = *app.position.read();
        let song = app
            .preview
            .read()
            .as_ref()
            .map(|p| p.song.clone())
            .or_else(|| app.song.read().clone());
        song.map_or(vec![], |s| {
            s.events
                .iter()
                .enumerate()
                .filter(|(_, e)| tick >= e.start as f64 && tick < e.end() as f64)
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        })
    });

    let layout_now = move || -> Option<(Rc<Song>, Layout)> {
        let song = app.song()?;
        if song.length == 0 {
            return None;
        }
        let l = geometry::layout(&song, width, height, *app.lanes.peek(), *app.view.peek());
        Some((song, l))
    };

    let onpointerdown = move |e: PointerEvent| {
        focus.press();
        let p = e.element_coordinates();
        let (x, y) = (p.x, p.y);
        let mods = e.modifiers();
        let now = platform::now();
        let (t0, x0, y0) = *last_press.peek();
        let double = now - t0 < 0.35 && (x - x0).abs() < 5.0 && (y - y0).abs() < 5.0;
        last_press.set(if double {
            (f64::NEG_INFINITY, 0.0, 0.0)
        } else {
            (now, x, y)
        });
        let Some((song, l)) = layout_now() else {
            return;
        };
        if double {
            return double_press(app, &song, &l, x, y);
        }

        // The overview strip: drag to move the view.
        if let Some(tick) = l.strip_at(x, y) {
            let span = l.view.span();
            if span >= song.length as f64 {
                return;
            }
            centre_on(app, &song, tick, span);
            drag.set(Some(Drag::Strip { span }));
            return;
        }

        // A note's edge, while the roll shows what the source says.
        if !mods.contains(Modifiers::SHIFT)
            && app.current()
            && let Some(edge) = l.edge_at(&song, x, y)
        {
            app.select(Some(edge.event.slot));
            let s = &song.slots[edge.event.slot];
            drag.set(Some(Drag::Resize(Box::new(Resize {
                song: song.clone(),
                src: app.source.peek().clone(),
                index: edge.event.slot,
                voice: s.voice.clone(),
                side: edge.side,
                start: s.start as f64,
                next: None,
            }))));
            return;
        }

        // A press moves the playhead: to the start of the note pressed, else to the press.
        // Shift-press moves it without changing the selection.
        let Some(pt) = l.point_at(x, y) else { return };
        if mods.contains(Modifiers::SHIFT) {
            app.seek(pt.tick);
            return;
        }
        let slot = geometry::slot_at(&song, pt.tick, pt.midi, pt.voice.as_deref());
        app.seek(slot.map_or(pt.tick, |i| song.slots[i].start as f64));
        app.select(slot);
        if let Some(i) = slot {
            app.audition(i);
        }
    };

    let onpointermove = move |e: PointerEvent| {
        let p = e.element_coordinates();
        let (x, y) = (p.x, p.y);
        let Some((song, l)) = layout_now() else {
            return;
        };
        let current = drag.peek().clone();
        match current {
            Some(Drag::Strip { span }) => {
                if let Some(tick) = l.strip_at(x, 0.0) {
                    centre_on(app, &song, tick, span);
                }
            }
            Some(Drag::Resize(mut r)) => {
                resize_move(app, &l, &mut r, x, e.modifiers().contains(Modifiers::ALT));
                drag.set(Some(Drag::Resize(r)));
            }
            None => {
                let want = if l.edge_at(&song, x, y).is_some() {
                    "ew-resize"
                } else {
                    "default"
                };
                if *cursor.peek() != want {
                    cursor.set(want);
                }
            }
        }
    };

    let onpointerup = move |_: PointerEvent| {
        if let Some(Drag::Resize(r)) = drag.take() {
            resize_end(app, *r);
        }
    };

    let onpointerleave = move |_: PointerEvent| {
        if let Some(Drag::Resize(_)) = drag.take() {
            resize_cancel(app);
        }
    };

    // ⌘-scroll zooms about the pointer, scrolling pans.
    let onwheel = move |e: WheelEvent| {
        let Some((song, l)) = layout_now() else {
            return;
        };
        e.prevent_default();
        let (dx, dy) = match e.delta() {
            dioxus::html::geometry::WheelDelta::Pixels(v) => (v.x, v.y),
            dioxus::html::geometry::WheelDelta::Lines(v) => (v.x * 16.0, v.y * 16.0),
            dioxus::html::geometry::WheelDelta::Pages(v) => (v.x * 400.0, v.y * 400.0),
        };
        let mods = e.modifiers();
        let mut view = app.view;
        let cur = *view.peek();
        let length = song.length as f64;
        if mods.contains(Modifiers::CONTROL) || mods.contains(Modifiers::META) {
            let tick = l.tick_at(e.element_coordinates().x);
            view.set(geometry::zoom(cur, length, (-dy * 0.01).exp(), tick));
        } else {
            let px = if dx.abs() > dy.abs() { dx } else { dy };
            let ticks = px / l.roll_w * l.view.span();
            view.set(geometry::pan(cur, length, ticks));
        }
    };

    let onkeydown = move |e: KeyboardEvent| {
        if unit.is_open() {
            return;
        }
        if e.key() == Key::Named(NamedKey::Escape) && drag.peek().is_some() {
            if let Some(Drag::Resize(_)) = drag.take() {
                resize_cancel(app);
            }
            e.prevent_default();
            e.stop_propagation();
            return;
        }
        let mods = e.modifiers();
        let handled = structure_bar::key(app, &e.code(), mods)
            || view_key(app, &e.key(), mods)
            || edit_key(app, &e.key(), &e.code(), mods);
        if handled {
            e.prevent_default();
            e.stop_propagation();
        }
    };

    let Some(song) = shown_song.filter(|s| s.length > 0) else {
        return rsx! {
            div { class: "roll", style: "width: {width}px; height: {height}px;",
                div { class: "roll-empty", "Write a voice line in the text, or press + in the Block bar, to start." }
            }
        };
    };
    let l = geometry::layout(&song, width, height, lanes_on, view);
    let sounding = sounding.read();
    let on = |i: usize| sounding.binary_search(&i).is_ok();
    let voice_of = |v: &str| song.voice_index(v).unwrap_or(0);
    let clip_w = l.roll_w;
    let clip_h = height - STRIP_H;
    // Positions inside the clipped part, whose corner is (KEYBOARD_W, STRIP_H).
    let cx = |t: f64| l.x_of(t) - KEYBOARD_W;
    let cy = |lane: &geometry::Lane, m: i32| l.y_of(lane, m) - STRIP_H;

    // Which keys sound, by lane and pitch: the voice sounding each.
    let lane_index = |voice: &str| {
        l.lanes
            .iter()
            .position(|ln| ln.voice.as_deref() == Some(voice))
            .unwrap_or(0)
    };
    let lit_keys: std::collections::HashMap<(usize, i32), usize> = sounding
        .iter()
        .filter_map(|&i| song.events.get(i))
        .map(|e| ((lane_index(&e.voice), e.midi), voice_of(&e.voice)))
        .collect();
    let lit = |li: usize, m: i32| lit_keys.get(&(li, m)).copied();

    // Beat and bar lines, in each block's own time signature; when zoomed in, the beats' halves too.
    let mut lines: Vec<(f64, &str)> = vec![];
    for b in &song.blocks {
        if (b.end as f64) < l.view.start || b.start as f64 > l.view.end {
            continue;
        }
        let beat = (TICKS_PER_QUARTER * 4) as f64 / b.time.1 as f64;
        let bar = beat * b.time.0 as f64;
        let step = if l.width_of(beat) >= 96.0 {
            beat / 2.0
        } else {
            beat
        };
        let start = b.start as f64;
        let mut t = start + ((l.view.start - start) / step).floor().max(0.0) * step;
        while t < (b.end as f64).min(l.view.end + step) {
            let rel = t - start;
            let color = if rel % bar == 0.0 {
                "#3b3d57"
            } else if rel % beat == 0.0 {
                "#22232f"
            } else {
                "#1c1d28"
            };
            lines.push((cx(t).round(), color));
            t += step;
        }
    }

    let sel = selected.and_then(|i| song.slots.get(i));
    let span = (l.hi - l.lo).max(1) as f64;
    let length = song.length as f64;
    let multi = l.lanes.len() > 1;
    let changes: Vec<(f64, String)> = song
        .changes
        .iter()
        .map(|c| {
            let mut parts = vec![];
            if let Some(k) = &c.key {
                parts.push(k.clone());
            }
            if let Some(t) = c.tempo {
                parts.push(format!("♩={t}"));
            }
            if let Some((n, d)) = c.time {
                parts.push(format!("{n}/{d}"));
            }
            (cx(c.tick as f64).round(), parts.join(" · "))
        })
        .collect();
    let note_style = |i: usize, e: &tune::NoteEvent| {
        let strong = on(i) || Some(e.slot) == selected;
        let alpha = (if stale { 0.45 } else { 1.0 }) * (if strong { 1.0 } else { 0.7 });
        format!(
            "left: {}px; top: {}px; width: {}px; height: {}px; border-radius: {}px; background: {}; opacity: {alpha};",
            cx(e.start as f64),
            cy(l.lane_of(&e.voice), e.midi) + 1.0,
            (cx(e.end() as f64) - cx(e.start as f64) - 1.0).max(2.0),
            (l.row_h - 2.0).max(2.0),
            (l.row_h / 3.0).min(3.0),
            voice_color(voice_of(&e.voice)),
        )
    };
    let note_label = |e: &tune::NoteEvent| format!("{} {}", e.voice, tune::pitch_name(e.midi));

    rsx! {
        div {
            class: "roll",
            style: "width: {width}px; height: {height}px;",
            "aria-hidden": "true",

            // The whole song in miniature, its blocks numbered, and the part in view outlined.
            div { class: "strip", style: "width: {width}px; height: {STRIP_H}px;",
                for (i, b) in song.blocks.iter().enumerate() {
                    div {
                        key: "b{i}",
                        class: "strip-block",
                        style: "left: {l.strip_x(b.start as f64)}px; width: {l.strip_x(b.end as f64) - l.strip_x(b.start as f64)}px; background: {band(i)};",
                        if song.blocks.len() > 1 && l.strip_x(b.end as f64) - l.strip_x(b.start as f64) > 14.0 {
                            "{i + 1}"
                        }
                    }
                }
                for (i, e) in song.events.iter().enumerate() {
                    div {
                        key: "m{i}",
                        class: "strip-note",
                        style: "left: {l.strip_x(e.start as f64)}px; top: {2.0 + (l.hi - e.midi) as f64 / span * (STRIP_H - 5.0)}px; width: {(l.strip_x(e.end() as f64) - l.strip_x(e.start as f64) - 0.5).max(1.0)}px; background: {voice_color(voice_of(&e.voice))};",
                    }
                }
                if view.is_some() {
                    div {
                        class: "strip-view",
                        style: "left: {l.strip_x(l.view.start).round()}px; width: {(l.strip_x(l.view.end) - l.strip_x(l.view.start) - 1.0).max(3.0)}px; height: {STRIP_H - 2.0}px;",
                    }
                }
            }

            div {
                class: "roll-clip",
                style: "left: {KEYBOARD_W}px; top: {STRIP_H}px; width: {clip_w}px; height: {clip_h}px;",
                // Row shading for black keys, and the gaps between lanes.
                for (li, lane) in l.lanes.iter().enumerate() {
                    for m in (lane.lo..=lane.hi).filter(|&m| black(m)) {
                        div {
                            key: "r{li}-{m}",
                            class: "row-black",
                            style: "top: {cy(lane, m)}px; height: {l.row_h}px; width: {clip_w}px;",
                        }
                    }
                }
                for (i, (x, color)) in lines.iter().enumerate() {
                    div { key: "l{i}", class: "beat", style: "left: {x}px; height: {clip_h}px; background: {color};" }
                }
                for (li, lane) in l.lanes.iter().enumerate().skip(1) {
                    div {
                        key: "g{li}",
                        class: "lane-gap",
                        style: "top: {lane.top - LANE_GAP - STRIP_H}px; height: {LANE_GAP}px; width: {clip_w}px;",
                    }
                }
                // The selected slot's span, behind the notes: in its own lane.
                if let Some(s) = sel {
                    div {
                        class: "sel-span",
                        style: "left: {cx(s.start as f64)}px; width: {cx(s.end as f64) - cx(s.start as f64)}px; top: {l.lane_of(&s.voice).top - STRIP_H}px; height: {l.lane_of(&s.voice).bottom - l.lane_of(&s.voice).top}px; background: {voice_color(voice_of(&s.voice))};",
                    }
                }
                for (i, e) in song.events.iter().enumerate().filter(|(_, e)| e.end() as f64 >= l.view.start && e.start as f64 <= l.view.end) {
                    div {
                        key: "n{i}",
                        class: if Some(e.slot) == selected { "note selected" } else if on(i) { "note on" } else { "note" },
                        title: note_label(e),
                        style: note_style(i, e),
                    }
                }
                if multi {
                    for (li, lane) in l.lanes.iter().enumerate() {
                        div {
                            key: "v{li}",
                            class: "lane-label",
                            style: "top: {lane.bottom - STRIP_H - 17.0}px; color: {voice_color(voice_of(lane.voice.as_deref().unwrap_or_default()))};",
                            "{lane.voice.clone().unwrap_or_default()}"
                        }
                    }
                }
                // Changes between blocks: a marker and what changed.
                for (i, (x, label)) in changes.iter().enumerate() {
                    div { key: "c{i}", class: "change", style: "left: {x}px; height: {clip_h}px; background: {CHANGE};",
                        div { class: "change-label", "{label}" }
                    }
                }
                Playhead { layout: l.clone(), height: clip_h }
            }

            // A keyboard for each lane.
            for (li, lane) in l.lanes.iter().enumerate() {
                for m in lane.lo..=lane.hi {
                    div {
                        key: "k{li}-{m}",
                        class: "key",
                        style: "top: {l.y_of(lane, m)}px; height: {l.row_h - 0.5}px; width: {KEYBOARD_W - 4.0}px; background: {key_color(lit(li, m), m)};",
                        if m.rem_euclid(12) == 0 && l.row_h >= 8.0 {
                            span { class: "key-label", style: "font-size: {(l.row_h - 1.0).min(10.0)}px; line-height: {l.row_h}px;", "C{m / 12 - 1}" }
                        }
                    }
                }
            }
            StripHead { layout: l.clone(), length }

            // What the pointer and keys reach: the roll's own layer, over all of it.
            div {
                class: "roll-input{focus.class()}",
                style: "width: {width}px; height: {height}px; cursor: {cursor};",
                tabindex: "0",
                role: "application",
                "aria-label": "Piano roll",
                onpointerdown,
                onpointermove,
                onpointerup,
                onpointerleave,
                onwheel,
                onkeydown,
                onfocus: move |_| focus.dom_focus(),
                onblur: move |_| focus.dom_blur(),
            }
        }
    }
}

/// The playhead across the notes, drawn on its own so the rest of the roll
/// is not redrawn as it moves.
#[component]
fn Playhead(layout: Layout, height: f64) -> Element {
    let app = use_app();
    let x = (layout.x_of(*app.position.read()) - KEYBOARD_W).round();
    rsx! {
        div { class: "playhead", style: "left: {x}px; height: {height}px; background: {ACCENT};" }
    }
}

/// The playhead's mark on the overview strip.
#[component]
fn StripHead(layout: Layout, length: f64) -> Element {
    let app = use_app();
    let x = layout.strip_x(app.position.read().min(length)).round();
    rsx! {
        div { class: "strip-head", style: "left: {x}px; height: {STRIP_H}px; background: {ACCENT};" }
    }
}

// ── the view ────────────────────────────────────────────────────────────────

/// Centres the view on `tick`, `span` wide.
fn centre_on(app: App, song: &Song, tick: f64, span: f64) {
    let mut view = app.view;
    view.set(geometry::fit(
        song.length as f64,
        tick - span / 2.0,
        tick + span / 2.0,
    ));
}

/// Whether the view is exactly block `b`.
fn fitted(app: App, song: &Song, start: i64, end: i64) -> bool {
    let v = geometry::shown(*app.view.peek(), song.length as f64);
    (v.start - start as f64).abs() < 1.0 && (v.end - end as f64).abs() < 1.0
}

fn show(app: App, song: &Song, start: i64, end: i64) {
    let mut view = app.view;
    view.set(geometry::fit(song.length as f64, start as f64, end as f64));
}

/// [ and ]: the previous or next block; from a view that is not a block, the block at its left edge.
fn step_block(app: App, dir: i64) {
    let Some(song) = app.song() else { return };
    if song.blocks.is_empty() {
        return;
    }
    let v = geometry::shown(*app.view.peek(), song.length as f64);
    let i = song
        .blocks
        .iter()
        .position(|b| v.start + 1.0 >= b.start as f64 && v.start + 1.0 < b.end as f64)
        .unwrap_or(0);
    let b = &song.blocks[i];
    let next = if fitted(app, &song, b.start, b.end) {
        i as i64 + dir
    } else {
        i as i64
    };
    if let Some(b) = usize::try_from(next).ok().and_then(|n| song.blocks.get(n)) {
        show(app, &song, b.start, b.end);
    }
}

/// A double press: on the strip, fits the block there (or, fitted, the
/// whole song); on the roll, writes a note of that pitch in the slot there.
fn double_press(app: App, song: &Song, l: &Layout, x: f64, y: f64) {
    if let Some(tick) = l.strip_at(x, y) {
        let block = song
            .blocks
            .iter()
            .find(|b| tick >= b.start as f64 && tick < b.end as f64);
        let mut view = app.view;
        match block {
            Some(b) if !fitted(app, song, b.start, b.end) => show(app, song, b.start, b.end),
            _ => view.set(None),
        }
        return;
    }
    let Some(p) = l.point_at(x, y) else { return };
    if !app.current() {
        return;
    }
    let Some(index) = geometry::slot_at(song, p.tick, p.midi, p.voice.as_deref()) else {
        return;
    };
    app.select(Some(index));
    edit_selected(
        app,
        |s, i| edit::set_pitch(s, i, p.midi),
        None::<fn(&SlotInfo) -> SlotKey>,
    );
}

/// Keys that move the view; true if it was one.
fn view_key(app: App, key: &Key, mods: Modifiers) -> bool {
    let Some(song) = app.song() else { return false };
    let length = song.length as f64;
    let mut view = app.view;
    if mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL) {
        let cur = *view.peek();
        let v = geometry::shown(cur, length);
        let at = app
            .selected_index()
            .map_or((v.start + v.end) / 2.0, |i| song.slots[i].start as f64);
        let Key::Character(c) = key else { return false };
        match c.as_str() {
            "=" | "+" => view.set(geometry::zoom(cur, length, 1.5, at)),
            "-" => view.set(geometry::zoom(cur, length, 1.0 / 1.5, at)),
            "0" => view.set(None),
            _ => return false,
        }
        return true;
    }
    let Key::Character(c) = key else { return false };
    match c.as_str() {
        "[" => step_block(app, -1),
        "]" => step_block(app, 1),
        "0" => view.set(None),
        "l" | "L" => app.set_lanes(!*app.lanes.peek()),
        _ => return false,
    }
    true
}

// ── editing the selection ───────────────────────────────────────────────────

/// Applies token edits to the selected slot and sounds what it now sounds;
/// `then` says what to select after, from the slot as it was.
fn edit_selected(
    app: App,
    op: impl Fn(&Song, usize) -> Vec<TokenEdit>,
    then: Option<impl FnOnce(&SlotInfo) -> SlotKey>,
) {
    let (Some(song), Some(sel)) = (app.song(), app.selected_index()) else {
        return;
    };
    let s = song.slots[sel].clone();
    if op(&song, sel).is_empty() {
        return;
    }
    if !app.try_edit(|src, song| apply_token_edits(src, &op(song, sel))) {
        return;
    }
    if let Some(then) = then {
        app.select_key(&then(&s));
    }
    if let Some(i) = app.selected_index() {
        app.audition(i);
    }
}

/// Shift+←/→: shrinks or grows the selected note by a slot.
fn lengthen_selected(app: App, dir: i32) {
    let (Some(song), Some(sel)) = (app.song(), app.selected_index()) else {
        return;
    };
    let Some(index) = edit::onset_of(&song, sel) else {
        return;
    };
    let s = song.slots[index].clone();
    let edits = edit::lengthen(&song, index, dir);
    if edits.is_empty() || !app.try_edit(|src, _| apply_token_edits(src, &edits)) {
        return;
    }
    if let Some(now) = app.song() {
        app.select(edit::onset_at(&now, &s.voice, s.start));
    }
}

/// The neighbouring editable slot in the same voice.
fn neighbour(song: &Song, sel: usize, dir: i64) -> Option<usize> {
    let voice = &song.slots[sel].voice;
    let mut i = sel as i64 + dir;
    while i >= 0 && (i as usize) < song.slots.len() {
        let s = &song.slots[i as usize];
        if &s.voice != voice {
            return None;
        }
        if s.editable {
            return Some(i as usize);
        }
        i += dir;
    }
    None
}

/// The slot sounding at the same time in the next (or previous) voice.
fn other_voice(song: &Song, sel: usize, dir: i64) -> Option<usize> {
    let cur = &song.slots[sel];
    let n = song.voices.len() as i64;
    let vi = song.voice_index(&cur.voice)? as i64;
    let voice = &song.voices[(vi + dir).rem_euclid(n) as usize];
    song.slots
        .iter()
        .position(|s| &s.voice == voice && s.editable && cur.start >= s.start && cur.start < s.end)
}

fn with_path(s: &SlotInfo, path: Vec<usize>) -> SlotKey {
    SlotKey { path, ..s.key() }
}

/// Keys that play, or edit and move the selection; true if it was one.
fn edit_key(app: App, key: &Key, code: &Code, mods: Modifiers) -> bool {
    if mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL) {
        return false;
    }
    if matches!(key, Key::Character(c) if c == " ") {
        app.toggle_play();
        return true;
    }
    let (Some(song), Some(sel)) = (app.song(), app.selected_index()) else {
        return false;
    };
    let shift = mods.contains(Modifiers::SHIFT);
    // ⌥1–9: the selected note's bar, re-divided into that many slots.
    if mods.contains(Modifiers::ALT) {
        let digit = match code {
            Code::Digit1 => 1,
            Code::Digit2 => 2,
            Code::Digit3 => 3,
            Code::Digit4 => 4,
            Code::Digit5 => 5,
            Code::Digit6 => 6,
            Code::Digit7 => 7,
            Code::Digit8 => 8,
            Code::Digit9 => 9,
            _ => 0,
        };
        if digit > 0 {
            edit_selected(
                app,
                |s, i| edit::divide_bar(s, i, digit),
                Some(|s: &SlotInfo| with_path(s, vec![0])),
            );
            return true;
        }
    }
    let steps = if shift { 7 } else { 1 };
    let none = None::<fn(&SlotInfo) -> SlotKey>;
    match key {
        Key::Named(NamedKey::ArrowUp) => {
            edit_selected(app, |s, i| edit::move_by(s, i, steps), none)
        }
        Key::Named(NamedKey::ArrowDown) => {
            edit_selected(app, |s, i| edit::move_by(s, i, -steps), none)
        }
        Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight) => {
            let dir = if *key == Key::Named(NamedKey::ArrowLeft) {
                -1
            } else {
                1
            };
            if shift {
                lengthen_selected(app, dir);
            } else if let Some(n) = neighbour(&song, sel, dir as i64) {
                app.select(Some(n));
            }
        }
        Key::Named(NamedKey::Tab) => {
            if let Some(n) = other_voice(&song, sel, if shift { -1 } else { 1 }) {
                app.select(Some(n));
            }
        }
        Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete) => {
            edit_selected(app, edit::to_rest, none)
        }
        Key::Named(NamedKey::Escape) => app.select(None),
        Key::Character(c) => {
            let first = |s: &SlotInfo| {
                let mut p = s.path.clone();
                p.push(0);
                with_path(s, p)
            };
            match c.as_str() {
                "." => edit_selected(app, edit::to_rest, none),
                "-" => edit_selected(app, edit::to_hold, none),
                "s" | "/" => edit_selected(app, |s, i| edit::split(s, i, 2), Some(first)),
                "t" => edit_selected(app, |s, i| edit::split(s, i, 3), Some(first)),
                "j" => edit_selected(
                    app,
                    edit::join_group,
                    Some(|s: &SlotInfo| {
                        with_path(s, s.path[..s.path.len().saturating_sub(1)].to_vec())
                    }),
                ),
                "i" => edit_selected(
                    app,
                    edit::insert_slot,
                    Some(|s: &SlotInfo| {
                        let mut p = s.path.clone();
                        if let Some(last) = p.last_mut() {
                            *last += 1;
                        }
                        with_path(s, p)
                    }),
                ),
                "x" => edit_selected(app, edit::delete_slot, none),
                "c" => edit_selected(app, edit::cycle_chord, none),
                "q" => edit_selected(app, edit::cycle_quality, none),
                "v" => edit_selected(app, edit::cycle_inversion, none),
                "+" | "=" => edit_selected(app, edit::add_third, none),
                "#" => edit_selected(app, |s, i| edit::toggle_accidental(s, i, "#"), none),
                "b" => edit_selected(app, |s, i| edit::toggle_accidental(s, i, "b"), none),
                d if d.len() == 1 && ("1"..="7").contains(&d) => {
                    let degree: i64 = d.parse().unwrap();
                    edit_selected(app, |s, i| edit::set_degree(s, i, degree - 1), none);
                }
                _ => return false,
            }
        }
        _ => return false,
    }
    true
}

// ── dragging a note's edges ─────────────────────────────────────────────────
//
// Dragging a note's right edge changes its length; its left edge, where it
// starts. The edge snaps to the slot under the pointer, split into halves
// (thirds with ⌥) as finely as the zoom allows; the roll and status line
// show the result until the button is let go.

/// How many equal parts a slot may split into: powers of 2 (or 3), each at
/// least 14px wide, up to 3 deep.
fn parts_for(l: &Layout, base: usize, s: &SlotInfo) -> usize {
    let mut n = 1;
    while n < base.pow(3) && l.width_of((s.end - s.start) as f64 / (n * base) as f64) >= 14.0 {
        n *= base;
    }
    n
}

/// A bar of `src` as written, brackets and all.
fn bar_text(song: &Song, src: &str, block: usize, voice: &str, bar: usize) -> String {
    let spans: Vec<(usize, usize)> = song
        .slots
        .iter()
        .filter(|s| s.voice == voice && s.block == block && s.bar == bar)
        .filter_map(|s| s.span)
        .collect();
    let (Some(from), Some(to)) = (
        spans.iter().map(|s| s.0).min(),
        spans.iter().map(|s| s.1).max(),
    ) else {
        return String::new();
    };
    let bytes = src.as_bytes();
    let (mut from, mut to) = (from, to);
    while from > 0 && (bytes[from - 1] == b'[' || bytes[from - 1].is_ascii_whitespace()) {
        from -= 1;
    }
    while to < bytes.len() && (bytes[to] == b']' || bytes[to].is_ascii_whitespace()) {
        to += 1;
    }
    src[from..to].trim().to_string()
}

fn resize_move(app: App, l: &Layout, r: &mut Resize, x: f64, thirds: bool) {
    let mut preview = app.preview;
    let tick = l.tick_at(x);
    let base = if thirds { 3 } else { 2 };
    let parts = |s: &SlotInfo| parts_for(l, base, s);
    let song = &r.song;
    let cut: Option<Cut> = match r.side {
        Side::End => edit::snap_cut(song, r.index, tick, parts),
        Side::Start => edit::snap_start(song, r.index, tick, parts),
    };
    let edits = match (cut, r.side) {
        (Some(c), Side::End) => edit::resize(song, r.index, c),
        (Some(c), Side::Start) => edit::restart(song, r.index, c),
        _ => vec![],
    };
    r.next = None;
    r.start = match (cut, r.side) {
        (Some(c), Side::Start) => edit::cut_tick(song, c),
        _ => song.slots[r.index].start as f64,
    };
    if edits.is_empty() {
        preview.set(None);
        app.describe();
        return;
    }
    let Ok(text) = apply_token_edits(&r.src, &edits) else {
        preview.set(None);
        app.describe();
        return;
    };
    let Ok(next) = tune::compile(&text) else {
        preview.set(None);
        app.describe();
        return;
    };
    let mut bars: Vec<(usize, String, usize)> = vec![];
    for e in &edits {
        let key = match (&e.path[1], &e.path[2], &e.path[3]) {
            (tune::Seg::Index(b), tune::Seg::Key(v), tune::Seg::Index(bar)) => {
                (*b, v.clone(), *bar)
            }
            _ => continue,
        };
        if !bars.contains(&key) {
            bars.push(key);
        }
    }
    let shown: Vec<String> = bars
        .iter()
        .map(|(b, v, bar)| format!("bar {}: {}", bar + 1, bar_text(&next, &text, *b, v, *bar)))
        .collect();
    app.say(format!(
        "{} → {}   (⌥ thirds · esc cancels)",
        r.voice,
        shown.join(" · ")
    ));
    let selected = edit::onset_at(&next, &r.voice, r.start.round() as i64);
    preview.set(Some(Preview {
        song: Rc::new(next),
        selected,
    }));
    r.next = Some(text);
}

fn resize_end(app: App, r: Resize) {
    let mut preview = app.preview;
    preview.set(None);
    match r.next {
        Some(text) => {
            app.commit(text);
            if let Some(song) = app.song() {
                app.select(edit::onset_at(&song, &r.voice, r.start.round() as i64));
            }
        }
        None => app.describe(),
    }
}

fn resize_cancel(app: App) {
    let mut preview = app.preview;
    preview.set(None);
    app.describe();
}
