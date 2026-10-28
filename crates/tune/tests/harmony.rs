//! Port of test/harmony.test.ts.

use tune::edit::{
    add_third, cycle_chord, cycle_inversion, cycle_quality, delete_slot, divide_bar, insert_slot,
    join_group, set_pitch, split, toggle_accidental,
};
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

/// Steps an operation `times` times on the same slot.
fn steps(
    source: &str,
    voice: &str,
    nth: usize,
    op: impl Fn(&Song, usize) -> Vec<TokenEdit>,
    times: usize,
) -> Vec<String> {
    let mut source = source.to_string();
    let mut out = vec![];
    for _ in 0..times {
        source = run(&source, voice, nth, &op);
        out.push(source.trim().to_string());
    }
    out
}

// ── harmony ─────────────────────────────────────────────────────────────────

#[test]
fn c_turns_a_note_into_its_triad_then_its_seventh_chord_then_back() {
    assert_eq!(
        steps("LH: 5, |\n", "LH", 0, cycle_chord, 3),
        ["LH: V, |", "LH: V7, |", "LH: 5, |"]
    );
    assert_eq!(
        steps("LH: 7 |\n", "LH", 0, cycle_chord, 2),
        ["LH: viio |", "LH: viih7 |"]
    );
    assert_eq!(
        steps("LH: 4 |\n", "LH", 0, cycle_chord, 2),
        ["LH: IV |", "LH: IVmaj7 |"]
    );
    // In minor, the diatonic qualities follow the scale.
    assert_eq!(
        run("key A minor\nLH: 1 |\n", "LH", 0, cycle_chord),
        "key A minor\nLH: i |\n"
    );
    // A stack becomes the triad on its first member.
    assert_eq!(run("RH: 2+4+6 |\n", "RH", 0, cycle_chord), "RH: ii |\n");
}

#[test]
fn q_steps_a_chord_through_the_qualities() {
    assert_eq!(
        steps("LH: I |\n", "LH", 0, cycle_quality, 4),
        ["LH: i |", "LH: io |", "LH: I+ |", "LH: I |"]
    );
    assert_eq!(
        steps("LH: V7, |\n", "LH", 0, cycle_quality, 5),
        [
            "LH: Vmaj7, |",
            "LH: v7, |",
            "LH: vh7, |",
            "LH: vo7, |",
            "LH: V7, |"
        ]
    );
    let pitches: Vec<String> = format_events(&compile("LH: io |\n").unwrap().events)
        .split('\n')
        .map(|l| l.split(' ').next_back().unwrap().to_string())
        .collect();
    assert_eq!(pitches, ["C3", "D#3", "F#3"]);
}

#[test]
fn v_steps_a_chord_through_its_inversions() {
    assert_eq!(
        steps("LH: I |\n", "LH", 0, cycle_inversion, 3),
        ["LH: I/3 |", "LH: I/5 |", "LH: I |"]
    );
    assert_eq!(
        steps("LH: V7 |\n", "LH", 0, cycle_inversion, 4),
        ["LH: V7/7 |", "LH: V7/2 |", "LH: V7/4 |", "LH: V7 |"]
    );
    // A chord borrowed from outside the scale spells its bass to match.
    assert_eq!(run("LH: II |\n", "LH", 0, cycle_inversion), "LH: II/#4 |\n");
    assert_eq!(
        run("key A minor\nLH: V |\n", "LH", 0, cycle_inversion),
        "key A minor\nLH: V/#7 |\n"
    );
}

#[test]
fn plus_stacks_a_third_on_top() {
    assert_eq!(
        steps("RH: 1 |\n", "RH", 0, add_third, 3),
        ["RH: 1+3 |", "RH: 1+3+5 |", "RH: 1+3+5+7 |"]
    );
    assert_eq!(run("RH: 6+1' |\n", "RH", 0, add_third), "RH: 6+1'+3' |\n");
    assert_eq!(run("LH: I |\n", "LH", 0, add_third), "LH: I |\n");
}

#[test]
fn accidentals_toggle_on_a_chords_root() {
    assert_eq!(
        run("LH: VII |\n", "LH", 0, |s, i| toggle_accidental(s, i, "b")),
        "LH: bVII |\n"
    );
    assert_eq!(
        run("LH: bVII |\n", "LH", 0, |s, i| toggle_accidental(s, i, "b")),
        "LH: VII |\n"
    );
}

