#![allow(clippy::useless_vec)] // `path![…]` is a Vec, borrowed where the editor takes a slice.

//! Port of test/structure.test.ts.

use fig::Value;
use tune::splice::{Ed, EditError, apply};
use tune::structure::{
    add_voice, clear_directive, delete_bar, delete_block, delete_voice, duplicate_bar,
    duplicate_block, insert_bar, insert_block, join_blocks, move_bar, move_block, move_voice,
    rename_voice, set_directive, split_block,
};
use tune::{Song, compile, format_song, path};

fn run(source: &str, op: impl FnOnce(&str, &Song) -> Result<String, EditError>) -> String {
    op(source, &compile(source).unwrap()).unwrap()
}

/// The error message of an edit that should be refused.
fn refused(source: &str, op: impl FnOnce(&str, &Song) -> Result<String, EditError>) -> String {
    match op(source, &compile(source).unwrap()) {
        Err(e) => e.0,
        Ok(out) => panic!("expected an error, got {out:?}"),
    }
}

fn edits(source: &str, f: impl FnOnce(&mut Ed) -> Result<(), EditError>) -> String {
    apply(source, f).unwrap()
}

/// The tune's events from block `b` on, shifted to start at 0.
fn from(source: &str, b: usize) -> Vec<String> {
    let song = compile(source).unwrap();
    let t = song.blocks[b].start;
    song.events
        .iter()
        .filter(|e| e.start >= t)
        .map(|e| format!("{} {} {} {}", e.start - t, e.duration, e.voice, e.midi))
        .collect()
}

// ── the language ────────────────────────────────────────────────────────────

#[test]
fn fig_edits_a_voices_bars_and_a_bars_tokens_item_by_item() {
    let src = "RH: 1 2 3 4 | 5 | // tune\nLH: I |\n";
    assert_eq!(
        edits(src, |ed| ed.insert_item(
            &path!["blocks", 0, "RH"],
            1,
            "6 7"
        )),
        "RH: 1 2 3 4 | 6 7 | 5 | // tune\nLH: I |\n"
    );
    // TS's appendValueRaw: an insert at the end.
    assert_eq!(
        edits(src, |ed| ed.insert_item(&path!["blocks", 0, "RH"], 2, "7")),
        "RH: 1 2 3 4 | 5 | 7 | // tune\nLH: I |\n"
    );
    assert_eq!(
        edits(src, |ed| ed.delete_item(&path!["blocks", 0, "RH"], 0)),
        "RH: 5 | // tune\nLH: I |\n"
    );
    assert_eq!(
        edits(src, |ed| ed.move_item(&path!["blocks", 0, "RH"], 0, 1)),
        "RH: 5 | 1 2 3 4 | // tune\nLH: I |\n"
    );
    assert_eq!(
        edits(src, |ed| ed.insert_item(
            &path!["blocks", 0, "RH", 0],
            1,
            "5"
        )),
        "RH: 1 5 2 3 4 | 5 | // tune\nLH: I |\n"
    );
    assert_eq!(
        edits(src, |ed| ed.delete_item(&path!["blocks", 0, "RH", 0], 3)),
        "RH: 1 2 3 | 5 | // tune\nLH: I |\n"
    );
    assert_eq!(
        edits("RH: 1 [2 3] |\n", |ed| ed.insert_item(
            &path!["blocks", 0, "RH", 0, 1],
            1,
            "4"
        )),
        "RH: 1 [2 4 3] |\n"
    );
}

#[test]
fn fig_edits_the_blocks_item_by_item() {
    let src = "RH: 1 |\n\nRH: 2 |\n\nRH: 3 |\n";
    assert_eq!(
        edits(src, |ed| ed.delete_item(&path!["blocks"], 1)),
        "RH: 1 |\n\nRH: 3 |\n"
    );
    assert_eq!(
        edits(src, |ed| ed.move_item(&path!["blocks"], 2, 0)),
        "RH: 3 |\n\nRH: 1 |\n\nRH: 2 |\n"
    );
    // TS's appendValueRaw: an insert at the end.
    assert_eq!(
        edits(src, |ed| ed.insert_item(&path!["blocks"], 3, "LH: I |")),
        "RH: 1 |\n\nRH: 2 |\n\nRH: 3 |\n\nLH: I |\n"
    );
}

// ── bars ────────────────────────────────────────────────────────────────────

