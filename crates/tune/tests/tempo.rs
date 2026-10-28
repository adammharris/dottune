//! Port of test/tempo.test.ts.

use tune::{TempoMap, compile};

/// bun's `toBeCloseTo`: equal to 2 decimal places.
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.005, "{a} is not close to {b}");
}

#[test]
fn one_tempo() {
    let map = TempoMap::new(&compile("tempo 120\nRH: 1 |\n").unwrap());
    close(map.total, 2.0);
    close(map.seconds(480.0), 0.5);
    close(map.ticks(1.5), 1440.0);
}

#[test]
fn a_tempo_change_between_blocks() {
    // Block 1: 4 beats at 120 = 2 s. Block 2: 4 beats at 60 = 4 s.
    let map = TempoMap::new(&compile("tempo 120\nRH: 1 |\n\ntempo 60\nRH: 1 |\n").unwrap());
    close(map.total, 6.0);
    close(map.seconds(1920.0), 2.0);
    close(map.seconds(1920.0 + 480.0), 3.0);
    close(map.ticks(3.0), 2400.0);
    close(map.ticks(1.0), 960.0);
}

#[test]
fn an_empty_song() {
    let map = TempoMap::new(&compile("tempo 90\n").unwrap());
    assert_eq!(map.total, 0.0);
    close(map.seconds(480.0), 480.0 / (1.5 * 480.0));
}
