//! Port of test/share.test.ts.

use tune::share::{Shared, read_share_hash, share_hash};

/// web/examples.ts's EXAMPLES: the same tunes as examples/*.tune, by name.
const EXAMPLES: [(&str, &str); 5] = [
    (
        "Mary had a little lamb",
        include_str!("../../../examples/mary-had-a-little-lamb.tune"),
    ),
    (
        "Ode to Joy",
        include_str!("../../../examples/ode-to-joy.tune"),
    ),
    (
        "Minor groove",
        include_str!("../../../examples/minor-groove.tune"),
    ),
    (
        "Key and tempo changes",
        include_str!("../../../examples/modulation.tune"),
    ),
    (
        "Waltz with stacks",
        include_str!("../../../examples/waltz-with-stacks.tune"),
    ),
];

/// `^#n=[^#]*&t=[A-Za-z0-9_-]+$`
fn looks_like_a_share_hash(hash: &str) -> bool {
    let Some(rest) = hash.strip_prefix("#n=") else {
        return false;
    };
    let Some((name, t)) = rest.split_once("&t=") else {
        return false;
    };
    !name.contains('#')
        && !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[test]
fn a_share_link_round_trips_the_source_and_name() {
    for (name, source) in EXAMPLES {
        let hash = share_hash(source, name);
        assert!(looks_like_a_share_hash(&hash), "{hash}");
        assert_eq!(
            read_share_hash(&hash).unwrap(),
            Some(Shared {
                name: Some(name.to_string()),
                source: source.to_string()
            })
        );
    }
}

#[test]
fn non_ascii_text_survives() {
    let source = "// café ♩ 🎹\nRH: 1 |\n";
    assert_eq!(
        read_share_hash(&share_hash(source, "naïve"))
            .unwrap()
            .map(|s| s.source)
            .as_deref(),
        Some(source)
    );
}

#[test]
fn a_hash_without_a_tune_shares_nothing() {
    assert_eq!(read_share_hash(""), Ok(None));
    assert_eq!(read_share_hash("#section"), Ok(None));
}

#[test]
fn a_damaged_link_throws() {
    assert!(read_share_hash("#t=AAAA").is_err());
}
