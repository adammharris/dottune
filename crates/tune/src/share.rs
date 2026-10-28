//! Share links: a tune, deflated and base64url-encoded, in the URL's hash.
//!
//!   https://…/#n=Ode%20to%20Joy&t=<deflate-raw, base64url>
//!
//! The same encoding the TypeScript player wrote with a browser's
//! `CompressionStream("deflate-raw")`, so its links still open.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Where the editor is published: what a link from a window opens.
pub const SITE: &str = "https://adammharris.github.io/dottune/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shared {
    pub name: Option<String>,
    pub source: String,
}

/// The hash (with its `#`) that shares `source` under `name`.
pub fn share_hash(source: &str, name: &str) -> String {
    let deflated = miniz_oxide::deflate::compress_to_vec(source.as_bytes(), 9);
    format!(
        "#n={}&t={}",
        encode_component(name),
        URL_SAFE_NO_PAD.encode(deflated)
    )
}

/// The tune a hash shares, or `None` if it shares none; an error if it is damaged.
pub fn read_share_hash(hash: &str) -> Result<Option<Shared>, String> {
    let mut name = None;
    let mut text = None;
    for pair in hash.trim_start_matches('#').split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "n" => name = Some(decode_component(v)),
            "t" => text = Some(v.to_string()),
            _ => {}
        }
    }
    let Some(t) = text else { return Ok(None) };
    // A browser's base64 may carry padding, and `+`/`/` from an older link.
    let t: String = t.trim_end_matches('=').replace('+', "-").replace('/', "_");
    let bytes = URL_SAFE_NO_PAD.decode(t).map_err(|e| e.to_string())?;
    let inflated = miniz_oxide::inflate::decompress_to_vec(&bytes).map_err(|e| format!("{e:?}"))?;
    let source = String::from_utf8(inflated).map_err(|e| e.to_string())?;
    Ok(Some(Shared { name, source }))
}

/// `application/x-www-form-urlencoded`, as `URLSearchParams` writes it.
fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn decode_component(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                match std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    Some(b) => {
                        out.push(b);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tune_survives_a_round_trip() {
        let source = "key C major\nRH: 1 2 3 - |\n// ünïcode\n";
        let hash = share_hash(source, "Ode to Joy & more");
        let shared = read_share_hash(&hash).unwrap().unwrap();
        assert_eq!(shared.source, source);
        assert_eq!(shared.name.as_deref(), Some("Ode to Joy & more"));
    }

    #[test]
    fn a_hash_without_a_tune_shares_none() {
        assert_eq!(read_share_hash("#foo=bar").unwrap(), None);
        assert_eq!(read_share_hash("").unwrap(), None);
    }

    #[test]
    fn a_damaged_link_is_an_error() {
        assert!(read_share_hash("#t=!!!").is_err());
    }
}
