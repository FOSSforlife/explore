//! Browser state and command execution. Has no knowledge of the terminal,
//! so it can be driven by tests or, later, scripts.

use std::collections::HashMap;

use anyhow::Result;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    command::Command,
    keymap::{Keymap, Resolution},
    provider::{Entry, Key, NodePath, Preview, Provider},
};

/// A list of entries, optionally with one highlighted.
#[derive(Debug, Default)]
pub struct Column {
    pub entries: Vec<Entry>,
    pub selected: Option<usize>,
}

/// What the right-hand column shows for the selected entry.
#[derive(Debug)]
pub enum PreviewPane {
    Nothing,
    Children(Column),
    Content(Preview),
    Error(String),
}

pub enum Mode {
    Normal,
    /// The `:` command line, with its current input.
    Prompt(String),
}

pub enum Message {
    Info(String),
    Error(String),
}

pub struct App {
    provider: Box<dyn Provider>,
    keymap: Keymap,

    cwd: NodePath,
    entries: Vec<Entry>,
    cursor: usize,
    parent: Option<Column>,
    preview: PreviewPane,
    /// Last selected child per visited node, so navigation feels stateful.
    memory: HashMap<NodePath, Key>,

    mode: Mode,
    message: Option<Message>,
    quit: bool,
}

impl App {
    pub fn new(provider: Box<dyn Provider>, keymap: Keymap) -> Result<Self> {
        let mut app = Self {
            provider,
            keymap,
            cwd: NodePath::new(),
            entries: Vec::new(),
            cursor: 0,
            parent: None,
            preview: PreviewPane::Nothing,
            memory: HashMap::new(),
            mode: Mode::Normal,
            message: None,
            quit: false,
        };
        app.load()?;
        Ok(app)
    }

    // ---------- accessors for the UI ----------

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn title(&self) -> String {
        self.provider.title()
    }

    pub fn display_cwd(&self) -> String {
        self.provider.display_path(&self.cwd)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn cursor(&self) -> Option<usize> {
        (!self.entries.is_empty()).then_some(self.cursor)
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    pub fn parent(&self) -> Option<&Column> {
        self.parent.as_ref()
    }

    pub fn preview(&self) -> &PreviewPane {
        &self.preview
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub fn pending_keys(&self) -> String {
        self.keymap.pending()
    }

    // ---------- input ----------

    pub fn handle_key(&mut self, key: KeyEvent) {
        match &mut self.mode {
            Mode::Normal => {
                self.message = None;
                if let Resolution::Command(cmd) = self.keymap.feed(key) {
                    self.run(cmd);
                }
            }
            Mode::Prompt(input) => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    KeyCode::Char('c') if ctrl => self.mode = Mode::Normal,
                    KeyCode::Enter => {
                        let line = std::mem::take(input);
                        self.mode = Mode::Normal;
                        self.run_line(&line);
                    }
                    KeyCode::Backspace if input.is_empty() => self.mode = Mode::Normal,
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char(c) if !ctrl => input.push(c),
                    _ => {}
                }
            }
        }
    }

    // ---------- commands ----------

    /// Execute a command, reporting failure in the status bar.
    pub fn run(&mut self, cmd: Command) {
        if let Err(e) = self.execute(cmd) {
            self.message = Some(Message::Error(format!("{e:#}")));
        }
    }

    /// Parse and run a command line; stops at the first failure.
    pub fn run_line(&mut self, line: &str) {
        let cmds = match Command::parse_line(line) {
            Ok(cmds) => cmds,
            Err(e) => {
                self.message = Some(Message::Error(format!("{e:#}")));
                return;
            }
        };
        for cmd in cmds {
            if let Err(e) = self.execute(cmd) {
                self.message = Some(Message::Error(format!("{e:#}")));
                return;
            }
        }
    }

    pub fn execute(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Quit => self.quit = true,
            Command::Move(delta) => {
                let target = self.cursor as isize + delta;
                self.set_cursor(target.max(0) as usize);
            }
            Command::Top => self.set_cursor(0),
            Command::Bottom => self.set_cursor(usize::MAX),
            Command::Enter => self.enter()?,
            Command::Leave => self.leave()?,
            Command::Reload => self.reload()?,
            Command::Prompt => {
                self.keymap.reset();
                self.mode = Mode::Prompt(String::new());
            }
        }
        Ok(())
    }

    // ---------- navigation ----------

