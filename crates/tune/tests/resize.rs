//! Port of test/resize.test.ts.

use tune::edit::{Cut, lengthen, resize, restart, snap_cut, snap_start};
use tune::splice::apply_token_edits;
use tune::{compile, format_events};

/// The `nth` slot of `voice`, as an index into `song.slots`.
fn slot_index(source: &str, voice: &str, nth: usize) -> usize {
    let slots = compile(source).unwrap().slots;
    slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.voice == voice)
        .nth(nth)
        .unwrap()
        .0
}

/// Resizes the `nth` slot of RH to end `k`/`n` through RH's `at`th slot.
fn cut_by(source: &str, nth: usize, at: usize, k: usize, n: usize) -> String {
    let song = compile(source).unwrap();
    let c = Cut {
        slot: slot_index(source, "RH", at),
        k,
        n,
    };
    apply_token_edits(source, &resize(&song, slot_index(source, "RH", nth), c)).unwrap()
}

fn cut(source: &str, nth: usize, at: usize) -> String {
    cut_by(source, nth, at, 1, 1)
}

fn events(source: &str) -> String {
    format_events(&compile(source).unwrap().events)
}

#[test]
fn growing_over_the_grid_holds_the_slots_it_covers() {
    assert_eq!(cut("RH: 1 2 3 4 |\n", 0, 2), "RH: 1 - - 4 |\n");
}

#[test]
fn shrinking_gives_holds_back_as_rests() {
    assert_eq!(cut("RH: 1 - - - |\n", 0, 1), "RH: 1 - . . |\n");
}

#[test]
fn an_end_inside_a_slot_splits_it() {
    // A dotted quarter: the 2 is struck again halfway through beat 2.
    let out = cut_by("RH: 1 2 3 4 |\n", 0, 1, 1, 2);
    assert_eq!(out, "RH: 1 [- 2] 3 4 |\n");
    assert_eq!(
        events(&out),
        "0 720 RH C4\n720 240 RH D4\n960 480 RH E4\n1440 480 RH F4"
    );
}

#[test]
fn splits_work_inside_groups() {
    assert_eq!(
        cut_by("RH: 1 [2 3 4] 5 6 |\n", 1, 2, 1, 2),
        "RH: 1 [2 [- 3] 4] 5 6 |\n"
    );
}

#[test]
fn shrinking_inside_the_notes_own_slot() {
    assert_eq!(cut_by("RH: 1 2 |\n", 0, 0, 1, 2), "RH: [1 .] 2 |\n");
    assert_eq!(cut_by("RH: 1 2 |\n", 0, 0, 3, 4), "RH: [1 - - .] 2 |\n");
}

#[test]
fn growing_over_a_held_note_strikes_what_is_left_of_it() {
    let out = cut("RH: 1 2 - - |\n", 0, 1);
    assert_eq!(out, "RH: 1 - 2 - |\n");
    assert_eq!(events(&out), "0 960 RH C4\n960 960 RH D4");
}

#[test]
fn growing_over_a_rest_does_not_let_its_hold_join_the_note() {
    let out = cut("RH: 1 . - 3 |\n", 0, 1);
    assert_eq!(out, "RH: 1 - . 3 |\n");
}

#[test]
fn touched_groups_are_written_in_their_coarsest_form() {
    // [1 .] grown to the end of its slot is just 1.
    assert_eq!(cut("RH: [1 .] 2 |\n", 0, 1), "RH: 1 2 |\n");
    assert_eq!(cut("RH: [1 . . .] 2 |\n", 0, 1), "RH: [1 .] 2 |\n");
    // A group the edit does not reach keeps its spelling.
    assert_eq!(cut("RH: 1 2 [3 -] |\n", 0, 1), "RH: 1 - [3 -] |\n");
}

#[test]
fn holds_cross_barlines_and_blocks() {
    assert_eq!(cut("RH: 1 2 | 3 4 |\n", 1, 2), "RH: 1 2 | - 4 |\n");
    assert_eq!(
        cut("RH: 1 2 |\n\nRH: 3 4 |\n", 1, 2),
        "RH: 1 2 |\n\nRH: - 4 |\n"
    );
}

#[test]
fn other_voices_and_spacing_are_untouched() {
    let src = "RH: 1   2   3   4 |\nLH: I       V      |\n";
    assert_eq!(
        cut_by(src, 0, 1, 1, 2),
        "RH: 1   [- 2]   3   4 |\nLH: I       V      |\n"
    );
}

#[test]
fn chords_and_stacks_resize_like_notes() {
    assert_eq!(cut("RH: 1+3+5 . |\n", 0, 1), "RH: 1+3+5 - |\n");
}

