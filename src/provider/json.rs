//! Browse JSON documents.

use std::{
    borrow::Cow,
    fmt::Write as _,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use super::{Entry, Key, Kind, Preview, Provider, ProviderFactory};

pub struct JsonFactory;

impl ProviderFactory for JsonFactory {
    fn name(&self) -> &'static str {
        "json"
    }

    fn description(&self) -> &'static str {
        "JSON documents (a file path, or `-` for stdin)"
    }

    fn detect(&self, source: &str) -> bool {
        source == "-"
            || Path::new(source)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json") || e.eq_ignore_ascii_case("geojson"))
    }

    fn open(&self, source: &str) -> Result<Box<dyn Provider>> {
        Ok(Box::new(JsonProvider::open(source)?))
    }
}

enum Origin {
    File(PathBuf),
    Stdin,
    Memory,
}

pub struct JsonProvider {
    root: Value,
    origin: Origin,
}

impl JsonProvider {
    pub fn open(source: &str) -> Result<Self> {
        let origin = if source == "-" {
            Origin::Stdin
        } else {
            Origin::File(source.into())
        };
        let root = load(&origin)?;
        Ok(Self { root, origin })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_value(root: Value) -> Self {
        Self {
            root,
            origin: Origin::Memory,
        }
    }

    fn resolve(&self, path: &[Key]) -> Result<&Value> {
        path.iter().try_fold(&self.root, |v, key| {
            match (v, key) {
                (Value::Object(m), Key::Name(k)) => m.get(k),
                (Value::Array(a), Key::Index(i)) => a.get(*i),
                _ => None,
            }
            .ok_or_else(|| anyhow!("no such node: {}", self.display_path(path)))
        })
    }
}

fn load(origin: &Origin) -> Result<Value> {
    let text = match origin {
        Origin::File(p) => {
            fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?
        }
        Origin::Stdin => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s).context("reading stdin")?;
            s
        }
        Origin::Memory => bail!("nothing to load"),
    };
    serde_json::from_str(&text).context("parsing JSON")
}

fn entry(key: Key, label: String, v: &Value) -> Entry {
    let (kind, type_name, info, expandable) = match v {
        Value::Object(m) => (Kind::Container, "object", m.len().to_string(), !m.is_empty()),
        Value::Array(a) => (Kind::Container, "array", a.len().to_string(), !a.is_empty()),
        Value::String(_) => (Kind::Text, "string", String::new(), false),
        Value::Number(_) => (Kind::Number, "number", String::new(), false),
        Value::Bool(_) => (Kind::Boolean, "bool", String::new(), false),
        Value::Null => (Kind::Null, "null", String::new(), false),
    };
    Entry {
        key,
        label,
        info,
        kind,
        type_name: Cow::Borrowed(type_name),
        expandable,
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

impl Provider for JsonProvider {
    fn name(&self) -> &str {
        "json"
    }

    fn title(&self) -> String {
        match &self.origin {
            Origin::File(p) => p.display().to_string(),
            Origin::Stdin => "<stdin>".into(),
            Origin::Memory => "<memory>".into(),
        }
    }

    fn children(&mut self, path: &[Key]) -> Result<Vec<Entry>> {
        Ok(match self.resolve(path)? {
            Value::Object(m) => m
                .iter()
                .map(|(k, v)| entry(Key::Name(k.clone()), k.clone(), v))
                .collect(),
            Value::Array(a) => a
                .iter()
                .enumerate()
                .map(|(i, v)| entry(Key::Index(i), format!("[{i}]"), v))
                .collect(),
            _ => Vec::new(),
        })
    }

    fn preview(&mut self, path: &[Key]) -> Result<Preview> {
        let v = self.resolve(path)?;
        let text = match v {
            Value::String(s) => s.clone(),
            other => serde_json::to_string_pretty(other)?,
        };
        Ok(Preview::Text {
            text,
            kind: entry(Key::Index(0), String::new(), v).kind,
        })
    }

    /// JSONPath-style: `$.store.books[0]["weird key"]`.
    fn display_path(&self, path: &[Key]) -> String {
        let mut s = String::from("$");
        for key in path {
            let _ = match key {
                Key::Index(i) => write!(s, "[{i}]"),
                Key::Name(n) if is_identifier(n) => write!(s, ".{n}"),
                Key::Name(n) => write!(s, "[{}]", Value::String(n.clone())),
            };
        }
        s
    }

    fn reload(&mut self) -> Result<()> {
        match self.origin {
            Origin::Stdin => bail!("cannot reload stdin"),
            Origin::Memory => Ok(()),
            Origin::File(_) => {
                self.root = load(&self.origin)?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider() -> JsonProvider {
        JsonProvider::from_value(json!({
            "b": [1, "two", null],
            "a": {"x y": true},
            "empty": {}
        }))
    }

    #[test]
    fn lists_children_in_document_order() {
        let mut p = provider();
        let root = p.children(&[]).unwrap();
        let labels: Vec<_> = root.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["b", "a", "empty"]);
        assert_eq!(root[0].info, "3");
        assert!(root[0].expandable);
        assert!(!root[2].expandable, "empty containers are not enterable");
    }

    #[test]
    fn resolves_nested_paths() {
        let mut p = provider();
        let items = p.children(&[Key::Name("b".into())]).unwrap();
        assert_eq!(items[1].kind, Kind::Text);
        let preview = p.preview(&[Key::Name("b".into()), Key::Index(1)]).unwrap();
        assert_eq!(
            preview,
            Preview::Text {
                text: "two".into(),
                kind: Kind::Text
            }
        );
        assert!(p.children(&[Key::Name("nope".into())]).is_err());
    }

    #[test]
    fn displays_jsonpath() {
        let p = provider();
        let path = [Key::Name("a".into()), Key::Name("x y".into()), Key::Index(2)];
        assert_eq!(p.display_path(&path), r#"$.a["x y"][2]"#);
    }
}
