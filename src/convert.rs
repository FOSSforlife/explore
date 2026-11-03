//! Turn non-JSON input into JSON with [jet](https://github.com/borkdude/jet).
//!
//! The app only understands JSON. Other formats are piped through the `jet`
//! binary on the user's `PATH`, so jet is only needed when a conversion is
//! actually about to run.

use std::{
    io::{ErrorKind, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;

/// Input formats, named like jet's `--from` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Json,
    Edn,
    Yaml,
    Transit,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Edn => "edn",
            Format::Yaml => "yaml",
            Format::Transit => "transit",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Format::Json => "JSON",
            Format::Edn => "EDN",
            Format::Yaml => "YAML",
            Format::Transit => "Transit",
        }
    }

    /// Guess the format from a file name, e.g. `data.yml` or `x.transit.json`.
    pub fn from_path(path: &Path) -> Option<Format> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        if name.ends_with(".transit.json") {
            return Some(Format::Transit);
        }
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "json" | "geojson" => Some(Format::Json),
            "edn" => Some(Format::Edn),
            "yaml" | "yml" => Some(Format::Yaml),
            "transit" => Some(Format::Transit),
            _ => None,
        }
    }
}

/// Unknown EDN tags (`#my/tag value`) would make jet fail to emit JSON, so
/// read them as `{"tag": "my/tag", "value": value}` instead.
const EDN_READER_OPTS: &str = r#"{:default (fn [tag value] {"tag" (str tag) "value" value})}"#;

/// Convert `text` from `format` to JSON text. JSON is returned unchanged.
///
/// jet emits one JSON document per top-level input value, so the result
/// may contain several whitespace-separated documents.
pub fn to_json(text: String, format: Format) -> Result<String> {
    if format == Format::Json {
        return Ok(text);
    }
    let label = format.label();

    let mut cmd = Command::new("jet");
    cmd.args(["--from", format.name(), "--to", "json"])
        .args(["--no-pretty", "--colors", "false"]);
    if format == Format::Edn {
        cmd.args(["--edn-reader-opts", EDN_READER_OPTS]);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => anyhow!(
                "reading {label} requires `jet` on your PATH, but it was not found \
                 (install it from https://github.com/borkdude/jet)"
            ),
            _ => anyhow!(e).context("starting `jet`"),
        })?;

    // Feed stdin from another thread so a large output can't deadlock us.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = thread::spawn(move || stdin.write_all(text.as_bytes()));
    let output = child.wait_with_output().context("running `jet`")?;
    // A write error just means jet stopped reading; its stderr says why.
    let _ = writer.join();

    let stderr = String::from_utf8_lossy(&output.stderr);
    // jet exits 0 even when it throws, so look at stderr too.
    if !output.status.success() || stderr.contains("Exception in thread") {
        bail!("jet could not convert {label}: {}", jet_error(&stderr));
    }
    let json = String::from_utf8(output.stdout).context("jet produced invalid UTF-8")?;
    if json.trim().is_empty() {
        bail!(
            "jet produced no output from the {label} input; it may be empty or malformed \
             (jet ignores some syntax errors silently)"
        );
    }
    Ok(json)
}

/// Reduce a JVM stack trace from jet to its message.
fn jet_error(stderr: &str) -> String {
    let message: Vec<&str> = stderr
        .lines()
        .take_while(|l| !l.starts_with('\t') && !l.starts_with("Caused by"))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut msg = message.join(" ");
    if let Some(rest) = msg.strip_prefix("Exception in thread \"main\" ") {
        msg = rest.to_string();
    }
    // Drop leading `java.lang.RuntimeException: `-style class names.
    while let Some((head, rest)) = msg.split_once(": ")
        && head.contains('.')
        && !head.contains(' ')
        && (head.ends_with("Exception") || head.ends_with("Error"))
    {
        msg = rest.to_string();
    }
    if msg.is_empty() {
        "unknown error".into()
    } else {
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jet_installed() -> bool {
        Command::new("jet").arg("--version").output().is_ok()
    }

    #[test]
    fn detects_formats_by_extension() {
        let f = |p: &str| Format::from_path(Path::new(p));
        assert_eq!(f("a.json"), Some(Format::Json));
        assert_eq!(f("a.EDN"), Some(Format::Edn));
        assert_eq!(f("a.yml"), Some(Format::Yaml));
        assert_eq!(f("a.yaml"), Some(Format::Yaml));
        assert_eq!(f("a.transit"), Some(Format::Transit));
        assert_eq!(f("a.transit.json"), Some(Format::Transit));
        assert_eq!(f("a.txt"), None);
        assert_eq!(f("-"), None);
    }

    #[test]
    fn json_passes_through_without_jet() {
        assert_eq!(to_json("[1]".into(), Format::Json).unwrap(), "[1]");
    }

    #[test]
    fn cleans_jet_stack_traces() {
        let trace = "Exception in thread \"main\" clojure.lang.EdnReader$ReaderException: \
                     java.lang.RuntimeException: EOF while reading, starting at line 1\n\
                     \tat clojure.lang.EdnReader.read(EdnReader.java:180)\n\
                     Caused by: java.lang.RuntimeException: EOF while reading\n";
        assert_eq!(jet_error(trace), "EOF while reading, starting at line 1");

        let yaml = "Exception in thread \"main\" expected a single document in the stream\n \
                    in 'reader', line 2, column 1:\n    a: 1\n    ^\n\n\
                    \tat org.yaml.snakeyaml.composer.Composer.getSingleNode(Composer.java:167)\n";
        assert_eq!(
            jet_error(yaml),
            "expected a single document in the stream in 'reader', line 2, column 1: a: 1 ^"
        );
    }

    #[test]
    fn converts_with_jet() {
        if !jet_installed() {
            eprintln!("skipping: jet not installed");
            return;
        }
        let json = to_json(r#"{:a #{1} :b #my/tag [1]}"#.into(), Format::Edn).unwrap();
        assert_eq!(json.trim(), r#"{"a":[1],"b":{"tag":"my/tag","value":[1]}}"#);
        let json = to_json("a: [1, 2]".into(), Format::Yaml).unwrap();
        assert_eq!(json.trim(), r#"{"a":[1,2]}"#);
        let err = to_json("{:a".into(), Format::Edn).unwrap_err();
        assert!(err.to_string().contains("EOF while reading"), "{err}");
        assert!(to_json("".into(), Format::Edn).is_err());
    }
}
