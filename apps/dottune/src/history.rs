//! One undo history for typing and GUI edits alike. Typing is recorded in
//! bursts: an entry per pause, caret move, or GUI edit, not per keystroke.

const LIMIT: usize = 200;
/// A pause this long ends a burst of typing.
const BURST_SEC: f64 = 1.0;

#[derive(Default)]
pub struct History {
    undo: Vec<String>,
    redo: Vec<String>,
    /// The source as of the last change, so typing can record what it replaced.
    last: String,
    /// When the last burst of typing last typed, while it goes on.
    typing: Option<f64>,
}

impl History {
    /// Starts afresh from text put in the source directly.
    pub fn reset(&mut self, text: &str) {
        self.last = text.to_string();
        self.typing = None;
    }

    /// Ends the current burst of typing, so the next starts an entry of its own.
    pub fn stop_typing(&mut self) {
        self.typing = None;
    }

    /// Records typing that made the source `now_text`, at `now` seconds.
    pub fn typed(&mut self, now_text: &str, now: f64) {
        if self.typing.is_none_or(|t| now - t > BURST_SEC) {
            let last = std::mem::take(&mut self.last);
            self.push(last);
        }
        self.typing = Some(now);
        self.redo.clear();
        self.last = now_text.to_string();
    }

    /// Records an edit from `current` to `next`; false if it changed nothing.
    pub fn commit(&mut self, current: &str, next: &str) -> bool {
        if next == current {
            return false;
        }
        self.typing = None;
        self.push(current.to_string());
        self.redo.clear();
        self.last = next.to_string();
        true
    }

    /// The text to go back to from `current`, if there is one.
    pub fn undo(&mut self, current: &str) -> Option<String> {
        self.typing = None;
        let prev = self.undo.pop()?;
        self.redo.push(current.to_string());
        self.last = prev.clone();
        Some(prev)
    }

    pub fn redo(&mut self, current: &str) -> Option<String> {
        self.typing = None;
        let next = self.redo.pop()?;
        self.push(current.to_string());
        self.last = next.clone();
        Some(next)
    }

    fn push(&mut self, text: String) {
        self.undo.push(text);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_is_undone_a_burst_at_a_time() {
        let mut h = History::default();
        h.reset("a");
        h.typed("ab", 0.0);
        h.typed("abc", 0.5);
        h.typed("abcd", 3.0);
        assert_eq!(h.undo("abcd").as_deref(), Some("abc"));
        assert_eq!(h.undo("abc").as_deref(), Some("a"));
        assert_eq!(h.undo("a"), None);
        assert_eq!(h.redo("a").as_deref(), Some("abc"));
    }

    #[test]
    fn an_edit_ends_a_burst_and_clears_redo() {
        let mut h = History::default();
        h.reset("a");
        h.typed("ab", 0.0);
        assert!(h.commit("ab", "ab!"));
        assert!(!h.commit("ab!", "ab!"));
        assert_eq!(h.undo("ab!").as_deref(), Some("ab"));
        h.commit("ab", "x");
        assert_eq!(h.redo("x"), None);
    }
}
