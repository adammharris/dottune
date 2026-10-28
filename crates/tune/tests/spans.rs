//! Port of test/spans.test.ts. Spans are byte offsets here (UTF-16 in the
//! TypeScript); every source below is ASCII, so they are the same.

use tune::compile;

fn sources() -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    // web/examples.ts's EXAMPLES are the same tunes as examples/*.tune, so the
    // directory covers them.
    out.push("key C\r\ntempo 90\r\ntime 3/4\r\n\r\n  RH: [1 2] [3 [4 5]]|5' - - | . // comment\r\nLH:1|I7/3\r\n".into());
    out
}

#[test]
fn every_written_slots_span_points_at_its_text() {
    for src in sources() {
        for s in compile(&src).unwrap().slots {
            if !s.editable {
                assert_eq!(s.span, None);
            } else {
                let (a, b) = s.span.unwrap();
                assert_eq!(&src[a..b], s.text);
            }
        }
    }
}
