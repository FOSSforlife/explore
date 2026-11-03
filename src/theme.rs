//! Colours. Providers describe *what* things are via [`Kind`]; this module
//! decides how they look.

use ratatui::style::Style;

use crate::provider::Kind;

pub fn kind(kind: Kind) -> Style {
    match kind {
        Kind::Container => Style::new().blue().bold(),
        Kind::Text => Style::new().green(),
        Kind::Number => Style::new().magenta(),
        Kind::Boolean => Style::new().yellow(),
        Kind::Null => Style::new().dark_gray(),
        Kind::Other => Style::new(),
    }
}

pub fn selection() -> Style {
    Style::new().reversed()
}

pub fn path() -> Style {
    Style::new().green().bold()
}

pub fn dim() -> Style {
    Style::new().dark_gray()
}

pub fn error() -> Style {
    Style::new().red()
}
