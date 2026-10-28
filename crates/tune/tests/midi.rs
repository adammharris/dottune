//! Port of test/midi.test.ts.

use tune::compile;
use tune::midi::{key_signature, read_midi, write_midi};

/// SPEC.md's examples whose expectation is an event list (tools/spec-examples.ts):
/// a ```tune fence immediately followed by an ```events fence. `(line, source)`.
fn event_examples() -> Vec<(usize, String)> {
    let spec =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../SPEC.md")).unwrap();
    let lines: Vec<&str> = spec.split('\n').collect();
    let mut fences: Vec<(String, String, usize)> = vec![];
    let mut i = 0;
    while i < lines.len() {
        if let Some(lang) = lines[i].strip_prefix("```") {
            let lang = lang.trim_end();
            if lang.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let start = i;
                let mut body = vec![];
                i += 1;
                while i < lines.len() && lines[i].trim_end() != "```" {
                    body.push(lines[i]);
                    i += 1;
                }
                fences.push((lang.to_string(), body.join("\n"), start + 1));
            }
        }
        i += 1;
    }
    let mut out = vec![];
    let mut i = 0;
    while i + 1 < fences.len() {
        let (tune, next) = (&fences[i], &fences[i + 1]);
        if tune.0 == "tune" && (next.0 == "events" || next.0 == "error") {
            if next.0 == "events" {
                out.push((tune.2, tune.1.clone()));
            }
            i += 1;
        }
        i += 1;
    }
    out
}

// Every spec example survives tune → MIDI → notes, track by track.
#[test]
fn midi_round_trip() {
    let examples = event_examples();
    assert!(
        examples.len() >= 27,
        "only {} examples found",
        examples.len()
    );
    for (line, source) in examples {
        let song = compile(&source).unwrap_or_else(|e| panic!("SPEC.md:{line}: {e}"));
        let file = read_midi(&write_midi(&song)).unwrap();
        assert_eq!(file.format, 1, "SPEC.md:{line}");
        assert_eq!(file.division, 480, "SPEC.md:{line}");
        let names: Vec<&str> = file.tracks.iter().map(|t| t.name.as_str()).collect();
        let mut want_names = vec![""];
        want_names.extend(song.voices.iter().map(String::as_str));
        assert_eq!(names, want_names, "SPEC.md:{line}");
        for voice in &song.voices {
            let track = file.tracks.iter().find(|t| &t.name == voice).unwrap();
            let mut expected: Vec<(i64, i64, i64)> = song
                .events
                .iter()
                .filter(|e| &e.voice == voice)
                .map(|e| (e.start, e.duration, e.midi as i64))
                .collect();
            expected.sort_by_key(|&(start, _, midi)| (start, midi));
            let got: Vec<(i64, i64, i64)> = track
                .notes
                .iter()
                .map(|n| (n.start as i64, n.duration as i64, n.midi as i64))
                .collect();
            assert_eq!(got, expected, "SPEC.md:{line} voice {voice}");
        }
    }
}

#[test]
fn conductor_track_tempo_time_key_length() {
    let song = compile("key A minor\ntempo 100\ntime 6/8\nRH: 1 - - . . . | . |\n").unwrap();
    let file = read_midi(&write_midi(&song)).unwrap();
    assert_eq!(file.tempos[0].1, 600_000);
    assert_eq!(file.times[0].1, (6, 8));
    assert_eq!((file.keys[0].1, file.keys[0].2), (0, true));
    // Trailing rests still count toward the length.
    assert_eq!(file.length, 2880);
}

#[test]
fn repeated_notes_do_not_cut_each_other_off() {
    let file = read_midi(&write_midi(&compile("RH: 1 1 1 1 |\n").unwrap())).unwrap();
    let durations: Vec<u64> = file.tracks[1].notes.iter().map(|n| n.duration).collect();
    assert_eq!(durations, [480, 480, 480, 480]);
}

#[test]
fn key_signatures() {
    assert_eq!(key_signature("C major"), (0, false));
    assert_eq!(key_signature("Eb major"), (-3, false));
    assert_eq!(key_signature("F# minor"), (3, true));
    assert_eq!(key_signature("D dorian"), (0, false));
    assert_eq!(key_signature("G mixolydian"), (0, false));
    assert_eq!(key_signature("Bb lydian"), (-1, false));
    assert_eq!(key_signature("C# phrygian"), (3, false)); // A major
}

#[test]
fn changes_between_blocks_become_conductor_events() {
    let song =
        compile("key C major\ntempo 120\nRH: 1 |\n\nkey E minor\ntempo 90\ntime 3/4\nRH: 1 |\n")
            .unwrap();
    let file = read_midi(&write_midi(&song)).unwrap();
    assert_eq!(file.tempos, [(0, 500_000), (1920, 666_667)]);
    assert_eq!(file.times, [(0, (4, 4)), (1920, (3, 4))]);
    assert_eq!(file.keys, [(0, 0, false), (1920, 1, true)]);
    assert_eq!(file.length, 1920 + 1440);
}
