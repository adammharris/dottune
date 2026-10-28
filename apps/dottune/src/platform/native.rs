//! In a window: files in the config directory, a thread's sleep, the
//! desktop's own Open and Save dialogs, and the window's size from winit.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Poll, Waker};
use std::time::Instant;

use dioxus::prelude::*;

use super::{Opened, Startup};

fn store_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("dottune"))
}

/// What was kept under `key` the last time.
pub fn load(key: &str) -> Option<String> {
    std::fs::read_to_string(store_dir()?.join(key)).ok()
}

/// Keeps `value` under `key` for next time.
pub fn save(key: &str, value: &str) {
    if let Some(dir) = store_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(key), value);
    }
}

/// Seconds since the page started, from a monotonic clock.
pub fn now() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// A sleep on a thread of its own: the window's event loop has no timer to
/// lend a task.
pub fn sleep(ms: u64) -> Pin<Box<dyn Future<Output = ()>>> {
    struct Sleep(Arc<Mutex<(bool, Option<Waker>)>>);
    impl Future for Sleep {
        type Output = ();
        fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<()> {
            let mut s = self.0.lock().unwrap();
            if s.0 {
                Poll::Ready(())
            } else {
                s.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
    let state = Arc::new(Mutex::new((false, None::<Waker>)));
    let s = state.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let mut s = s.lock().unwrap();
        s.0 = true;
        if let Some(w) = s.1.take() {
            w.wake();
        }
    });
    Box::pin(Sleep(state))
}

/// The file named on the command line, if any.
pub fn startup() -> Startup {
    let Some(path) = std::env::args().nth(1) else {
        return Startup::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(source) => Startup {
            opened: Some(Opened {
                name: file_stem(&path),
                source,
            }),
            damaged_link: false,
        },
        Err(e) => {
            eprintln!("{path}: {e}");
            Startup::default()
        }
    }
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map_or_else(|| "tune".into(), |s| s.to_string_lossy().into_owned())
}

/// Asks for a `.tune` file and hands what it holds to `done`.
pub fn open_file(done: impl FnOnce(Opened) + 'static) {
    let picked = rfd::FileDialog::new()
        .add_filter("Tune", &["tune", "txt"])
        .set_title("Open a tune")
        .pick_file();
    if let Some(path) = picked
        && let Ok(source) = std::fs::read_to_string(&path)
    {
        done(Opened {
            name: file_stem(&path.to_string_lossy()),
            source,
        });
    }
}

/// Asks where to write `bytes`, named `filename` to start with.
pub fn save_file(filename: &str, _mime: &str, bytes: Vec<u8>) -> Result<(), String> {
    let ext = filename.rsplit('.').next().unwrap_or("");
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(filename)
        .add_filter(ext, &[ext])
        .save_file()
    else {
        return Ok(());
    };
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn copy_text(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| e.to_string())
}

/// The link that opens `hash` in the editor: the published one, from a window.
pub fn share_url(hash: &str) -> String {
    format!("{}{hash}", tune::share::SITE)
}

/// Whether Copy put the link where it was asked to go.
pub const COPIES_LINKS: bool = true;

/// The window's size in CSS pixels, kept up to date.
pub fn use_viewport() -> Signal<(f64, f64)> {
    let window = dioxus_native::use_window();
    let size = move |w: &dyn dioxus_native::winit::window::Window| {
        let s = w.surface_size();
        let scale = w.scale_factor();
        (s.width as f64 / scale, s.height as f64 / scale)
    };
    let initial = size(window.as_ref());
    let mut viewport = use_signal(|| initial);
    let win = window.clone();
    dioxus_native::use_window_event(move |event, _| {
        use dioxus_native::winit::event::WindowEvent;
        if matches!(
            event,
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. }
        ) {
            let now = size(win.as_ref());
            if *viewport.peek() != now {
                viewport.set(now);
            }
        }
    });
    viewport
}

/// Names the window after the tune.
pub fn use_title(title: String) {
    let window = dioxus_native::use_window();
    use_effect(use_reactive!(|title| window.set_title(&title)));
}
