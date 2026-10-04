# Tune format — v0.3 draft

A plain-text music format for people who think at a keyboard.
Three ideas carry the whole design:

1. **Spacing is time.** A bar is split evenly among its tokens.
   You never write a duration.
2. **Scale degrees, not letter names.** `1` is the tonic of the current key.
   Change the key line and the whole piece transposes.
3. **Every note says its own octave.** A degree sounds in its voice's octave
   unless it carries `'` or `,` marks. It never depends on the note before it.

Working file extension: `.tune`. (Name is a placeholder.)

```tune
key C major
tempo 100

RH: 3 2 1 2 | 3 3 3 - | 2 2 2 - | 3 5 5 - |
LH: I       | I       | V,      | I       |
```

---

## 1. File structure

A file is a **header** followed by one or more **blocks**, with optional **changes** between blocks.

- **Header:** directive lines (§2) before the first block.
- **Block:** a run of consecutive voice lines. Blocks are separated by one or more blank lines.
  Blocks play one after another, like systems on a page.
- **Change:** directive lines between blocks (§2.1). They start after a blank line and run up to the block they apply to.
- **Voice line:** `Name: bar | bar | bar |`
  - `Name` matches `[A-Za-z][A-Za-z0-9_]*`. `RH` and `LH` are conventional, not special — except for their default octave (§4.2).
  - The directive words `key`, `tempo`, `time`, and `octave` are reserved and cannot name a voice.
  - The trailing `|` is optional. An empty bar (`||`, or `|` with only whitespace between) is an error.
  - A voice may appear at most once per block.
- **Comments:** `//` to end of line. A line that is only a comment is ignored entirely (it does not end a block).

### 1.1 Block length and padding

A block's length is the bar count of its longest voice line.
Any voice that is shorter than the block, or absent from the block,
is padded with rests to the block's length.
The next block begins after that.

A block can be read on its own: its pitches depend only on its tokens and the settings in force (§4.2).

---

## 2. Directives

| Directive | Example | Default |
|---|---|---|
| `key <tonic> [mode]` | `key Eb major`, `key A minor`, `key D dorian` | `key C major` |
| `tempo <bpm>` | `tempo 96` (quarter notes per minute) | `tempo 120` |
| `time <n>/<d>` | `time 3/4`, `time 6/8` | `time 4/4` |
| `octave <voice> <n>` | `octave LH 2` | `LH` → 3, all others → 4 |

- `<tonic>` is a letter `A`–`G` with optional `#` or `b`.
- `<bpm>` is a number from 4 to 1000; decimals are allowed.
- In `time <n>/<d>`, `<n>` is a whole number from 1 to 64 and `<d>` is a power of two from 1 to 64.
- In `octave <voice> <n>`, `<voice>` cannot be a directive word, and `<n>` is a whole number (it may be negative).
- `[mode]` is one of `major`, `minor`, `ionian`, `dorian`, `phrygian`, `lydian`, `mixolydian`, `aeolian`, `locrian`. `major` = `ionian`, `minor` = `aeolian` (natural minor).
- Repeating a directive in the same header or change: the last one wins (for `octave`, per voice).
- An unknown directive is an error.

### 2.1 Changes between blocks

Directives may also appear between blocks. Such a run of directive lines is a **change**:
it applies from the next block onward, until changed again.

```tune
key C major
RH: 1 2 3 - |

key G major
tempo 90
RH: 1 2 3 - |
```

- A change must come after a blank line — a directive directly below a voice line, inside a block, is an error.
- A change must be followed by a block. Directives after the last block are an error.
- **`tempo`** changes playback speed only; ticks are unaffected.
- **`time`** changes the bar length of the following blocks.
- **`key`** changes the tonic and scale. Each voice keeps its octave number,
  so its degrees now count up from the new tonic in that octave (§4.2).
  A `key` directive that repeats the current key changes nothing.
- **`octave <voice> <n>`** changes the voice's octave from the next block on.

---

