//! Where things are on the piano roll: lanes, the part of the song in view,
//! and what lies under a point. Pure functions of a song and a size, so
//! they are tested without a window. Coordinates are CSS pixels from the
//! roll's top-left corner.

use tune::{NoteEvent, Song, TICKS_PER_QUARTER};

pub const KEYBOARD_W: f64 = 44.0;
/// The overview strip across the top: the whole song, its blocks, and the part in view.
pub const STRIP_H: f64 = 20.0;
/// The gap between lanes.
pub const LANE_GAP: f64 = 2.0;
/// The narrowest view: one quarter note across the roll.
pub const MIN_SPAN: f64 = TICKS_PER_QUARTER as f64;

/// A stretch of the song, in ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub start: f64,
    pub end: f64,
}

impl View {
    pub fn span(&self) -> f64 {
        self.end - self.start
    }
}

/// A band of pitch rows: one voice's in lanes, or every voice's when combined.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    /// The voice drawn here, or `None` for all of them.
    pub voice: Option<String>,
    pub lo: i32,
    pub hi: i32,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub view: View,
    pub lanes: Vec<Lane>,
    /// The pitch range across every lane, for the overview strip.
    pub lo: i32,
    pub hi: i32,
    pub row_h: f64,
    pub roll_w: f64,
    pub length: f64,
}

/// A point on the roll: its tick, its pitch row, and its lane's voice
/// (`None` where every voice shares one).
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub tick: f64,
    pub midi: i32,
    pub voice: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Start,
    End,
}

/// A note edge: the handle that moves where a note starts or ends.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub event: NoteEvent,
    pub side: Side,
}

// ── the view ────────────────────────────────────────────────────────────────

/// What `view` shows of a song `length` ticks long; `None` shows all of it.
pub fn shown(view: Option<View>, length: f64) -> View {
    view.unwrap_or(View {
        start: 0.0,
        end: length,
    })
}

/// `span` ticks from `start`, kept within the song and no narrower than a
/// quarter note; `None` if that is the whole song.
pub fn place(length: f64, start: f64, span: f64) -> Option<View> {
    let span = length.min(MIN_SPAN.max(span));
    let start = (length - span).min(start.max(0.0));
    (span < length).then_some(View {
        start,
        end: start + span,
    })
}

/// `start`–`end`, centred, at least a quarter note wide.
pub fn fit(length: f64, start: f64, end: f64) -> Option<View> {
    let span = MIN_SPAN.max(end - start);
    place(length, (start + end) / 2.0 - span / 2.0, span)
}

/// A view from before the song got shorter, pulled back inside it.
pub fn clamp(view: Option<View>, length: f64) -> Option<View> {
    match view {
        Some(v) if v.end > length => place(length, length, v.span()),
        v => v,
    }
}

/// Zoomed by `factor` (above 1 zooms in), keeping `tick` where it is on screen.
pub fn zoom(view: Option<View>, length: f64, factor: f64, tick: f64) -> Option<View> {
    let v = shown(view, length);
    let span = length.min(MIN_SPAN.max(v.span() / factor));
    place(length, tick - (tick - v.start) / v.span() * span, span)
}

/// Scrolled by `ticks`; the whole song does not scroll.
pub fn pan(view: Option<View>, length: f64, ticks: f64) -> Option<View> {
    view.and_then(|v| place(length, v.start + ticks, v.span()))
}

/// Scrolled, if need be, so `start`–`end` is in view, keeping the zoom.
pub fn reveal(view: Option<View>, length: f64, start: f64, end: f64) -> Option<View> {
    let v = view?;
    if start >= v.start && end <= v.end {
        return view;
    }
    let span = v.span();
    let from = if end - start > span || start < v.start {
        start
    } else {
        end - span
    };
    place(length, from, span)
}

/// While playing: the page turned when the playhead runs off the right
/// edge, or jumps out of view.
pub fn follow(view: Option<View>, length: f64, last: f64, tick: f64) -> Option<View> {
    let v = view?;
    let ran_off = last < v.end && tick >= v.end;
    let jumped = tick < last && (tick < v.start || tick >= v.end);
    if ran_off || jumped {
        reveal(view, length, tick, tick + v.span())
    } else {
        view
    }
}