#[test]
fn inserting_a_bar_writes_rests_in_every_voice_that_reaches_it() {
    let src = "RH: 1 2 3 4 | 5 |\nLH: I |\n";
    assert_eq!(
        run(src, |s, g| insert_bar(s, g, 0, 1)),
        "RH: 1 2 3 4 | . . . . | 5 |\nLH: I |\n"
    );
    assert_eq!(
        run(src, |s, g| insert_bar(s, g, 0, 0)),
        "RH: . . . . | 1 2 3 4 | 5 |\nLH: . . . . | I |\n"
    );
    // At the end, only the longest voices need it; the rest are padded.
    assert_eq!(
        run(src, |s, g| insert_bar(s, g, 0, 2)),
        "RH: 1 2 3 4 | 5 | . . . . |\nLH: I |\n"
    );
    assert_eq!(
        run("time 3/4\nRH: 1 |\n", |s, g| insert_bar(s, g, 0, 1)),
        "time 3/4\nRH: 1 | . . . |\n"
    );
}

#[test]
fn deleting_a_bar_takes_it_from_every_voice() {
    let src = "RH: 1 | 2 | 3 |\nLH: I | V |\n";
    assert_eq!(
        run(src, |s, g| delete_bar(s, g, 0, 1)),
        "RH: 1 | 3 |\nLH: I |\n"
    );
    assert_eq!(
        run(src, |s, g| delete_bar(s, g, 0, 2)),
        "RH: 1 | 2 |\nLH: I | V |\n"
    );
    // A voice whose only bar goes loses its line.
    assert_eq!(
        run("RH: 1 | 2 |\nLH: I |\n", |s, g| delete_bar(s, g, 0, 0)),
        "RH: 2 |\n"
    );
    // A block whose only bar goes is deleted.
    assert_eq!(
        run("RH: 1 |\n\nRH: 2 |\n", |s, g| delete_bar(s, g, 1, 0)),
        "RH: 1 |\n"
    );
}

#[test]
fn duplicating_a_bar_copies_it_after_itself() {
    let src = "RH: 1 [2 3] | 5 |\nLH: I |\n";
    assert_eq!(
        run(src, |s, g| duplicate_bar(s, g, 0, 0)),
        "RH: 1 [2 3] | 1 [2 3] | 5 |\nLH: I | I |\n"
    );
    assert_eq!(
        run(src, |s, g| duplicate_bar(s, g, 0, 1)),
        "RH: 1 [2 3] | 5 | 5 |\nLH: I |\n"
    );
}

#[test]
fn moving_a_bar_swaps_it_with_its_neighbour() {
    let src = "RH: 1 | 2 | 3 |\nLH: I | V |\n";
    assert_eq!(
        run(src, |s, g| move_bar(s, g, 0, 0, 1)),
        "RH: 2 | 1 | 3 |\nLH: V | I |\n"
    );
    // LH has no bar 3: it rests there, so the bar it moves out of becomes a rest.
    assert_eq!(
        run(src, |s, g| move_bar(s, g, 0, 1, 1)),
        "RH: 1 | 3 | 2 |\nLH: I | . | V |\n"
    );
    assert_eq!(
        run(src, |s, g| move_bar(s, g, 0, 2, -1)),
        "RH: 1 | 3 | 2 |\nLH: I | . | V |\n"
    );
    assert_eq!(run(src, |s, g| move_bar(s, g, 0, 0, -1)), src);
}

// ── blocks ──────────────────────────────────────────────────────────────────

#[test]
fn a_new_block_has_the_voices_of_the_one_before_it() {
    assert_eq!(
        run("RH: 1 |\nLH: I |\n\nRH: 5 |\n", |s, g| insert_block(
            s, g, 0
        )),
        "RH: 1 |\nLH: I |\n\nRH: . . . . |\nLH: . . . . |\n\nRH: 5 |\n"
    );
    assert_eq!(
        run("key G major\n", |s, g| insert_block(s, g, 0)),
        "key G major\n\nRH: . . . . |\n"
    );
    assert_eq!(run("", |s, g| insert_block(s, g, 0)), "RH: . . . . |\n");
}

