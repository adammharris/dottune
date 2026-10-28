//! Port of test/edit.test.ts.

use tune::edit::{move_by, set_degree, split, to_hold, to_rest, toggle_accidental};
use tune::splice::apply_token_edits;
use tune::{Song, TokenEdit, compile, format_events};

/// Applies an operation to the `nth` slot of `voice`, through fig.
fn run(
    source: &str,
    voice: &str,
    nth: usize,
    op: impl Fn(&Song, usize) -> Vec<TokenEdit>,
) -> String {
    let song = compile(source).unwrap();
    let index = song
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.voice == voice)
        .nth(nth)
        .unwrap()
        .0;
    let edits = op(&song, index);
    if edits.is_empty() {
        source.to_string()
    } else {
        apply_token_edits(source, &edits).unwrap()
    }
}

fn pitches_of(source: &str, voice: &str) -> Vec<String> {
    let song = compile(source).unwrap();
    let events: Vec<_> = song
        .events
        .into_iter()
        .filter(|e| e.voice == voice)
        .collect();
    format_events(&events)
        .split('\n')
        .map(|l| l.split(' ').skip(2).collect::<Vec<_>>().join(" "))
        .collect()
}

fn pitches(source: &str) -> Vec<String> {
    pitches_of(source, "RH")
}

#[test]
fn moving_a_note_changes_only_that_note() {
    let out = run("RH: 1 5, |\n", "RH", 0, |s, i| move_by(s, i, 1));
    assert_eq!(out, "RH: 2 5, |\n");
    assert_eq!(pitches(&out), ["RH D4", "RH G3"]);
}

#[test]
fn moving_a_note_an_octave_adds_a_mark() {
    let out = run("RH: 1 2 3 |\n", "RH", 1, |s, i| move_by(s, i, 7));
    assert_eq!(out, "RH: 1 2' 3 |\n");
    assert_eq!(pitches(&out), ["RH C4", "RH D5", "RH E4"]);
}

#[test]
fn stepping_past_7_or_below_1_moves_the_marks() {
    assert_eq!(
        run("RH: 7 |\n", "RH", 0, |s, i| move_by(s, i, 1)),
        "RH: 1' |\n"
    );
    assert_eq!(
        run("RH: 1 |\n", "RH", 0, |s, i| move_by(s, i, -1)),
        "RH: 7, |\n"
    );
    assert_eq!(
        run("RH: 2' |\n", "RH", 0, |s, i| move_by(s, i, -2)),
        "RH: 7 |\n"
    );
}

#[test]
fn moving_a_chord_picks_the_diatonic_quality() {
    assert_eq!(
        run("LH: I V7 |\n", "LH", 0, |s, i| move_by(s, i, 1)),
        "LH: ii V7 |\n"
    );
    assert_eq!(
        run("LH: I V7 |\n", "LH", 1, |s, i| move_by(s, i, 1)),
        "LH: I vi7 |\n"
    );
    assert_eq!(
        run("LH: V |\n", "LH", 0, |s, i| move_by(s, i, 2)),
        "LH: viio |\n"
    );
    assert_eq!(
        run("LH: viio |\n", "LH", 0, |s, i| move_by(s, i, 1)),
        "LH: I' |\n"
    );
}

#[test]
fn moving_a_stack_keeps_its_shape() {
    assert_eq!(
        run("RH: 1+3+5 |\n", "RH", 0, |s, i| move_by(s, i, 1)),
        "RH: 2+4+6 |\n"
    );
    let out = run("RH: 5+7+2' |\n", "RH", 0, |s, i| move_by(s, i, 1));
    assert_eq!(out, "RH: 6+1'+3' |\n");
    assert_eq!(pitches(&out), ["RH A4", "RH C5", "RH E5"]);
}

#[test]
fn typing_a_degree_keeps_the_octave() {
    let out = run("RH: 1 . 3 |\n", "RH", 1, |s, i| set_degree(s, i, 4));
    assert_eq!(out, "RH: 1 5 3 |\n");
    assert_eq!(
        run("RH: 1' |\n", "RH", 0, |s, i| set_degree(s, i, 2)),
        "RH: 3' |\n"
    );
    assert_eq!(
        run("LH: V, |\n", "LH", 0, |s, i| set_degree(s, i, 0)),
        "LH: 1, |\n"
    );
}

#[test]
fn rest_and_hold_leave_later_notes_alone() {
    assert_eq!(
        pitches(&run("RH: 1 5 1 |\n", "RH", 1, to_rest)),
        ["RH C4", "RH C4"]
    );
    let held = run("RH: 3 5 1' |\n", "RH", 1, to_hold);
    assert_eq!(held, "RH: 3 - 1' |\n");
    assert_eq!(pitches(&held), ["RH E4", "RH C5"]);
}

#[test]
fn split_halves_a_slot_without_moving_it() {
    let out = run("RH: 1' 2 |\n", "RH", 0, |s, i| split(s, i, 2));
    assert_eq!(out, "RH: [1' 1'] 2 |\n");
    assert_eq!(pitches(&out), ["RH C5", "RH C5", "RH D4"]);
}

#[test]
fn accidentals_toggle() {
    let sharp = run("RH: 4 |\n", "RH", 0, |s, i| toggle_accidental(s, i, "#"));
    assert_eq!(sharp, "RH: #4 |\n");
    assert_eq!(
        run(&sharp, "RH", 0, |s, i| toggle_accidental(s, i, "#")),
        "RH: 4 |\n"
    );
}

#[test]
fn edits_inside_groups_and_later_blocks_reach_the_right_token() {
    let src = "RH: 1 [2 3] |\n\nRH: 5 |\n";
    assert_eq!(
        run(src, "RH", 2, |s, i| set_degree(s, i, 3)),
        "RH: 1 [2 4] |\n\nRH: 5 |\n"
    );
    assert_eq!(
        run(src, "RH", 3, |s, i| set_degree(s, i, 0)),
        "RH: 1 [2 3] |\n\nRH: 1 |\n"
    );
}

#[test]
fn an_edit_leaves_the_next_block_alone_across_a_key_change() {
    let src = "RH: 4 |\n\nkey G major\nRH: 3 |\n";
    let out = run(src, "RH", 0, |s, i| move_by(s, i, 1));
    assert_eq!(out, "RH: 5 |\n\nkey G major\nRH: 3 |\n");
    assert_eq!(pitches(&out), ["RH G4", "RH B4"]);
}

#[test]
fn chords_moved_after_a_key_change_use_the_new_scale() {
    let src = "LH: I |\n\nkey A minor\nLH: i |\n";
    assert_eq!(
        run(src, "LH", 1, |s, i| move_by(s, i, 1)),
        "LH: I |\n\nkey A minor\nLH: iio |\n"
    );
}
