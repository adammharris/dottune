# dottune

A plain-text music format for people who think at a keyboard (SPEC.md), and
an editor for it: the text of a tune beside a piano roll that plays it and
edits it. The editor is a Dioxus app on [unit](https://github.com/diaryx-org/unit)'s
components, in a window drawn by Blitz or in a browser.

```sh
cargo test --workspace                                # the format, SPEC.md's examples, the editor's geometry
cargo run -p dottune                                  # the editor, in a window
cargo run -p dottune -- examples/ode-to-joy.tune      # … on a file
apps/dottune/web/build.sh                             # the editor as a static site, in target/web
cargo run -p tune --bin events -- examples/ode-to-joy.tune      # a tune's event list (SPEC §6)
cargo run -p tune --bin midi -- examples/ode-to-joy.tune out.mid
```

- `crates/tune`: the format. The compiler, the edits the editor makes (as
  fig splices, through the tune language in `figlang.rs`), MIDI, the tempo
  map and share links.
- `apps/dottune`: the editor. `--features web --no-default-features` for the
  browser; the Pages workflow builds and publishes it from `main`.

`.fig/languages.figl` gives the `fig` CLI the tune language, through the
`fig-tune` helper.