#[test]
fn deleting_a_block_keeps_the_next_ones_settings() {
    assert_eq!(
        run("RH: 1 |\n\nRH: 2 |\n\nRH: 3 |\n", |s, g| delete_block(
            s, g, 1
        )),
        "RH: 1 |\n\nRH: 3 |\n"
    );
    assert_eq!(
        run("RH: 1 |\n\nRH: 2 |\n", |s, g| delete_block(s, g, 0)),
        "RH: 2 |\n"
    );
    // The first block goes: the second's change now sits in the header.
    let src = "key C major\nRH: 1 |\n\ntempo 90\nRH: 2 |\n";
    assert_eq!(
        run(src, |s, g| delete_block(s, g, 0)),
        "key C major\ntempo 90\nRH: 2 |\n"
    );
    // A block with a change goes: the next block keeps what the change set.
    let changed = "RH: 1 |\n\ntempo 90\nkey G major\nRH: 2 |\n\nRH: 3 |\n";
    let out = run(changed, |s, g| delete_block(s, g, 1));
    assert_eq!(from(&out, 1), from(changed, 2));
    assert_eq!(compile(&out).unwrap().blocks[1].tempo, 90.0);
}

#[test]
fn moving_a_block_keeps_every_blocks_settings() {
    let src = "key C major\nRH: 1 |\n\ntempo 90\nRH: 2 |\n\nRH: 3 |\n";
    let out = run(src, |s, g| move_block(s, g, 1, -1));
    let before = compile(src).unwrap();
    let after = compile(&out).unwrap();
    let tempos: Vec<f64> = after.blocks.iter().map(|b| b.tempo).collect();
    assert_eq!(tempos, [90.0, 120.0, 90.0]);
    assert_eq!(from(&out, 0)[..1], from(src, 1)[..1]);
    let midis: Vec<i32> = after.events.iter().map(|e| e.midi).collect();
    assert_eq!(
        midis,
        [
            before.events[1].midi,
            before.events[0].midi,
            before.events[2].midi
        ]
    );
    assert_eq!(run(src, |s, g| move_block(s, g, 2, 1)), src);
}

#[test]
fn duplicating_a_block_copies_its_change() {
    let src = "RH: 1 |\n\ntempo 90 // slower\nRH: 2 | 3 |\n\nRH: 4 |\n";
    let out = run(src, |s, g| duplicate_block(s, g, 1));
    assert_eq!(
        out,
        "RH: 1 |\n\ntempo 90 // slower\nRH: 2 | 3 |\n\ntempo 90 // slower\nRH: 2 | 3 |\n\nRH: 4 |\n"
    );
}

#[test]
fn splitting_and_joining_a_block_leave_the_music_alone() {
    let src = "key D minor\nRH: 1 | 2 | 3 | // melody\nLH: i | V |\n";
    let split = run(src, |s, g| split_block(s, g, 0, 1));
    assert_eq!(
        split,
        "key D minor\nRH: 1 | // melody\nLH: i |\n\nRH: 2 | 3 |\nLH: V |\n"
    );
    assert_eq!(
        format_song(&compile(&split).unwrap()),
        format_song(&compile(src).unwrap())
    );
    let two_bars = run(src, |s, g| split_block(s, g, 0, 2));
    assert_eq!(
        two_bars,
        "key D minor\nRH: 1 | 2 | // melody\nLH: i | V |\n\nRH: 3 |\n"
    );

    let joined = run(&split, |s, g| join_blocks(s, g, 0));
    assert_eq!(
        joined,
        "key D minor\nRH: 1 | 2 | 3 | // melody\nLH: i | V |\n"
    );
    // A voice missing from either block rests where it was missing.
    let gappy = "RH: 1 | 2 |\n\nRH: 3 |\nLH: I |\n";
    let out = run(gappy, |s, g| join_blocks(s, g, 0));
    assert_eq!(out, "RH: 1 | 2 | 3 |\nLH: . | . | I |\n");
    assert_eq!(
        format_song(&compile(&out).unwrap()),
        format_song(&compile(gappy).unwrap())
    );
    let err = refused("RH: 1 |\n\ntempo 90\nRH: 2 |\n", |s, g| {
        join_blocks(s, g, 0)
    });
    assert!(err.contains("starts with a change"), "{err}");
}

// ── voices ──────────────────────────────────────────────────────────────────

#[test]
fn adding_a_voice_writes_a_line_of_rests() {
    assert_eq!(
        run("RH: 1 | 2 |\n", |s, g| add_voice(s, g, 0, "Bass")),
        "RH: 1 | 2 |\nBass: . | . |\n"
    );
    assert_eq!(
        run("RH: 1 |\n\nRH: 2 |\n", |s, g| add_voice(s, g, 0, "LH")),
        "RH: 1 |\nLH: . |\n\nRH: 2 |\n"
    );
    let err = refused("RH: 1 |\n", |s, g| add_voice(s, g, 0, "RH"));
    assert!(err.contains("already"), "{err}");
    let err = refused("RH: 1 |\n", |s, g| add_voice(s, g, 0, "tempo"));
    assert!(err.contains("directive"), "{err}");
    let err = refused("RH: 1 |\n", |s, g| add_voice(s, g, 0, "2x"));
    assert!(err.contains("not a voice name"), "{err}");
}

