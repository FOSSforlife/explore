//! Rendering. Pure function of [`App`] state.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{App, Column, Message, Mode, PreviewPane},
    provider::{Entry, Preview},
    theme,
};

pub fn draw(f: &mut Frame, app: &App) {
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let [parent, current, preview] = Layout::horizontal([
        Constraint::Ratio(1, 8),
        Constraint::Ratio(3, 8),
        Constraint::Ratio(4, 8),
    ])
    .spacing(1)
    .areas(body);

    draw_title(f, top, app);

    if let Some(col) = app.parent() {
        draw_column(f, parent, col);
    }
    draw_list(f, current, app.entries(), app.cursor());
    draw_preview(f, preview, app.preview());

    draw_status(f, bottom, app);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let line = Line::from(vec![
        Span::styled(format!("{} ", app.title()), theme::dim()),
        Span::styled(app.display_cwd(), theme::path()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_preview(f: &mut Frame, area: Rect, pane: &PreviewPane) {
    match pane {
        PreviewPane::Nothing | PreviewPane::Content(Preview::Empty) => {}
        PreviewPane::Children(col) => draw_column(f, area, col),
        PreviewPane::Content(Preview::Text { text, kind }) => f.render_widget(
            Paragraph::new(text.as_str())
                .style(theme::kind(*kind))
                .wrap(Wrap { trim: false }),
            area,
        ),
        PreviewPane::Error(e) => f.render_widget(
            Paragraph::new(e.as_str())
                .style(theme::error())
                .wrap(Wrap { trim: false }),
            area,
        ),
    }
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    if let Mode::Prompt(input) = app.mode() {
        f.render_widget(Paragraph::new(format!(":{input}")), area);
        f.set_cursor_position((area.x + 1 + input.chars().count() as u16, area.y));
        return;
    }

    let left = match (app.selected(), app.cursor()) {
        (Some(e), Some(i)) => format!(" {}/{}  {}", i + 1, app.entries().len(), e.type_name),
        _ => " empty".into(),
    };
    let (right, right_style) = match app.message() {
        Some(Message::Error(m)) => (m.clone(), theme::error()),
        Some(Message::Info(m)) => (m.clone(), theme::dim()),
        None => {
            let pending = app.pending_keys();
            let sep = if pending.is_empty() { "" } else { "  " };
            (format!("{pending}{sep}{}", app.provider_name()), theme::dim())
        }
    };

    let [l, r] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(right.chars().count() as u16 + 1),
    ])
    .areas(area);
    f.render_widget(Paragraph::new(left).style(theme::dim()), l);
    f.render_widget(Paragraph::new(format!("{right} ")).style(right_style), r);
}

fn draw_column(f: &mut Frame, area: Rect, col: &Column) {
    draw_list(f, area, &col.entries, col.selected);
}

fn draw_list(f: &mut Frame, area: Rect, entries: &[Entry], selected: Option<usize>) {
    let width = area.width as usize;
    let rows: Vec<ListItem> = entries
        .iter()
        .map(|e| {
            let used = 1 + e.label.chars().count() + e.info.chars().count() + 1;
            let pad = width.saturating_sub(used);
            let line = Line::from(vec![
                Span::raw(" "),
                Span::raw(e.label.as_str()),
                Span::raw(" ".repeat(pad)),
                Span::raw(e.info.as_str()),
                Span::raw(" "),
            ])
            .style(theme::kind(e.kind));
            ListItem::new(line)
        })
        .collect();

    let list = List::new(rows).highlight_style(theme::selection());
    let mut state = ListState::default().with_selected(selected);
    f.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{keymap::Keymap, provider::json::JsonProvider};
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;

    #[test]
    fn renders_three_columns() {
        let data = json!({"users": [{"name": "ada"}], "count": 1});
        let mut app =
            App::new(Box::new(JsonProvider::from_value(data)), Keymap::defaults()).unwrap();
        app.run_line("enter");

        let mut term = Terminal::new(TestBackend::new(64, 6)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let screen: String = term
            .backend()
            .buffer()
            .content()
            .chunks(64)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>() + "\n")
            .collect();

        assert!(screen.contains("$.users"), "{screen}");
        assert!(screen.contains("users"), "parent column: {screen}");
        assert!(screen.contains("[0]"), "current column: {screen}");
        assert!(screen.contains("name"), "preview column: {screen}");
        assert!(screen.contains("1/1  object"), "status: {screen}");
    }
}
