//! Edits through fig: the tune language's editor, so every change is a
//! minimal splice that fig reparses (and rolls back if the result does not
//! parse).

use std::fmt;

use fig::{Editor, Segment, Value};

use crate::types::TuneErrors;

/// One step of a path into a tune's tree (`figlang`'s doc says what the tree is).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Seg {
    Key(String),
    Index(usize),
}

impl From<&str> for Seg {
    fn from(k: &str) -> Self {
        Seg::Key(k.to_string())
    }
}

impl From<&String> for Seg {
    fn from(k: &String) -> Self {
        Seg::Key(k.clone())
    }
}

impl From<usize> for Seg {
    fn from(i: usize) -> Self {
        Seg::Index(i)
    }
}

pub type Path = Vec<Seg>;

/// A path from its steps: `path!["blocks", 0, "RH"]`.
#[macro_export]
macro_rules! path {
    ($($s:expr),* $(,)?) => { vec![$($crate::splice::Seg::from($s)),*] };
}

/// A token replaced: the text that goes at `path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenEdit {
    pub path: Path,
    pub text: String,
}

/// Why an edit could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditError(pub String);

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EditError {}

impl From<fig::Error> for EditError {
    fn from(e: fig::Error) -> Self {
        EditError(e.to_string())
    }
}

impl From<TuneErrors> for EditError {
    fn from(e: TuneErrors) -> Self {
        EditError(e.to_string())
    }
}

impl From<String> for EditError {
    fn from(s: String) -> Self {
        EditError(s)
    }
}

impl From<&str> for EditError {
    fn from(s: &str) -> Self {
        EditError(s.to_string())
    }
}

/// An open editor over a tune.
pub struct Ed(Editor);

fn segs(path: &[Seg]) -> Vec<Segment<'_>> {
    path.iter()
        .map(|s| match s {
            Seg::Key(k) => Segment::Key(k),
            Seg::Index(i) => Segment::Index(*i),
        })
        .collect()
}

impl Ed {
    /// Replaces the value at `path` with `text` as written: a token, a
    /// `[ ]` group, or a whole bar. The printer writes a string as its
    /// text, so nothing is quoted.
    pub fn replace(&mut self, path: &[Seg], text: &str) -> Result<(), EditError> {
        Ok(self
            .0
            .replace_value(&segs(path), Value::Str(text.to_string()))?)
    }

    /// Inserts `text`, as written, at `index` of the sequence at `path`.
    pub fn insert_item(&mut self, path: &[Seg], index: usize, text: &str) -> Result<(), EditError> {
        Ok(self
            .0
            .insert_item(&segs(path), index, Value::Str(text.to_string()))?)
    }

    pub fn delete_item(&mut self, path: &[Seg], index: usize) -> Result<(), EditError> {
        Ok(self.0.delete_item(&segs(path), index)?)
    }

    pub fn move_item(&mut self, path: &[Seg], from: usize, to: usize) -> Result<(), EditError> {
        Ok(self.0.move_item(&segs(path), from, to)?)
    }

    /// Sets the value at `path`, adding the entry if there is none.
    pub fn set(&mut self, path: &[Seg], value: Value) -> Result<(), EditError> {
        Ok(self.0.set_value(&segs(path), value)?)
    }

    pub fn delete_key(&mut self, path: &[Seg]) -> Result<(), EditError> {
        Ok(self.0.delete_key(&segs(path))?)
    }

    pub fn rename_key(&mut self, path: &[Seg], key: &str) -> Result<(), EditError> {
        Ok(self.0.rename_key(&segs(path), key)?)
    }
}

/// Runs `f` against an editor over `source`; returns the edited source.
/// Fails if any edit is refused.
pub fn apply(
    source: &str,
    f: impl FnOnce(&mut Ed) -> Result<(), EditError>,
) -> Result<String, EditError> {
    let mut ed = Ed(Editor::open(source.as_bytes(), crate::figlang::format())?);
    f(&mut ed)?;
    Ok(ed.0.source()?.to_string())
}

/// Replaces each edit's token.
pub fn apply_token_edits(source: &str, edits: &[TokenEdit]) -> Result<String, EditError> {
    if edits.is_empty() {
        return Ok(source.to_string());
    }
    apply(source, |ed| {
        for e in edits {
            ed.replace(&e.path, &e.text)?;
        }
        Ok(())
    })
}