// ── layout ──────────────────────────────────────────────────────────────────

/// The pitches `midis` span, two semitones either side, widened to at least `min`.
fn range(midis: impl Iterator<Item = i32> + Clone, min: i32) -> (i32, i32) {
    let mut lo = midis.clone().min().unwrap_or(60) - 2;
    let mut hi = midis.max().unwrap_or(60) + 2;
    while hi - lo < min {
        lo -= 1;
        hi += 1;
    }
    (lo, hi)
}

/// The roll for `song` at `width` × `height`: a lane per voice (if `lanes`
/// and there is more than one), else one shared, showing `view`.
pub fn layout(song: &Song, width: f64, height: f64, lanes: bool, view: Option<View>) -> Layout {
    let bands: Vec<(Option<String>, i32, i32)> = if lanes && song.voices.len() > 1 {
        song.voices
            .iter()
            .map(|v| {
                let (lo, hi) = range(
                    song.events.iter().filter(|e| &e.voice == v).map(|e| e.midi),
                    12,
                );
                (Some(v.clone()), lo, hi)
            })
            .collect()
    } else {
        let (lo, hi) = range(song.events.iter().map(|e| e.midi), 24);
        vec![(None, lo, hi)]
    };
    // Every lane has the same row height, so a semitone looks the same size in each.
    let rows: i32 = bands.iter().map(|b| b.2 - b.1 + 1).sum();
    let row_h = (height - STRIP_H - LANE_GAP * (bands.len() as f64 - 1.0)) / rows as f64;
    let mut top = STRIP_H;
    let lanes: Vec<Lane> = bands
        .into_iter()
        .map(|(voice, lo, hi)| {
            let lane = Lane {
                voice,
                lo,
                hi,
                top,
                bottom: top + (hi - lo + 1) as f64 * row_h,
            };
            top = lane.bottom + LANE_GAP;
            lane
        })
        .collect();
    let length = song.length as f64;
    Layout {
        lo: lanes.iter().map(|l| l.lo).min().unwrap_or(0),
        hi: lanes.iter().map(|l| l.hi).max().unwrap_or(0),
        lanes,
        row_h,
        roll_w: width - KEYBOARD_W,
        length,
        view: shown(clamp(view, length), length),
    }
}

impl Layout {
    /// The lane `voice` is drawn in.
    pub fn lane_of(&self, voice: &str) -> &Lane {
        self.lanes
            .iter()
            .find(|l| l.voice.as_deref() == Some(voice))
            .unwrap_or(&self.lanes[0])
    }

    /// Where tick `t` is across the roll.
    pub fn x_of(&self, t: f64) -> f64 {
        KEYBOARD_W + (t - self.view.start) / self.view.span() * self.roll_w
    }

    /// The top of pitch row `midi` in `lane`.
    pub fn y_of(&self, lane: &Lane, midi: i32) -> f64 {
        lane.top + (lane.hi - midi) as f64 * self.row_h
    }

    /// How wide `ticks` is on screen.
    pub fn width_of(&self, ticks: f64) -> f64 {
        ticks / self.view.span() * self.roll_w
    }

    /// Where tick `t` is across the overview strip, which shows the whole song.
    pub fn strip_x(&self, t: f64) -> f64 {
        KEYBOARD_W + t / self.length.max(1.0) * self.roll_w
    }

    // ── what is under a point ───────────────────────────────────────────

    /// The tick at `x`, clamped to the part in view.
    pub fn tick_at(&self, x: f64) -> f64 {
        let t = self.view.start + (x - KEYBOARD_W) / self.roll_w * self.view.span();
        t.clamp(self.view.start, self.view.end)
    }

