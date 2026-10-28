//! Port of test/errors.test.ts.

use tune::midi::{read_midi, write_midi};
use tune::{TuneError, compile};

fn errors_of(source: &str) -> Vec<TuneError> {
    match compile(source) {
        Err(e) => e.0,
        Ok(_) => panic!("expected an error"),
    }
}

/// Each error as `line: text at fault`.
fn faults(source: &str) -> Vec<String> {
    errors_of(source)
        .iter()
        .map(|e| format!("{}: {}", e.line, e.span.map_or("-", |(a, b)| &source[a..b])))
        .collect()
}

#[test]
fn errors_point_at_the_text_at_fault() {
    assert_eq!(faults("RH: 1 2 x 4 |"), ["1: x"]);
    assert_eq!(faults("RH: 1 [2 3 4 |"), ["1: ["]);
    assert_eq!(faults("RH: 1 2] |"), ["1: ]"]);
    assert_eq!(faults("RH: 1 [] |"), ["1: []"]);
    assert_eq!(faults("RH: 1 || 2 |"), ["1: |"]);
    assert_eq!(faults("RH: 1 |\nRH: 2 |"), ["2: RH"]);
    assert_eq!(faults("key H major\nRH: 1 |"), ["1: key H major"]);
    assert_eq!(faults("  tempo 2  // slow\nRH: 1 |"), ["1: tempo 2"]);
    assert_eq!(faults("RH: 1 Ih |"), ["1: Ih"]);
    assert_eq!(faults("RH: 1 |\n\ntempo 90"), ["3: tempo 90"]);
}

#[test]
fn every_error_is_reported_in_order_and_the_first_is_thrown() {
    let source = "tempo 0\nRH: 1 x | 2 |\nLH: I [ |\n\nfoo 3\nRH: y |";
    assert_eq!(
        faults(source),
        ["1: tempo 0", "2: x", "3: [", "5: foo 3", "6: y"]
    );
    assert_eq!(compile(source).unwrap_err().first().line, 1);
}

#[test]
fn a_bad_bar_does_not_hide_errors_in_the_rest_of_its_line() {
    assert_eq!(faults("RH: 1 x 3 | 4 [5 | y |"), ["1: x", "1: [", "1: y"]);
    assert_eq!(faults("RH: 1 || 2 | z |"), ["1: |", "1: z"]);
}

#[test]
fn an_error_in_one_voice_line_does_not_hide_one_in_the_next_block() {
    let lines: Vec<String> = faults("RH: 1 || 2 |\n\nRH: 9 |")
        .iter()
        .map(|f| f.split(':').next().unwrap().to_string())
        .collect();
    assert_eq!(lines, ["1", "3"]);
}

#[test]
fn pitches_must_be_in_midis_range() {
    assert_eq!(faults("RH: 1 5'''''' |"), ["1: 5''''''"]);
    assert_eq!(faults("octave LH -1\nLH: 1, |"), ["2: 1,"]);
    assert_eq!(faults("octave LH -1\nLH: I/3 |"), ["2: I/3"]);
    assert_eq!(compile("octave RH 9\nRH: 5 |").unwrap().events[0].midi, 127);
    assert_eq!(compile("octave RH -1\nRH: 1 |").unwrap().events[0].midi, 0);
}

#[test]
fn tempo_and_time_limits() {
    assert_eq!(compile("tempo 4\nRH: 1 |").unwrap().tempo, 4.0);
    assert_eq!(compile("tempo 1000\nRH: 1 |").unwrap().tempo, 1000.0);
    assert_eq!(faults("tempo 1001\nRH: 1 |").len(), 1);
    assert_eq!(compile("time 64/64\nRH: 1 |").unwrap().time, (64, 64));
    assert_eq!(faults("time 65/4\nRH: 1 |").len(), 1);
    assert_eq!(faults("time 3/128\nRH: 1 |").len(), 1);
    assert_eq!(faults("time 0/4\nRH: 1 |").len(), 1);
}

#[test]
fn every_midi_value_survives_a_round_trip_at_the_limits() {
    let file = read_midi(&write_midi(
        &compile("tempo 4\ntime 64/64\noctave RH 9\nRH: 5 |").unwrap(),
    ))
    .unwrap();
    assert_eq!(file.tempos[0].1, 15_000_000);
    assert_eq!(file.times[0].1, (64, 64));
    assert_eq!(file.tracks[1].notes[0].midi, 127);
}

#[test]
fn voices_past_the_fifteenth_share_channels_in_turn_never_the_drum_channel() {
    let source = (0..17)
        .map(|i| format!("V{i}: 1 |"))
        .collect::<Vec<_>>()
        .join("\n");
    let channels: Vec<u8> = read_midi(&write_midi(&compile(&source).unwrap()))
        .unwrap()
        .tracks[1..]
        .iter()
        .map(|t| t.notes[0].channel)
        .collect();
    assert_eq!(
        channels,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 0, 1]
    );
}
