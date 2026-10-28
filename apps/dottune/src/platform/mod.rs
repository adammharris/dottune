//! What the page needs from where it runs: somewhere to keep things, a
//! clock and a sleep, files in and out, the clipboard, and the window's
//! size. A window drawn by Blitz has them from the OS (`native.rs`), a
//! browser tab from the browser (`web.rs`).

#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
pub use native::*;

#[cfg(all(feature = "web", not(feature = "native")))]
mod web;
#[cfg(all(feature = "web", not(feature = "native")))]
pub use web::*;

/// A tune to open at start: from the command line, or a share link.
#[derive(Clone, Debug, Default)]
pub struct Opened {
    pub name: String,
    pub source: String,
}

/// What the page opens with: a tune it was given, if any, and whether a
/// link it was given was damaged.
#[derive(Clone, Debug, Default)]
pub struct Startup {
    pub opened: Option<Opened>,
    pub damaged_link: bool,
}