#[test]
fn rests_holds_and_padding_cannot_be_resized_or_grown_into() {
    let src = "RH: . - |\nLH: I | I |\n";
    let song = compile(src).unwrap();
    let c = Cut {
        slot: slot_index(src, "RH", 1),
        k: 1,
        n: 1,
    };
    assert_eq!(resize(&song, slot_index(src, "RH", 0), c), []);
    let padded = "RH: 1 |\nLH: I | I |\n";
    assert_eq!(
        lengthen(&compile(padded).unwrap(), slot_index(padded, "RH", 0), 1),
        []
    );
}

#[test]
fn snapping_picks_the_nearest_point_in_the_slot_under_the_cursor() {
    let src = "RH: 1 2 3 4 |\n";
    let song = compile(src).unwrap();
    let i = slot_index(src, "RH", 0);
    // 1.4 beats in, in halves of a beat: the half beat at 1.5.
    assert_eq!(
        snap_cut(&song, i, 480.0 * 1.4, |_| 2),
        Some(Cut {
            slot: slot_index(src, "RH", 1),
            k: 1,
            n: 2
        })
    );
    // Past the end: the end of the last slot.
    assert_eq!(
        snap_cut(&song, i, 99999.0, |_| 1),
        Some(Cut {
            slot: slot_index(src, "RH", 3),
            k: 1,
            n: 1
        })
    );
    // Before the note: never shorter than its first part.
    assert_eq!(
        snap_cut(&song, i, -5.0, |_| 4),
        Some(Cut {
            slot: i,
            k: 1,
            n: 4
        })
    );
}

#[test]
fn the_keyboard_grows_by_a_slot_and_shrinks_by_a_hold_then_by_halves() {
    let src = "RH: 1 2 3 4 |\n";
    let at = |s: &str| slot_index(s, "RH", 0);
    let grown = apply_token_edits(src, &lengthen(&compile(src).unwrap(), at(src), 1)).unwrap();
    assert_eq!(grown, "RH: 1 - 3 4 |\n");
    let back =
        apply_token_edits(&grown, &lengthen(&compile(&grown).unwrap(), at(&grown), -1)).unwrap();
    assert_eq!(back, "RH: 1 . 3 4 |\n");
    assert_eq!(
        apply_token_edits(&back, &lengthen(&compile(&back).unwrap(), at(&back), -1)).unwrap(),
        "RH: [1 .] . 3 4 |\n"
    );
}

/// Moves the start of the `nth` slot of RH to `k`/`n` through RH's `at`th slot.
fn start_by(source: &str, nth: usize, at: usize, k: usize, n: usize) -> String {
    let song = compile(source).unwrap();
    let c = Cut {
        slot: slot_index(source, "RH", at),
        k,
        n,
    };
    apply_token_edits(source, &restart(&song, slot_index(source, "RH", nth), c)).unwrap()
}

fn start(source: &str, nth: usize, at: usize) -> String {
    start_by(source, nth, at, 0, 1)
}

#[test]
fn starting_earlier_takes_over_the_slots_before_cutting_short_what_was_there() {
    assert_eq!(start("RH: 1 2 3 4 |\n", 2, 1), "RH: 1 3 - 4 |\n");
    assert_eq!(
        start_by("RH: 1 - 3 4 |\n", 2, 1, 1, 2),
        "RH: 1 [- 3] - 4 |\n"
    );
    assert_eq!(start("RH: 1 2 | 3 4 |\n", 2, 1), "RH: 1 3 | - 4 |\n");
}

#[test]
fn starting_later_leaves_a_rest_and_keeps_the_end() {
    assert_eq!(start("RH: 1 - - 4 |\n", 0, 1), "RH: . 1 - 4 |\n");
    assert_eq!(start_by("RH: 1 2 |\n", 0, 0, 1, 2), "RH: [. 1] 2 |\n");
    let out = start_by("RH: 1 - - 4 |\n", 0, 2, 1, 2);
    assert_eq!(out, "RH: . . [. 1] 4 |\n");
    assert_eq!(events(&out), "1200 240 RH C4\n1440 480 RH F4");
}

#[test]
fn a_start_never_reaches_the_notes_end_or_crosses_padding() {
    let song = compile("RH: 1 2 |\n").unwrap();
    let i = slot_index("RH: 1 2 |\n", "RH", 0);
    assert_eq!(
        snap_start(&song, i, 9999.0, |_| 2),
        Some(Cut {
            slot: i,
            k: 1,
            n: 2
        })
    );
    assert_eq!(
        restart(
            &song,
            i,
            Cut {
                slot: i,
                k: 1,
                n: 1
            }
        ),
        []
    );
    let padded = "LH: I | I |\nRH: . |\n\nRH: 1 |\n";
    assert_eq!(
        snap_start(
            &compile(padded).unwrap(),
            slot_index(padded, "RH", 2),
            0.0,
            |_| 1
        ),
        Some(Cut {
            slot: slot_index(padded, "RH", 2),
            k: 0,
            n: 1
        })
    );
}
