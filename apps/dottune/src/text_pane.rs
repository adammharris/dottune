//! The source pane: the text of the tune, its line numbers, and every error
//! in it, each where it is.

use dioxus::prelude::*;

use crate::app::use_app;

#[component]
pub fn TextPane(width: f64, height: f64) -> Element {
    let app = use_app();
    let focus = unit::use_focus(unit::Kind::Text, |_, _| false);
    let source = app.source.read().clone();
    let errors = app.errors.read().clone();
    // An error's place is only good while the text is what it was found in.
    let fresh = *app.error_source.read() == source;
    let lines = source.split('\n').count();
    let error_lines: Vec<usize> = if fresh {
        errors.iter().map(|e| e.line).collect()
    } else {
        vec![]
    };
    let list_h = if errors.is_empty() {
        0.0
    } else {
        (errors.len().min(5) as f64) * 20.0 + 12.0
    };
    let editor_h = height - list_h;
    let selected = {
        let song = app.song.read();
        let compiled = app.compiled.read();
        app.read_selected()
            .zip(song.as_ref())
            .filter(|_| *compiled == source)
            .and_then(|(i, s)| s.slots[i].span.map(|sp| (s.slots[i].text.clone(), sp)))
    };
    let selected_line =
        selected.map(|(_, (start, _))| source[..start.min(source.len())].matches('\n').count() + 1);

    rsx! {
        section { class: "text-pane", style: "width: {width}px; height: {height}px;",
            div { class: "editor", style: "height: {editor_h}px;",
                div { class: "gutter", "aria-hidden": "true",
                    for n in 1..=lines {
                        div {
                            key: "{n}",
                            class: if error_lines.contains(&n) { "gutter-line err" } else if Some(n) == selected_line { "gutter-line sel" } else { "gutter-line" },
                            "{n}"
                        }
                    }
                }
                textarea {
                    class: "source{focus.class()}",
                    "aria-label": "The tune",
                    spellcheck: "false",
                    autocomplete: "off",
                    value: "{source}",
                    onpointerdown: move |_| {
                        focus.press();
                        app.history.write_unchecked().stop_typing();
                    },
                    onfocus: move |_| focus.dom_focus(),
                    onblur: move |_| focus.dom_blur(),
                    oninput: move |e: FormEvent| app.typed(e.value()),
                }
            }
            if !errors.is_empty() {
                ul { class: "errors", style: "height: {list_h}px;", "aria-label": "Errors",
                    for (i, e) in errors.iter().enumerate() {
                        li { key: "{i}", class: if fresh { "" } else { "stale" },
                            span { class: "where", "{app.where_is(e)}" }
                            " {e.reason}"
                        }
                    }
                }
            }
        }
    }
}