    /// The tick, pitch, and lane at a point, or `None` outside the roll.
    pub fn point_at(&self, x: f64, y: f64) -> Option<Point> {
        if x < KEYBOARD_W || y < STRIP_H {
            return None;
        }
        // A point in the gap below a lane belongs to it.
        let lane = self
            .lanes
            .iter()
            .find(|l| y < l.bottom + LANE_GAP)
            .unwrap_or(self.lanes.last()?);
        let midi = lane
            .lo
            .max(lane.hi - ((y - lane.top) / self.row_h).floor() as i32);
        Some(Point {
            tick: self.view.start + (x - KEYBOARD_W) / self.roll_w * self.view.span(),
            midi,
            voice: lane.voice.clone(),
        })
    }

    /// The tick of the whole song at a point in the overview strip, or `None` outside it.
    pub fn strip_at(&self, x: f64, y: f64) -> Option<f64> {
        (y < STRIP_H)
            .then(|| ((x - KEYBOARD_W) / self.roll_w * self.length).clamp(0.0, self.length))
    }

    /// The note edge at a point. An edge inside its own note wins over a
    /// neighbour's edge just outside it, so where two notes touch, each
    /// keeps its own side.
    pub fn edge_at(&self, song: &Song, x: f64, y: f64) -> Option<Edge> {
        let p = self.point_at(x, y)?;
        let px = self.width_of(1.0);
        let margin = 4.0 / px;
        let mut outside: Option<(Edge, f64)> = None;
        for e in &song.events {
            if e.midi != p.midi || p.voice.as_ref().is_some_and(|v| v != &e.voice) {
                continue;
            }
            let (start, end) = (e.start as f64, e.end() as f64);
            let grip = 6f64.min(self.width_of(e.duration as f64) / 3.0) / px;
            if p.tick >= start && p.tick < start + grip && p.tick - start < end - p.tick {
                return Some(Edge {
                    event: e.clone(),
                    side: Side::Start,
                });
            }
            if p.tick <= end && p.tick > end - grip {
                return Some(Edge {
                    event: e.clone(),
                    side: Side::End,
                });
            }
            let before = start - p.tick;
            let after = p.tick - end;
            let nearer = |d: f64, o: &Option<(Edge, f64)>| {
                d > 0.0 && d <= margin && o.as_ref().is_none_or(|o| d < o.1)
            };
            if nearer(before, &outside) {
                outside = Some((
                    Edge {
                        event: e.clone(),
                        side: Side::Start,
                    },
                    before,
                ));
            }
            if nearer(after, &outside) {
                outside = Some((
                    Edge {
                        event: e.clone(),
                        side: Side::End,
                    },
                    after,
                ));
            }
        }
        outside.map(|o| o.0)
    }
}