## 3. Time

### 3.1 Bars

A bar lasts `n × 4 / d` quarter notes (`4/4` → 4, `3/4` → 3, `6/8` → 3).
The bar is divided **evenly** among its top-level tokens.

```
| 1 |                 one whole-bar note
| 1 2 |               two halves
| 1 2 3 4 |           four quarters (in 4/4)
| 1 2 3 4 5 6 7 1 |   eight eighths
```

### 3.2 Slot tokens

Every token fills exactly one slot.

| Token | Meaning |
|---|---|
| a note, stack, or chord (§4, §5) | starts sounding |
| `-` | **hold**: extends whatever occupied the previous slot (note, stack, chord, or rest). Works across barlines and across blocks. A `-` with nothing before it in the voice is a rest. |
| `.` | **rest** |
| `[ ... ]` | **group**: takes one slot and divides it evenly among its contents. Groups nest. |

`[` and `]` need not be separated by spaces from what they enclose: `[1 2]` and `[ 1 2 ]` are the same.
An empty group `[]` or unbalanced brackets are errors.

Groups give you everything a plain grid can't:

```
1 [2 3] 4 5        quarter, two eighths, quarter, quarter
[1 - - 2] 3 - -    dotted-eighth + sixteenth, then a dotted half
1 [2 3 4] 5 6      a triplet in the second beat
```

### 3.3 Ticks

Output time is measured in ticks at **480 per quarter note**.
Positions are computed exactly (as fractions) and rounded to the nearest tick only at output:
`start = round(exact_start)`, `duration = round(exact_end) − start`.

---

## 4. Notes (scale degrees)

### 4.1 Syntax

```
[accidental] degree [octave marks]
     #|b      1–7      ' (up)  , (down)   — any number
```

Examples: `1`, `5`, `b3`, `#4`, `1'`, `5,,`.

The degree selects a pitch from the key's mode; the accidental raises/lowers it one semitone.
In `key A minor`, `3` is C and `#7` is G#.

### 4.2 Octaves

Each voice has an **octave** `n`: from `octave <voice> <n>`, or 3 for `LH` and 4 for every other voice.
The degrees `1`–`7` with no marks are the seven scale notes counting **upward** from the tonic in octave `n`.
Each `'` raises a note an octave; each `,` lowers it one.

A note's pitch depends only on its own token, the key, and its voice's octave — never on the notes before it.
So `7 1` falls a seventh (`7 1'` rises a step), and `1 5,` drops a fourth.

Middle C is `C4` = MIDI 60. In `key C major`, `RH`'s `5` is G4; in `key A minor`, `RH`'s `1` is A4 and `3` is C5.

Every pitch a token sounds must lie in MIDI's range, `C-1` (0) to `G9` (127). A note, stack, or chord with any pitch outside it is an error.

---

## 5. Harmony

### 5.1 Stacks

`a+b+c` sounds several degrees at once (e.g. `1+3+5`).

- Each member is placed like a note (§4.2), on its own: `1+3+5` is a root-position triad,
  and `5,+1+3` puts the fifth underneath.
- Members may be written in any order; they sound together either way.

### 5.2 Chords (Roman numerals)

```
[accidental] numeral [quality] [7|maj7] [octave marks] [/bass]
```

- **Numeral:** `I`–`VII` builds a **major** triad on that degree; `i`–`vii` builds a **minor** triad.
- **Accidental:** `bVII`, `#iv` — shifts the root by a semitone.
- **Quality suffix** (overrides the case):

  | Suffix | Triad | With `7` |
  |---|---|---|
  | (none, upper) | major `0 4 7` | dominant `0 4 7 10` |
  | (none, lower) | minor `0 3 7` | minor 7 `0 3 7 10` |
  | `o` | diminished `0 3 6` | `o7` fully diminished `0 3 6 9` |
  | `h` | — | `h7` half-diminished `0 3 6 10` (`h` requires `7`) |
  | `+` | augmented `0 4 8` | `+7` `0 4 8 10` |

  `maj7` adds a major seventh (`11`) to any triad: `Imaj7`, `imaj7`.

