//! Commands: the single vocabulary for everything the user can do.
//!
//! Key bindings, the `:` prompt, and `--command` all resolve to [`Command`]
//! values and go through [`crate::app::App::execute`]. A future scripting
//! layer should produce these too, rather than poking at app state directly.

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    /// Move the cursor by a relative amount (negative = up).
    Move(isize),
    Top,
    Bottom,
    /// Descend into the selected entry.
    Enter,
    /// Go up to the parent node.
    Leave,
    /// Ask the provider to re-read its data.
    Reload,
    /// Open the `:` command line.
    Prompt,
}

impl Command {
    /// Parse a command line. Multiple commands may be separated by `;`.
    pub fn parse_line(line: &str) -> Result<Vec<Command>> {
        line.split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(Command::parse)
            .collect()
    }

    /// Parse a single command, e.g. `down 5`.
    pub fn parse(input: &str) -> Result<Command> {
        let mut words = input.split_whitespace();
        let Some(name) = words.next() else {
            bail!("empty command");
        };
        let arg = words.next();
        if let Some(extra) = words.next() {
            bail!("`{name}`: unexpected argument `{extra}`");
        }

        let count = |default: isize| -> Result<isize> {
            arg.map_or(Ok(default), |a| {
                a.parse().with_context(|| format!("`{name}`: `{a}` is not a number"))
            })
        };
        let no_arg = |cmd: Command| -> Result<Command> {
            match arg {
                Some(a) => bail!("`{name}` takes no argument (got `{a}`)"),
                None => Ok(cmd),
            }
        };

        match name {
            "q" | "quit" => no_arg(Command::Quit),
            "down" => Ok(Command::Move(count(1)?)),
            "up" => Ok(Command::Move(-count(1)?)),
            "move" => match arg {
                Some(_) => Ok(Command::Move(count(0)?)),
                None => bail!("`move` needs an offset"),
            },
            "top" => no_arg(Command::Top),
            "bottom" => no_arg(Command::Bottom),
            "enter" => no_arg(Command::Enter),
            "leave" => no_arg(Command::Leave),
            "reload" => no_arg(Command::Reload),
            "prompt" => no_arg(Command::Prompt),
            _ => bail!("unknown command `{name}`"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(Command::parse("down").unwrap(), Command::Move(1));
        assert_eq!(Command::parse("up 3").unwrap(), Command::Move(-3));
        assert_eq!(Command::parse("move -2").unwrap(), Command::Move(-2));
        assert_eq!(
            Command::parse_line("top; enter;").unwrap(),
            [Command::Top, Command::Enter]
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Command::parse("frobnicate").is_err());
        assert!(Command::parse("down x").is_err());
        assert!(Command::parse("quit now").is_err());
        assert!(Command::parse("move").is_err());
    }
}
