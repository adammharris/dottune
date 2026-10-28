//! The tunes in examples/, to start from or to learn the format by.

pub const EXAMPLES: [(&str, &str); 5] = [
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
