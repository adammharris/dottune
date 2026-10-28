//! In a browser tab: local storage, `setTimeout`, a file input and a
//! download, the async clipboard, and the tab's size.

use std::future::Future;
use std::pin::Pin;

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use super::{Opened, Startup};

fn window() -> web_sys::Window {
    web_sys::window().expect("a page has a window")
}

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

pub fn load(key: &str) -> Option<String> {
    storage()?.get_item(&format!("tune:{key}")).ok().flatten()
}

pub fn save(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(&format!("tune:{key}"), value);
    }
}

pub fn now() -> f64 {
    js_sys::Date::now() / 1000.0
}

pub fn sleep(ms: u64) -> Pin<Box<dyn Future<Output = ()>>> {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
    });
    Box::pin(async move {
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    })
}

/// The tune a share link carries, if the page was opened with one. The
/// hash is cleared, so a reload opens what was last edited.
pub fn startup() -> Startup {
    let location = window().location();
    let hash = location.hash().unwrap_or_default();
    if hash.is_empty() {
        return Startup::default();
    }
    if let Ok(history) = window().history() {
        let path = location.pathname().unwrap_or_default() + &location.search().unwrap_or_default();
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
    }
    match tune::share::read_share_hash(&hash) {
        Ok(Some(shared)) => Startup {
            opened: Some(Opened {
                name: shared.name.unwrap_or_else(|| "tune".into()),
                source: shared.source,
            }),
            damaged_link: false,
        },
        Ok(None) => Startup::default(),
        Err(_) => Startup {
            opened: None,
            damaged_link: true,
        },
    }
}

/// Asks for a `.tune` file, through a file input, and hands what it holds to `done`.
pub fn open_file(done: impl FnOnce(Opened) + 'static) {
    let document = window().document().unwrap();
    let input: web_sys::HtmlInputElement =
        document.create_element("input").unwrap().unchecked_into();
    input.set_type("file");
    input.set_accept(".tune,.txt,text/plain");
    let picked = input.clone();
    let mut done = Some(done);
    let onchange = Closure::<dyn FnMut()>::new(move || {
        let Some(file) = picked.files().and_then(|f| f.get(0)) else {
            return;
        };
        let Some(done) = done.take() else { return };
        let name = file.name();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(text) = wasm_bindgen_futures::JsFuture::from(file.text()).await {
                let stem = name
                    .rsplit_once('.')
                    .map_or(name.as_str(), |(s, _)| s)
                    .to_string();
                done(Opened {
                    name: stem,
                    source: text.as_string().unwrap_or_default(),
                });
            }
        });
    });
    input.set_onchange(Some(onchange.as_ref().unchecked_ref()));
    onchange.forget();
    input.click();
}

/// Downloads `bytes` as `filename`.
pub fn save_file(filename: &str, mime: &str, bytes: Vec<u8>) -> Result<(), String> {
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let parts = js_sys::Array::of1(&array);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("{e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| format!("{e:?}"))?;
    let a: web_sys::HtmlAnchorElement = window()
        .document()
        .unwrap()
        .create_element("a")
        .unwrap()
        .unchecked_into();
    a.set_href(&url);
    a.set_download(filename);
    a.click();
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    let _ = window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 1000);
    Ok(())
}

/// Copies `text`; the browser may still refuse, after this returns.
pub fn copy_text(text: &str) -> Result<(), String> {
    let _ = window().navigator().clipboard().write_text(text);
    Ok(())
}

pub fn share_url(hash: &str) -> String {
    let location = window().location();
    format!(
        "{}{}{hash}",
        location.origin().unwrap_or_default(),
        location.pathname().unwrap_or_default()
    )
}

pub const COPIES_LINKS: bool = true;

pub fn use_viewport() -> Signal<(f64, f64)> {
    let size = || {
        let w = window();
        let get =
            |v: Result<wasm_bindgen::JsValue, _>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        (get(w.inner_width()), get(w.inner_height()))
    };
    let mut viewport = use_signal(size);
    use_hook(move || {
        let onresize = Closure::<dyn FnMut()>::new(move || viewport.set(size()));
        let _ =
            window().add_event_listener_with_callback("resize", onresize.as_ref().unchecked_ref());
        onresize.forget();
    });
    viewport
}

pub fn use_title(title: String) {
    use_effect(use_reactive!(|title| {
        if let Some(d) = window().document() {
            d.set_title(&title);
        }
    }));
}