- **Root placement:** the root is placed like a note (§4.2); octave marks apply to the root. Chord tones are stacked upward from the root in close position, using the semitone intervals above.
- **Slash bass:** `/degree` (optionally with `#`/`b`) adds that degree as an extra note at the nearest copy **strictly below** the root. `I/3` = first-inversion sound with E in the bass.

A voice can freely mix notes, stacks, and chords.

---

## 6. Output: the event list

Compiling a file produces a song: `tempo`, `time`, `key`, the changes between blocks, and a list of note events.
The **event list** is the format's contract — tests, the MIDI exporter, and the player all consume it.

Text form, one event per line:

```
<start> <duration> <voice> <pitch>
```

and, where a change between blocks alters the key, tempo, or time signature, one line per altered setting:

```
<start> key <tonic> <mode>
<start> tempo <bpm>
<start> time <n>/<d>
```

The header's settings are not listed; `octave` changes appear only through the pitches they produce.

- `start`, `duration`: ticks (§3.3).
- `pitch`: scientific pitch name, **sharps only**: `C C# D D# E F F# G G# A A# B`, then the octave. `C4` = MIDI 60.
- A held note (`-`) is one event, not several.
- Sort order: `start`; at the same start, changes (`time`, then `key`, then `tempo`) before notes;
  then voice (in order of the voice's first appearance in the file), then pitch (low to high).
- `key` is written with the mode in lower case as given (`key G major`, `key A minor`).

---

## 7. Errors

Errors are reported with a 1-based line number.
At minimum, these are errors: unknown token, unknown directive, directive inside a block, directive after the last block, reserved voice name (in a voice line or an `octave` directive), invalid key, tempo out of range, invalid time signature, pitch out of range, empty bar, empty group, unbalanced brackets, duplicate voice in a block, malformed voice line.

A file may contain several errors. An implementation may report them all, but must report the one on the lowest line; the conformance examples below check that line.

---

## 8. Examples (conformance tests)

Every `tune` block below is followed by its expected `events` (or `error`) block.
These are extracted mechanically into the test suite — keep them exact.

### 8.1 Mary had a little lamb

```tune
key C major
tempo 100
RH: 3 2 1 2 | 3 3 3 - | 2 2 2 - | 3 5 5 - |
```

```events
0 480 RH E4
480 480 RH D4
960 480 RH C4
1440 480 RH D4
1920 480 RH E4
2400 480 RH E4
2880 960 RH E4
3840 480 RH D4
4320 480 RH D4
4800 960 RH D4
5760 480 RH E4
6240 480 RH G4
6720 960 RH G4
```

### 8.2 Eight to the bar; `7 1'` reaches the next tonic

```tune
RH: 1 2 3 4 5 6 7 1' |
```

```events
0 240 RH C4
240 240 RH D4
480 240 RH E4
720 240 RH F4
960 240 RH G4
1200 240 RH A4
1440 240 RH B4
1680 240 RH C5
```

### 8.3 Octave marks

```tune
RH: 1 1' 1 1, |
```

```events
0 480 RH C4
480 480 RH C5
960 480 RH C4
1440 480 RH C3
```

### 8.4 Degrees count up from the tonic, whatever came before

```tune
RH: 1 7 7, 5, |
```

```events
0 480 RH C4
480 480 RH B4
960 480 RH B3
1440 480 RH G3
```

### 8.5 Whole notes and rests

```tune
RH: 1 | . 3 . 5 |
```

```events
0 1920 RH C4
2400 480 RH E4
3360 480 RH G4
```

### 8.6 Holds cross barlines

```tune
RH: 1 2 3 5 | - - 3 . |
```

```events
0 480 RH C4
480 480 RH D4
960 480 RH E4
1440 1440 RH G4
2880 480 RH E4
```

### 8.7 A hold after a rest is a rest

```tune
RH: . - 1 - |
```

```events
960 960 RH C4
```

### 8.8 Groups

```tune
RH: 1 [2 3] 4 [5 6 7] |
```

```events
0 480 RH C4
480 240 RH D4
720 240 RH E4
960 480 RH F4
1440 160 RH G4
1600 160 RH A4
1760 160 RH B4
```

### 8.9 Dotted rhythm with a group

```tune
RH: [1 - - 2] 3 - - |
```

```events
0 360 RH C4
360 120 RH D4
480 1440 RH E4
```

### 8.10 Accidentals

```tune
RH: 1 #4 5 b7 |
```

```events
0 480 RH C4
480 480 RH F#4
960 480 RH G4
1440 480 RH A#4
```

### 8.11 Minor key, raised seventh

```tune
key A minor
RH: 1 3 5 #7 | 1' |
```

```events
0 480 RH A4
480 480 RH C5
960 480 RH E5
1440 480 RH G#5
1920 1920 RH A5
```

### 8.12 Other keys and 3/4

```tune
key G major
time 3/4
RH: 1 2 3 | 5 |
```

```events
0 480 RH G4
480 480 RH A4
960 480 RH B4
1440 1440 RH D5
```

### 8.13 Compound time

```tune
time 6/8
RH: 1 - - 5, - - | 1 |
```

```events
0 720 RH C4
720 720 RH G3
1440 1440 RH C4
```

### 8.14 Two hands

```tune
RH: 3 2 1 2 | 3 3 3 - |
LH: I       | V,      |
```

```events
0 480 RH E4
0 1920 LH C3
0 1920 LH E3
0 1920 LH G3
480 480 RH D4
960 480 RH C4
1440 480 RH D4
1920 480 RH E4
1920 1920 LH G2
1920 1920 LH B2
1920 1920 LH D3
2400 480 RH E4
2880 960 RH E4
```

### 8.15 Chord roots take octave marks

```tune
LH: I vi, IV, V7, |
```

```events
0 480 LH C3
0 480 LH E3
0 480 LH G3
480 480 LH A2
480 480 LH C3
480 480 LH E3
960 480 LH F2
960 480 LH A2
960 480 LH C3
1440 480 LH G2
1440 480 LH B2
1440 480 LH D3
1440 480 LH F3
```

### 8.16 Chord qualities and borrowed chords

```tune
LH: ii7 viio, Imaj7 bVII, |
```

```events
0 480 LH D3
0 480 LH F3
0 480 LH A3
0 480 LH C4
480 480 LH B2
480 480 LH D3
480 480 LH F3
960 480 LH C3
960 480 LH E3
960 480 LH G3
960 480 LH B3
1440 480 LH A#2
1440 480 LH D3
1440 480 LH F3
```

### 8.17 Slash bass

```tune
LH: I/3 |
```

```events
0 1920 LH E2
0 1920 LH C3
0 1920 LH E3
0 1920 LH G3
```

### 8.18 Stacks

```tune
RH: 1+3+5 5,+1+3 |
```

```events
0 960 RH C4
0 960 RH E4
0 960 RH G4
960 960 RH G3
960 960 RH C4
960 960 RH E4
```

### 8.19 Holding a chord

```tune
LH: I - IV - |
```

```events
0 960 LH C3
0 960 LH E3
0 960 LH G3
960 960 LH F3
960 960 LH A3
960 960 LH C4
```

### 8.20 Blocks, padding, and comments

```tune
// Blocks play in sequence; short voices are padded with rests.
RH: 1 2 | 3 4 |
LH: I |   // LH rests in bar 2

RH: 5 - - - |
LH: V, |
```

```events
0 960 RH C4
0 1920 LH C3
0 1920 LH E3
0 1920 LH G3
960 960 RH D4
1920 960 RH E4
2880 960 RH F4
3840 1920 RH G4
3840 1920 LH G2
3840 1920 LH B2
3840 1920 LH D3
```

### 8.21 Octave directive

```tune
octave LH 2
octave RH 5
RH: 1 |
LH: 1 |
```

```events
0 1920 RH C5
0 1920 LH C2
```

### 8.22 Errors

```tune
RH: 1 2 x 4 |
```

```error
line 1
```

```tune
RH: 1 [2 3 4 |
```

```error
line 1
```

```tune
RH: 1 || 2 |
```

```error
line 1
```

```tune
RH: 1 2 3 4 |
key D major
```

```error
line 2
```

```tune
RH: 1 |
RH: 2 |
```

```error
line 2
```

```tune
key H major
RH: 1 |
```

```error
line 1
```

A tempo too slow for MIDI:

```tune
tempo 2
RH: 1 |
```

```error
line 1
```

A beat that is not a power of two:

```tune
time 4/3
RH: 1 |
```

```error
line 1
```

A pitch above `G9`:

```tune
RH: 1 2 |
RH2: 1'''''''' |
```

```error
line 2
```

A reserved name in an `octave` directive:

```tune
octave key 3
RH: 1 |
```

```error
line 1
```

With several errors, the lowest line is the one checked:

```tune
RH: 1 x |
LH: I [ |
```

```error
line 1
```

### 8.23 Tempo change

```tune
tempo 100
RH: 1 2 |

tempo 80
RH: 3 |
```

```events
0 960 RH C4
960 960 RH D4
1920 tempo 80
1920 1920 RH E4
```

### 8.24 Time signature change

```tune
RH: 1 2 3 4 |

time 3/4
RH: 5 4 3 |
```

```events
0 480 RH C4
480 480 RH D4
960 480 RH E4
1440 480 RH F4
1920 time 3/4
1920 480 RH G4
2400 480 RH F4
2880 480 RH E4
```

### 8.25 Key change: degrees count from the new tonic

```tune
key C major
RH: 1 2 3 - |

key G major
RH: 1 2 3 - |
```

```events
0 480 RH C4
480 480 RH D4
960 960 RH E4
1920 key G major
1920 480 RH G4
2400 480 RH A4
2880 960 RH B4
```

### 8.26 Same tonic, new mode

```tune
RH: 3 |

key C minor
RH: 3 |
```

```events
0 1920 RH E4
1920 key C minor
1920 1920 RH D#4
```

### 8.27 Octave change

```tune
RH: 1 2 |

octave RH 5
RH: 1 2 |
```

```events
0 960 RH C4
960 960 RH D4
1920 960 RH C5
2880 960 RH D5
```

### 8.28 A hold carries across a change

```tune
RH: 1 - |

tempo 60
RH: - 2 |
```

```events
0 2880 RH C4
1920 tempo 60
2880 960 RH D4
```

### 8.29 Change errors

A directive inside a block:

```tune
RH: 1 |
tempo 90
RH: 2 |
```

```error
line 2
```

A directive with no block after it:

```tune
RH: 1 |

tempo 90
```

```error
line 3
```

A reserved voice name:

```tune
time: 1 |
```

```error
line 1
```

---

## 9. Not in v0.3

Ideas deliberately left out, roughly in order of likely value:

- Dynamics and accents (velocity is fixed for now).
- Chord patterns: `I ~arp8` to arpeggiate, `I ~alberti`, etc.
- Changes in the middle of a block (they apply between blocks only).
- Pickup (anacrusis) bars.
- Repeats and named sections (`[A]`, `play A A B A`).
- Drum voices.
- Chord voice-leading (chords are close position above their root).

## 10. Open questions

- Should the case of a numeral be **ignored** when a quality suffix is present (current rule), or should `Io` be an error?
- Is LH's default octave 3 right for chords? In `key C`, `V` sits at G3–D4, so chord parts often want `octave LH 2` or `V,`.
- Should a voice's octave start at the tonic (current rule) or at C, as pitch names do? In `key Bb major`, `octave RH 4` spans B♭4–A5.