#[test]
fn a_pitch_is_written_as_the_degree_that_sounds_it() {
    assert_eq!(
        run("RH: . |\n", "RH", 0, |s, i| set_pitch(s, i, 67)),
        "RH: 5 |\n"
    );
    assert_eq!(
        run("RH: 1 |\n", "RH", 0, |s, i| set_pitch(s, i, 61)),
        "RH: #1 |\n"
    );
    assert_eq!(
        run("RH: - |\n", "RH", 0, |s, i| set_pitch(s, i, 72)),
        "RH: 1' |\n"
    );
    assert_eq!(
        run("RH: . |\n", "RH", 0, |s, i| set_pitch(s, i, 59)),
        "RH: 7, |\n"
    );
    assert_eq!(
        run("key Eb major\nLH: . |\n", "LH", 0, |s, i| set_pitch(
            s, i, 46
        )),
        "key Eb major\nLH: 5, |\n"
    );
    assert_eq!(
        run("octave RH 5\nRH: . |\n", "RH", 0, |s, i| set_pitch(
            s, i, 72
        )),
        "octave RH 5\nRH: 1 |\n"
    );
}

// ── rhythm ──────────────────────────────────────────────────────────────────

#[test]
fn split_makes_thirds_as_well_as_halves() {
    assert_eq!(
        run("RH: 1 2 |\n", "RH", 0, |s, i| split(s, i, 3)),
        "RH: [1 1 1] 2 |\n"
    );
}

#[test]
fn inserting_a_slot_re_divides_its_group_or_bar() {
    assert_eq!(
        run("RH: 1 2 3 |\n", "RH", 0, insert_slot),
        "RH: 1 . 2 3 |\n"
    );
    assert_eq!(
        run("RH: 1 [2 3] |\n", "RH", 2, insert_slot),
        "RH: 1 [2 3 .] |\n"
    );
}

#[test]
fn deleting_a_slot_re_divides_what_is_left() {
    assert_eq!(run("RH: 1 2 3 |\n", "RH", 1, delete_slot), "RH: 1 3 |\n");
    assert_eq!(run("RH: 1 [2 3] |\n", "RH", 1, delete_slot), "RH: 1 3 |\n");
    assert_eq!(
        run("RH: 1 [2 3 4] |\n", "RH", 1, delete_slot),
        "RH: 1 [3 4] |\n"
    );
    assert_eq!(run("RH: 1 |\n", "RH", 0, delete_slot), "RH: . |\n");
}

#[test]
fn joining_a_group_makes_it_one_slot_again() {
    assert_eq!(run("RH: 1 [2 3] |\n", "RH", 2, join_group), "RH: 1 2 |\n");
    assert_eq!(
        run("RH: [1 [2 3]] 4 |\n", "RH", 2, join_group),
        "RH: [1 2] 4 |\n"
    );
    assert_eq!(run("RH: 1 2 |\n", "RH", 0, join_group), "RH: 1 2 |\n");
}

#[test]
fn dividing_a_bar_keeps_what_fits_the_new_grid() {
    assert_eq!(
        run("RH: 1 2 |\n", "RH", 0, |s, i| divide_bar(s, i, 4)),
        "RH: 1 - 2 - |\n"
    );
    assert_eq!(
        run("RH: 1 [2 3] 4 . |\n", "RH", 0, |s, i| divide_bar(s, i, 8)),
        "RH: 1 - 2 3 4 - . . |\n"
    );
    assert_eq!(
        run("RH: 1 - 2 - |\n", "RH", 0, |s, i| divide_bar(s, i, 2)),
        "RH: 1 2 |\n"
    );
    // Each slot takes the first note starting within it; one that fits no slot alone is lost.
    assert_eq!(
        run("RH: 1 2 3 |\n", "RH", 0, |s, i| divide_bar(s, i, 2)),
        "RH: 1 3 |\n"
    );
    // A note held over the barline keeps holding.
    assert_eq!(
        run("RH: 1 | - 2 |\n", "RH", 1, |s, i| divide_bar(s, i, 4)),
        "RH: 1 | - - 2 - |\n"
    );
}
