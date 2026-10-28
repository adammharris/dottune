//! The tune editor: the text of a tune beside a piano roll that plays it
//! and edits it, in a window drawn by Blitz (`native`) or a browser
//! (`web`).
//!
//! The page is laid out from the window's size, which it knows, rather
//! than measured, which Blitz cannot do from a handler: the header, the
//! structure bar and the status line have fixed heights, the text pane a
//! fixed width, and the roll the rest.

mod app;
mod audio;
mod examples;
mod geometry;
mod history;
mod platform;
mod roll;
mod structure_bar;
mod text_pane;
mod toolbar;

use dioxus::prelude::*;
use unit::prelude::*;

use app::use_app_provider;

const HEADER_H: f64 = 88.0;
const STRUCTURE_H: f64 = 40.0;
const FOOTER_H: f64 = 28.0;
const TEXT_W: f64 = 380.0;

const CSS: &str = include_str!("style.css");

fn main() {
    #[cfg(feature = "native")]
    {
        use dioxus_native::{Config, LogicalSize, WindowAttributes};
        let window = WindowAttributes::default()
            .with_title("tune")
            .with_surface_size(LogicalSize::new(1280.0, 800.0));
        dioxus_native::launch_cfg(
            Root,
            vec![],
            vec![Box::new(Config::new().with_window_attributes(window))],
        );
    }
    #[cfg(all(feature = "web", not(feature = "native")))]
    {
        console_error_panic_hook::set_once();
        dioxus_web::launch::launch_cfg(Root, dioxus_web::Config::new());
    }
}

#[allow(non_snake_case)]
fn Root() -> Element {
    rsx! {
        UnitRoot { appearance: Appearance::Dark, accent: Accent::System, class: "dottune",
            style { {CSS} }
            Page {}
        }
    }
}

#[component]
fn Page() -> Element {
    let app = use_app_provider();
    let unit = use_unit();
    let viewport = platform::use_viewport();
    platform::use_title(format!("{} · tune", app.name.read()));
    let (vw, vh) = viewport();
    let text_open = *app.text_open.read();
    let main_h = (vh - HEADER_H - FOOTER_H).max(120.0);
    let text_w = if text_open {
        TEXT_W.min(vw * 0.45)
    } else {
        0.0
    };
    let roll_w = (vw - text_w).max(200.0);
    let roll_h = (main_h - STRUCTURE_H).max(80.0);
    let status = app.status.read().clone();

    // ⌘Z, ⌘⇧Z and ⌘Y undo and redo, in the text and on the roll alike; ⌘↵
    // plays, ⌘\ folds the text, ⌘S saves, ⌘O opens. A menu or dialog open
    // has every key first.
    let onkeydown = move |e: KeyboardEvent| {
        if unit.is_open() {
            return;
        }
        let mods = e.modifiers();
        if !(mods.contains(Modifiers::META) || mods.contains(Modifiers::CONTROL)) {
            return;
        }
        let handled = match e.key() {
            Key::Named(NamedKey::Enter) => {
                app.toggle_play();
                true
            }
            Key::Character(c) => match c.to_lowercase().as_str() {
                "z" if mods.contains(Modifiers::SHIFT) => {
                    app.redo();
                    true
                }
                "z" => {
                    app.undo();
                    true
                }
                "y" => {
                    app.redo();
                    true
                }
                "\\" => {
                    app.set_text_open(!text_open);
                    true
                }
                "s" => {
                    app.save();
                    true
                }
                "o" => {
                    app.open();
                    true
                }
                _ => false,
            },
            _ => false,
        };
        if handled {
            e.prevent_default();
            e.stop_propagation();
        }
    };

    rsx! {
        div { class: "page", style: "width: {vw}px; height: {vh}px;", onkeydown,
            toolbar::Toolbar {}
            main { class: "main", style: "height: {main_h}px;",
                if text_open {
                    text_pane::TextPane { width: text_w, height: main_h }
                }
                section { class: "stage", style: "width: {roll_w}px;",
                    structure_bar::StructureBar {}
                    roll::Roll { width: roll_w, height: roll_h }
                }
            }
            footer {
                class: if status.error { "status error" } else { "status" },
                role: "status",
                "{status.text}"
            }
        }
    }
}
