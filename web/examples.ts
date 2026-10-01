export const EXAMPLES: Record<string, string> = {
  "Mary had a little lamb": `key C major
tempo 100
time 4/4

RH: 3 2 1 2 | 3 3 3 - | 2 2 2 - | 3 5 5 - |
LH: I       | I       | V       | I       |

RH: 3 2 1 2 | 3 3 3 3 | 2 2 3 2 | 1 - - - |
LH: I       | I       | V       | I       |
`,

  "Ode to Joy": `key C major
tempo 120
time 4/4

// Eight slots per bar makes the dotted rhythm in bar 4 easy.
RH: 3 3 4 5 | 5 4 3 2 | 1 1 2 3 | 3 - - 2 2 - - - |
LH: I       | V       | I       | V                |

RH: 3 3 4 5 | 5 4 3 2 | 1 1 2 3 | 2 - - 1 1 - - - |
LH: I       | V       | I       | V   I            |
`,

  "Minor groove": `key A minor
tempo 96
time 4/4
octave LH 2
octave Bass 1

RH: 5 [5 6] 5 3 | 1 - - . | 4 [4 5] 4 2 | #7 - - . |
LH: i           | i       | iv          | V7       |
Bass: 1 . [1 1] . | 1 . [1 5] . | 4 . [4 4] . | 5 . [5 5] . |
`,

  "Key and tempo changes": `key C major
tempo 100

RH: 3 2 1 2 | 3 3 3 - | 2 2 2 - | 3 5 5 - |
LH: I       | I       | V       | I       |

// up a whole step, a little faster
key D major
tempo 112
RH: 3 2 1 2 | 3 3 3 3 | 2 2 3 2 | 1 - - - |
LH: I       | I       | V       | I       |

// waltz time to finish
time 3/4
RH: 3 - 5 | 1 - - |
LH: I     | I     |
`,

  "Waltz with stacks": `key F major
tempo 132
time 3/4
octave Chords 3
octave Bass 2

RH: 5 - 3 | 4 - 2 | 3 [4 3] 2 | 1 - - |
Chords: . 1+3+5 1+3+5 | . 2+4+7 2+4+7 | . 1+4+6 1+4+6 | . 1+3+5 - |
Bass: 1 . . | 5 . . | 4 . . | 1 . . |
`,
};