    fn set_cursor(&mut self, index: usize) {
        if self.entries.is_empty() {
            return;
        }
        self.cursor = index.min(self.entries.len() - 1);
        self.memory
            .insert(self.cwd.clone(), self.entries[self.cursor].key.clone());
        self.refresh_preview();
    }

    fn enter(&mut self) -> Result<()> {
        let Some(entry) = self.selected().filter(|e| e.expandable) else {
            return Ok(());
        };
        let key = entry.key.clone();
        self.cwd.push(key);
        if let Err(e) = self.load() {
            self.cwd.pop();
            self.load()?;
            return Err(e);
        }
        Ok(())
    }

    fn leave(&mut self) -> Result<()> {
        if let Some(key) = self.cwd.pop() {
            self.memory.insert(self.cwd.clone(), key);
            self.load()?;
        }
        Ok(())
    }

    fn reload(&mut self) -> Result<()> {
        self.provider.reload()?;
        // The data may have changed shape; back out until the path exists.
        while let Err(e) = self.load() {
            if self.cwd.pop().is_none() {
                return Err(e);
            }
        }
        self.message = Some(Message::Info("reloaded".into()));
        Ok(())
    }

    // ---------- loading ----------

    fn position(entries: &[Entry], key: Option<&Key>) -> Option<usize> {
        key.and_then(|k| entries.iter().position(|e| &e.key == k))
    }

    /// (Re)load all columns for the current directory.
    fn load(&mut self) -> Result<()> {
        let entries = self.provider.children(&self.cwd)?;
        let parent = match self.cwd.split_last() {
            Some((key, rest)) => {
                let entries = self.provider.children(rest)?;
                let selected = Self::position(&entries, Some(key));
                Some(Column { entries, selected })
            }
            None => None,
        };

        self.cursor = Self::position(&entries, self.memory.get(&self.cwd)).unwrap_or(0);
        self.entries = entries;
        self.parent = parent;
        self.refresh_preview();
        Ok(())
    }

    fn refresh_preview(&mut self) {
        let Some(entry) = self.entries.get(self.cursor) else {
            self.preview = PreviewPane::Nothing;
            return;
        };
        let expandable = entry.expandable;
        let mut path = self.cwd.clone();
        path.push(entry.key.clone());

        let result = if expandable {
            self.provider.children(&path).map(|entries| {
                let selected = Self::position(&entries, self.memory.get(&path));
                PreviewPane::Children(Column { entries, selected })
            })
        } else {
            self.provider.preview(&path).map(PreviewPane::Content)
        };
        self.preview = result.unwrap_or_else(|e| PreviewPane::Error(format!("{e:#}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::json::JsonProvider;
    use serde_json::json;

    fn app() -> App {
        let data = json!({
            "a": {"x": 1, "y": [true, false]},
            "b": "hello",
            "c": []
        });
        App::new(Box::new(JsonProvider::from_value(data)), Keymap::defaults()).unwrap()
    }

    #[test]
    fn navigates_and_remembers_cursor() {
        let mut app = app();
        app.run_line("enter; down; enter");
        assert_eq!(app.display_cwd(), "$.a.y");
        app.run_line("down; leave; leave");
        assert_eq!(app.display_cwd(), "$");

        // Re-entering restores the previous selections.
        app.run_line("enter");
        assert_eq!(app.selected().unwrap().label, "y");
        app.run_line("enter");
        assert_eq!(app.selected().unwrap().label, "[1]");
    }

    #[test]
    fn previews_selection() {
        let mut app = app();
        assert!(matches!(app.preview(), PreviewPane::Children(c) if c.entries.len() == 2));
        app.run_line("down");
        assert!(matches!(app.preview(), PreviewPane::Content(Preview::Text { text, .. }) if text == "hello"));
    }

    #[test]
    fn clamps_and_ignores_leaves() {
        let mut app = app();
        app.run_line("up 10");
        assert_eq!(app.cursor(), Some(0));
        app.run_line("bottom; enter");
        assert_eq!(app.display_cwd(), "$", "empty array is not enterable");
        app.run_line("down 99");
        assert_eq!(app.cursor(), Some(2));
    }

    #[test]
    fn reports_bad_commands() {
        let mut app = app();
        app.run_line("down; bogus");
        assert!(matches!(app.message(), Some(Message::Error(_))));
        assert_eq!(app.cursor(), Some(0), "nothing runs if parsing fails");
    }
}
