//! Data-source plugins.
//!
//! A [`Provider`] exposes some hierarchical data as a tree of [`Entry`]s,
//! addressed by [`NodePath`]s. The browser knows nothing about the
//! underlying format: it only asks a provider to list a node's children and
//! to preview leaves. That is the whole contract.
//!
//! # Adding a new data source
//!
//! 1. Create a module (e.g. `provider/fs.rs`) with a type implementing
//!    [`Provider`] and a factory implementing [`ProviderFactory`].
//! 2. Add the factory to [`registry`].
//!
//! Providers map their own concepts onto the generic [`Kind`] so the theme
//! can style them consistently; add a `Kind` variant when a new concept
//! (symlink, executable, HTTP method, ...) deserves its own look.

pub mod json;

use std::borrow::Cow;

use anyhow::{Result, anyhow, bail};

use crate::convert::Format;

/// Identifies one child within its parent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Name(String),
    Index(usize),
}

/// Location of a node, as a sequence of keys from the root. The empty path is
/// the root.
pub type NodePath = Vec<Key>;

/// Provider-agnostic classification of a node, used for styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Container,
    Text,
    Number,
    Boolean,
    Null,
    /// Fallback for providers whose nodes have no better match.
    #[allow(dead_code)]
    Other,
}

/// A child of some node, as shown in a column.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: Key,
    /// What the column displays on the left.
    pub label: String,
    /// What the column displays on the right (ranger's size column).
    pub info: String,
    pub kind: Kind,
    /// Human-readable type, shown in the status bar.
    pub type_name: Cow<'static, str>,
    /// Whether the browser may descend into this entry.
    pub expandable: bool,
}

/// How a provider wants a leaf to be shown in the preview column.
#[derive(Debug, Clone, PartialEq)]
pub enum Preview {
    Empty,
    Text { text: String, kind: Kind },
}

/// A browsable tree of data.
///
/// Methods take `&mut self` so implementations are free to load lazily,
/// cache, or talk to the outside world.
pub trait Provider {
    /// Short name of the backend, e.g. `"json"`.
    fn name(&self) -> &str;

    /// Description of what is being browsed (file name, URL, ...).
    fn title(&self) -> String;

    /// List the children of the node at `path`.
    fn children(&mut self, path: &[Key]) -> Result<Vec<Entry>>;

    /// Preview the (non-expandable) node at `path`.
    fn preview(&mut self, _path: &[Key]) -> Result<Preview> {
        Ok(Preview::Empty)
    }

    /// Render a path in the provider's native notation.
    fn display_path(&self, path: &[Key]) -> String {
        let parts: Vec<String> = path
            .iter()
            .map(|k| match k {
                Key::Name(n) => n.clone(),
                Key::Index(i) => i.to_string(),
            })
            .collect();
        format!("/{}", parts.join("/"))
    }

    /// Re-read the underlying data. Paths may become invalid afterwards.
    fn reload(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Settings from the command line that affect how a source is read.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenOptions {
    /// Input format given with `--from`; `None` means auto-detect.
    pub from: Option<Format>,
}

/// Knows how to recognise and open one kind of source.
pub trait ProviderFactory {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;

    /// Cheap guess (e.g. by extension) whether this factory handles `source`.
    fn detect(&self, source: &str) -> bool;

    fn open(&self, source: &str, opts: OpenOptions) -> Result<Box<dyn Provider>>;
}

/// All known providers, in auto-detection priority order.
pub fn registry() -> Vec<Box<dyn ProviderFactory>> {
    vec![Box::new(json::JsonFactory)]
}

/// Open `source` with the named provider, or auto-detect one.
///
/// Detection first asks each factory's [`ProviderFactory::detect`]; if none
/// claims the source, every factory is tried in turn.
pub fn open(source: &str, provider: Option<&str>, opts: OpenOptions) -> Result<Box<dyn Provider>> {
    let factories = registry();

    if let Some(name) = provider {
        let factory = factories.iter().find(|f| f.name() == name).ok_or_else(|| {
            let names: Vec<_> = factories.iter().map(|f| f.name()).collect();
            anyhow!("unknown provider `{name}` (available: {})", names.join(", "))
        })?;
        return factory.open(source, opts);
    }

    if let Some(factory) = factories.iter().find(|f| f.detect(source)) {
        return factory.open(source, opts);
    }

    let mut errors = Vec::new();
    for factory in &factories {
        match factory.open(source, opts) {
            Ok(p) => return Ok(p),
            Err(e) => errors.push(format!("  {}: {e:#}", factory.name())),
        }
    }
    bail!("no provider could open `{source}`:\n{}", errors.join("\n"))
}
