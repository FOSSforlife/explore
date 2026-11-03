//! Key sequences → commands.
//!
//! Bindings are written as vim-style strings (`"gg"`, `"<C-c>"`,
//! `"<pagedown>"`), so a config file can later feed [`Keymap::bind`]
//! directly.

use std::fmt;

use anyhow::{Result, bail};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::command::Command;

/// One key press, normalised for matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    code: KeyCode,
    mods: KeyModifiers,
}

impl Chord {
    fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        // Shift is already reflected in the character itself ('G' vs 'g').
        let mods = match code {
            KeyCode::Char(_) => mods - KeyModifiers::SHIFT,
            _ => mods,
        };
        Self { code, mods }
    }
}

impl From<KeyEvent> for Chord {
    fn from(k: KeyEvent) -> Self {
        Self::new(k.code, k.modifiers)
    }
}

const NAMED_KEYS: &[(&str, KeyCode)] = &[
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("enter", KeyCode::Enter),
    ("cr", KeyCode::Enter),
    ("esc", KeyCode::Esc),
    ("tab", KeyCode::Tab),
    ("bs", KeyCode::Backspace),
    ("space", KeyCode::Char(' ')),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
    ("del", KeyCode::Delete),
    ("lt", KeyCode::Char('<')),
];

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let named = NAMED_KEYS.iter().find(|(_, c)| *c == self.code).map(|(n, _)| *n);
        let mut prefix = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            prefix.push_str("C-");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            prefix.push_str("A-");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            prefix.push_str("S-");
        }
        match (self.code, named) {
            (KeyCode::Char(c), None) if prefix.is_empty() => write!(f, "{c}"),
            (KeyCode::Char(c), None) => write!(f, "<{prefix}{c}>"),
            (_, Some(n)) => write!(f, "<{prefix}{n}>"),
            (code, None) => write!(f, "<{prefix}{code:?}>"),
        }
    }
}

/// Parse a key sequence such as `"gg"` or `"<C-d>"`.
pub fn parse_keys(spec: &str) -> Result<Vec<Chord>> {
    let mut out = Vec::new();
    let mut rest = spec;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
            && end > 1
        {
            out.push(parse_special(&rest[1..end])?);
            rest = &rest[end + 1..];
        } else {
            out.push(Chord::new(KeyCode::Char(c), KeyModifiers::NONE));
            rest = &rest[c.len_utf8()..];
        }
    }
    if out.is_empty() {
        bail!("empty key sequence");
    }
    Ok(out)
}

fn parse_special(inner: &str) -> Result<Chord> {
    let mut mods = KeyModifiers::NONE;
    let mut name = inner;
    loop {
        let m = match name.get(..2) {
            Some("C-" | "c-") => KeyModifiers::CONTROL,
            Some("A-" | "a-" | "M-" | "m-") => KeyModifiers::ALT,
            Some("S-" | "s-") => KeyModifiers::SHIFT,
            _ => break,
        };
        mods |= m;
        name = &name[2..];
    }
    let code = if let Some(&(_, code)) = NAMED_KEYS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        code
    } else {
        let mut chars = name.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => KeyCode::Char(c),
            _ => bail!("unknown key `<{inner}>`"),
        }
    };
    Ok(Chord::new(code, mods))
}

pub enum Resolution {
    Command(Command),
    /// The keys so far are a prefix of some binding.
    Pending,
    Unbound,
}

#[derive(Default)]
pub struct Keymap {
    bindings: Vec<(Vec<Chord>, Command)>,
    pending: Vec<Chord>,
}

impl Keymap {
    /// Bind `keys` to `cmd`, replacing any existing binding for that sequence.
    ///
    /// Note: an exact match fires immediately, so binding both `g` and `gg`
    /// makes `gg` unreachable.
    pub fn bind(&mut self, keys: &str, cmd: Command) -> Result<()> {
        let seq = parse_keys(keys)?;
        self.bindings.retain(|(s, _)| *s != seq);
        self.bindings.push((seq, cmd));
        Ok(())
    }

    /// Feed one key press and see whether it completes a binding.
    pub fn feed(&mut self, key: KeyEvent) -> Resolution {
        self.feed_chord(key.into())
    }

    fn feed_chord(&mut self, chord: Chord) -> Resolution {
        self.pending.push(chord);

        if let Some((_, cmd)) = self.bindings.iter().find(|(s, _)| *s == self.pending) {
            let cmd = cmd.clone();
            self.pending.clear();
            return Resolution::Command(cmd);
        }
        if self.bindings.iter().any(|(s, _)| s.starts_with(&self.pending)) {
            return Resolution::Pending;
        }

        // A broken sequence: start over from the latest key on its own.
        let retry = self.pending.len() > 1;
        self.pending.clear();
        if retry {
            self.feed_chord(chord)
        } else {
            Resolution::Unbound
        }
    }

    /// Keys typed so far of an incomplete sequence, for display.
    pub fn pending(&self) -> String {
        self.pending.iter().map(Chord::to_string).collect()
    }

    pub fn reset(&mut self) {
        self.pending.clear();
    }

    /// Ranger/vim-like defaults.
    pub fn defaults() -> Self {
        use Command::*;
        let mut km = Self::default();
        let defaults = [
            ("q", Quit),
            ("<C-c>", Quit),
            ("j", Move(1)),
            ("<down>", Move(1)),
            ("k", Move(-1)),
            ("<up>", Move(-1)),
            ("<pagedown>", Move(20)),
            ("<pageup>", Move(-20)),
            ("<C-d>", Move(20)),
            ("<C-u>", Move(-20)),
            ("gg", Top),
            ("<home>", Top),
            ("G", Bottom),
            ("<end>", Bottom),
            ("l", Enter),
            ("<right>", Enter),
            ("<enter>", Enter),
            ("h", Leave),
            ("<left>", Leave),
            ("R", Reload),
            (":", Prompt),
        ];
        for (keys, cmd) in defaults {
            km.bind(keys, cmd).expect("default bindings are valid");
        }
        km
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(km: &mut Keymap, spec: &str) -> Vec<Option<Command>> {
        parse_keys(spec)
            .unwrap()
            .into_iter()
            .map(|c| match km.feed_chord(c) {
                Resolution::Command(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parses_specs() {
        let keys = parse_keys("g<C-d><pagedown><lt>").unwrap();
        assert_eq!(keys.len(), 4);
        assert_eq!(keys[1], Chord::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(keys[3], Chord::new(KeyCode::Char('<'), KeyModifiers::NONE));
        assert_eq!(parse_keys("<").unwrap().len(), 1, "lone < is literal");
        assert!(parse_keys("<nope>").is_err());
    }

    #[test]
    fn resolves_sequences() {
        let mut km = Keymap::defaults();
        assert_eq!(press(&mut km, "gg"), [None, Some(Command::Top)]);
        assert_eq!(km.pending(), "");
        // Broken sequence retries the last key alone.
        assert_eq!(press(&mut km, "gj"), [None, Some(Command::Move(1))]);
        assert_eq!(press(&mut km, "G"), [Some(Command::Bottom)]);
    }

    #[test]
    fn shift_is_normalised_for_chars() {
        let mut km = Keymap::defaults();
        let ev = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert!(matches!(km.feed(ev), Resolution::Command(Command::Bottom)));
    }
}