#[test]
fn deleting_a_voice_from_one_block_or_all() {
    let src = "RH: 1 |\nLH: I |\n\nLH: V |\n\nRH: 2 |\nLH: I |\n";
    assert_eq!(
        run(src, |s, g| delete_voice(s, g, "LH", Some(0))),
        "RH: 1 |\n\nLH: V |\n\nRH: 2 |\nLH: I |\n"
    );
    assert_eq!(
        run(src, |s, g| delete_voice(s, g, "LH", None)),
        "RH: 1 |\n\nRH: 2 |\n"
    );
}

#[test]
fn renaming_a_voice_renames_its_lines_and_octaves() {
    let src = "octave LH 2\nRH: 1 |\nLH: I |\n\noctave LH 3\nLH: V |\n";
    assert_eq!(
        run(src, |s, g| rename_voice(s, g, "LH", "Bass")),
        "octave Bass 2\nRH: 1 |\nBass: I |\n\noctave Bass 3\nBass: V |\n"
    );
    let err = refused(src, |s, g| rename_voice(s, g, "LH", "RH"));
    assert!(err.contains("already"), "{err}");
}

#[test]
fn moving_a_voice_swaps_its_lines() {
    let src = "RH: 1 |\nLH: I |\n\ntempo 90\nRH: 2 |\nLH: V |\n";
    let out = run(src, |s, g| move_voice(s, g, "LH", -1));
    assert_eq!(out, "LH: I |\nRH: 1 |\n\ntempo 90\nLH: V |\nRH: 2 |\n");
    assert_eq!(compile(&out).unwrap().voices, ["LH", "RH"]);
    let err = refused("RH: 1 |\n\nLH: I |\n", |s, g| move_voice(s, g, "LH", -1));
    assert!(err.contains("first appears"), "{err}");
}

// ── directives ──────────────────────────────────────────────────────────────

#[test]
fn a_blocks_directives_are_listed_the_headers_for_the_first() {
    let song =
        compile("tempo 90\noctave LH 2\nRH: 1 |\n\nkey G major\noctave RH 5\nRH: 2 |\n").unwrap();
    let directives: Vec<Vec<String>> = song.blocks.iter().map(|b| b.directives.clone()).collect();
    assert_eq!(directives, [["tempo", "octave LH"], ["key", "octave RH"]]);
}

#[test]
fn clearing_a_directive_hands_the_block_back_the_setting_before_it() {
    let src = "tempo 90\nRH: 1 |\n\ntempo 80\noctave RH 5\noctave LH 2\nRH: 2 |\n";
    assert_eq!(
        run(src, |s, g| clear_directive(s, g, 1, "tempo")),
        "tempo 90\nRH: 1 |\n\noctave RH 5\noctave LH 2\nRH: 2 |\n"
    );
    assert_eq!(
        run(src, |s, g| clear_directive(s, g, 1, "octave RH")),
        "tempo 90\nRH: 1 |\n\ntempo 80\noctave LH 2\nRH: 2 |\n"
    );
    assert_eq!(
        run(src, |s, g| clear_directive(s, g, 0, "tempo")),
        "RH: 1 |\n\ntempo 80\noctave RH 5\noctave LH 2\nRH: 2 |\n"
    );
    assert_eq!(
        run("RH: 1 |\n\noctave RH 5\nRH: 2 |\n", |s, g| clear_directive(
            s,
            g,
            1,
            "octave RH"
        )),
        "RH: 1 |\n\nRH: 2 |\n"
    );
}

#[test]
fn setting_a_directive_edits_it_in_place_or_writes_a_line() {
    assert_eq!(
        run("tempo 90\nRH: 1 |\n", |s, g| set_directive(
            s,
            g,
            0,
            &path!["tempo"],
            Value::Int(100)
        )),
        "tempo 100\nRH: 1 |\n"
    );
    assert_eq!(
        run("RH: 1 |\n\nRH: 2 |\n", |s, g| set_directive(
            s,
            g,
            1,
            &path!["key"],
            Value::Str("A minor".into())
        )),
        "RH: 1 |\n\nkey A minor\nRH: 2 |\n"
    );
}