/// The slot a click means: the note under it, else the slot at that time
/// in the voice sounding nearest that pitch. Given a lane's voice, only that voice.
pub fn slot_at(song: &Song, tick: f64, midi: i32, voice: Option<&str>) -> Option<usize> {
    if let Some(hit) = song.events.iter().find(|e| {
        e.midi == midi
            && voice.is_none_or(|v| v == e.voice)
            && tick >= e.start as f64
            && tick < e.end() as f64
    }) {
        return Some(hit.slot);
    }
    let mut best: Option<(usize, i32)> = None;
    let voices: Vec<&str> = match voice {
        Some(v) => vec![v],
        None => song.voices.iter().map(String::as_str).collect(),
    };
    for v in voices {
        let mut last: Option<i32> = None;
        for (i, s) in song.slots.iter().enumerate() {
            if s.voice != v {
                continue;
            }
            if let Some(&m) = s.midi.first() {
                last = Some(m);
            }
            if tick < s.start as f64 || tick >= s.end as f64 {
                continue;
            }
            if !s.editable {
                break;
            }
            let dist = last.map_or(24, |l| (l - midi).abs());
            if best.is_none_or(|b| dist < b.1) {
                best = Some((i, dist));
            }
            break;
        }
    }
    best.map(|b| b.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tune::compile;

    const BAR: f64 = (TICKS_PER_QUARTER * 4) as f64;
    const W: f64 = KEYBOARD_W + 800.0;
    const H: f64 = 600.0;
    const LEN: f64 = BAR * 8.0;

    fn song(src: &str) -> Song {
        compile(src).unwrap()
    }

    /// The x of tick `t` and the y of the middle of row `midi` in `voice`'s lane.
    fn at(l: &Layout, t: f64, midi: i32, voice: Option<&str>) -> (f64, f64) {
        let lane = voice.map_or(&l.lanes[0], |v| l.lane_of(v));
        (l.x_of(t), l.y_of(lane, midi) + l.row_h / 2.0)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn view(start: f64, end: f64) -> Option<View> {
        Some(View { start, end })
    }

    #[test]
    fn each_voice_gets_a_lane_in_order_all_with_one_row_height() {
        let l = layout(&song("RH: 1' 5' |\nLH: 1, |\n"), W, H, true, None);
        let voices: Vec<_> = l.lanes.iter().map(|ln| ln.voice.as_deref()).collect();
        assert_eq!(voices, [Some("RH"), Some("LH")]);
        let (rh, lh) = (&l.lanes[0], &l.lanes[1]);
        assert_eq!(rh.top, STRIP_H);
        assert!(close(lh.top, rh.bottom + LANE_GAP));
        assert!(close(lh.bottom, H));
        for ln in &l.lanes {
            assert!(close(
                (ln.bottom - ln.top) / (ln.hi - ln.lo + 1) as f64,
                l.row_h
            ));
        }
        assert_eq!((rh.lo, rh.hi), (69, 82));
        assert_eq!((lh.lo, lh.hi), (30, 42));
    }

    #[test]
    fn one_voice_or_lanes_off_shares_one_lane_at_least_two_octaves_tall() {
        let two = song("RH: 1 |\nLH: 1 |\n");
        for l in [
            layout(&song("RH: 1 2 |\n"), W, H, true, None),
            layout(&two, W, H, false, None),
        ] {
            assert_eq!(l.lanes.len(), 1);
            assert_eq!(l.lanes[0].voice, None);
            assert!(l.lanes[0].hi - l.lanes[0].lo >= 24);
        }
    }

    #[test]
    fn a_point_belongs_to_the_lane_it_is_in_gap_included_and_to_nothing_above_the_roll() {
        let s = song("RH: 1' |\nLH: 1, |\n");
        let l = layout(&s, W, H, true, None);
        let (rh, lh) = (&l.lanes[0], &l.lanes[1]);
        let voice = |x, y| l.point_at(x, y).and_then(|p| p.voice);
        assert_eq!(
            voice(KEYBOARD_W + 10.0, rh.top + 1.0).as_deref(),
            Some("RH")
        );
        assert_eq!(
            voice(KEYBOARD_W + 10.0, rh.bottom + LANE_GAP / 2.0).as_deref(),
            Some("RH")
        );
        assert_eq!(
            voice(KEYBOARD_W + 10.0, lh.top + 1.0).as_deref(),
            Some("LH")
        );
        assert_eq!(l.point_at(KEYBOARD_W + 10.0, STRIP_H - 1.0), None);
        assert_eq!(l.point_at(KEYBOARD_W - 1.0, 300.0), None);
        // The strip maps across the whole song, whatever is in view.
        let zoomed = layout(&s, W, H, true, view(0.0, BAR / 2.0));
        assert_eq!(zoomed.strip_at(KEYBOARD_W + 400.0, 5.0), Some(BAR / 2.0));
    }

    #[test]
    fn a_click_in_a_lane_picks_that_voice_even_where_another_is_nearer_in_pitch() {
        let s = song("RH: 1 . |\nLH: . 1 |\n");
        let lh = s.slots.iter().position(|x| x.voice == "LH");
        assert_eq!(slot_at(&s, 10.0, 60, Some("LH")), lh);
        assert_eq!(
            slot_at(&s, 10.0, 60, None),
            s.slots.iter().position(|x| x.voice == "RH")
        );
    }

    #[test]
    fn where_two_notes_touch_each_side_of_the_boundary_grabs_its_own_note() {
        let s = song("octave LH 4\nRH: . 1 . . |\nLH: . . 1 - |\n");
        let l = layout(&s, W, H, false, None);
        let (x, y) = at(&l, BAR / 2.0, 60, None);
        let right = l.edge_at(&s, x + 2.0, y).unwrap();
        assert_eq!(
            (right.side, right.event.voice.as_str()),
            (Side::Start, "LH")
        );
        let left = l.edge_at(&s, x - 2.0, y).unwrap();
        assert_eq!((left.side, left.event.voice.as_str()), (Side::End, "RH"));
    }

    #[test]
    fn a_lone_edge_can_be_grabbed_from_just_outside_the_note() {
        let s = song("RH: 1 . . . |\n");
        let l = layout(&s, W, H, false, None);
        let (x, y) = at(&l, BAR / 4.0, 60, None);
        assert_eq!(l.edge_at(&s, x + 3.0, y).map(|e| e.side), Some(Side::End));
        assert_eq!(l.edge_at(&s, x + 6.0, y), None);
        assert_eq!(l.edge_at(&s, l.x_of(BAR / 8.0), y), None);
        assert_eq!(l.edge_at(&s, x, y - l.row_h), None);
    }

    #[test]
    fn in_lanes_edges_only_match_the_lanes_voice() {
        let s = song("octave LH 4\nRH: 1 . |\nLH: 1 . |\n");
        let l = layout(&s, W, H, true, None);
        let (x, y) = at(&l, BAR / 2.0, 60, Some("LH"));
        assert_eq!(
            l.edge_at(&s, x - 1.0, y).map(|e| e.event.voice),
            Some("LH".into())
        );
    }

    #[test]
    fn zooming_keeps_the_tick_under_the_pointer_in_place() {
        let v = zoom(None, LEN, 4.0, LEN / 4.0).unwrap();
        assert_eq!(v.span(), LEN / 4.0);
        assert!(close((LEN / 4.0 - v.start) / v.span(), 0.25));
    }

    #[test]
    fn zoom_stops_at_a_quarter_note_and_at_the_whole_song() {
        assert_eq!(zoom(None, LEN, 1e6, 0.0), view(0.0, MIN_SPAN));
        assert_eq!(zoom(view(0.0, BAR), LEN, 1.0 / 100.0, 0.0), None);
    }

    #[test]
    fn panning_stays_inside_the_song_and_the_whole_song_does_not_pan() {
        assert_eq!(pan(view(0.0, BAR), LEN, -500.0), view(0.0, BAR));
        assert_eq!(pan(view(0.0, BAR), LEN, LEN * 2.0), view(LEN - BAR, LEN));
        assert_eq!(pan(None, LEN, 100.0), None);
    }

    #[test]
    fn revealing_scrolls_only_as_far_as_needed() {
        let v = view(BAR, BAR * 2.0);
        assert_eq!(reveal(v, LEN, BAR + 10.0, BAR + 20.0), v);
        assert_eq!(
            reveal(v, LEN, BAR * 2.0, BAR * 2.0 + 100.0),
            view(BAR + 100.0, BAR * 2.0 + 100.0)
        );
        assert_eq!(reveal(v, LEN, 0.0, 100.0), view(0.0, BAR));
    }

    #[test]
    fn following_turns_the_page_as_the_playhead_runs_off_and_after_a_loop() {
        let v = view(0.0, BAR);
        assert_eq!(follow(v, LEN, BAR - 10.0, BAR - 5.0), v);
        assert_eq!(
            follow(v, LEN, BAR - 10.0, BAR + 5.0),
            view(BAR + 5.0, BAR * 2.0 + 5.0)
        );
        assert_eq!(
            follow(view(BAR * 4.0, BAR * 5.0), LEN, BAR * 5.0 - 1.0, 0.0),
            view(0.0, BAR)
        );
        // Scrolled away by hand while playing: left alone until the playhead crosses the edge.
        assert_eq!(
            follow(view(BAR * 4.0, BAR * 5.0), LEN, 100.0, 200.0),
            view(BAR * 4.0, BAR * 5.0)
        );
    }

    #[test]
    fn a_view_is_pulled_back_inside_a_song_that_got_shorter() {
        assert_eq!(
            clamp(view(BAR * 6.0, BAR * 8.0), BAR * 4.0),
            view(BAR * 2.0, BAR * 4.0)
        );
        assert_eq!(clamp(view(0.0, BAR * 8.0), BAR * 4.0), None);
    }
}
