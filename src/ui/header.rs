use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{app::App, environment::Daylight, model};

pub(super) fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let colors = app.settings.theme.palette();
    let count = format!("[{} plants]", app.plants.len());
    let view = if app.detail { "SPECIMEN" } else { "GREENHOUSE" };
    let flags = format!(
        "{}{}",
        if app.due_only { " · due" } else { "" },
        if app.query.is_empty() {
            ""
        } else {
            " · filtered"
        }
    );
    let middle = [
        format!(
            "{view} · sorted by {}{}{}",
            app.grouping.label(),
            if app.due_only { " · due" } else { "" },
            if app.query.is_empty() {
                String::new()
            } else {
                format!(" · {}", app.query)
            }
        ),
        format!("{view} · sorted by {}{flags}", app.grouping.label()),
        format!("{view} · {}{flags}", app.grouping.label()),
        format!("{view}{flags}"),
        format!(
            "{}{}",
            if app.detail { "SP" } else { "GH" },
            if flags.is_empty() { "" } else { "*" }
        ),
    ];
    let left = |brand: &str, tagline: bool| {
        Line::from(vec![
            Span::styled(
                brand.to_string(),
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if tagline {
                    " / your indoor garden "
                } else {
                    " "
                },
                Style::default().fg(colors.muted),
            ),
            Span::styled(count.clone(), Style::default().fg(colors.count)),
        ])
    };
    let lefts = [
        left("plantmon", true),
        left("plantmon", false),
        left("pm", false),
    ];
    let date = model::date(app.now);
    let clocks = [date.clone(), date.get(11..).unwrap_or(&date).to_string()];
    let mut chosen = None;
    // Keep the complete brand until the terminal genuinely needs the tiny layout.
    'search: for left_range in [0..2, 2..3] {
        for clock in &clocks {
            let right_width = Line::raw(format!("{} {clock}", app.daylight.icon())).width();
            for text in &middle {
                let width = Line::raw(text.as_str()).width();
                if width > area.width as usize {
                    continue;
                }
                let x = (area.width as usize - width) / 2;
                if x + width + 1 + right_width > area.width as usize {
                    continue;
                }
                for index in left_range.clone() {
                    if lefts[index].width() < x {
                        chosen = Some((
                            index,
                            text.as_str(),
                            x as u16,
                            width as u16,
                            clock.as_str(),
                            right_width as u16,
                        ));
                        break 'search;
                    }
                }
            }
        }
    }
    let Some((index, text, x, width, clock, right_width)) = chosen else {
        return;
    };
    frame.render_widget(
        Paragraph::new(lefts[index].clone()),
        Rect::new(area.x, area.y, x.saturating_sub(1), 1),
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(colors.muted))
            .alignment(Alignment::Center),
        Rect::new(area.x + x, area.y, width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", app.daylight.icon()),
                Style::default().fg(if app.daylight == Daylight::Day {
                    colors.warning
                } else {
                    colors.water
                }),
            ),
            Span::styled(clock, Style::default().fg(colors.muted)),
        ]))
        .alignment(Alignment::Right),
        Rect::new(
            area.right().saturating_sub(right_width),
            area.y,
            right_width,
            1,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::Database, settings::Theme};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn header_is_one_line_centered_and_right_aligned_without_collisions() -> anyhow::Result<()> {
        let mut app = App::new(Database::memory()?)?;
        for theme in Theme::ALL {
            app.settings.theme = theme;
            for width in [38, 50, 60, 80, 120, 160] {
                let mut terminal = Terminal::new(TestBackend::new(width, 1))?;
                terminal.draw(|frame| draw(frame, &app, frame.area()))?;
                let buffer = terminal.backend().buffer();
                let row: String = (0..width).map(|x| buffer[(x, 0)].symbol()).collect();
                assert!(row.contains("[0 plants]"), "{width}: {row}");
                assert!(row.contains(app.daylight.icon()));
                assert!(!row.contains("care reminders"));
                let clock = model::date(app.now);
                assert!(row.ends_with(&clock) || row.ends_with(&clock[11..]));
                let count_x = row.find("[0 plants]").unwrap() as u16;
                assert_eq!(buffer[(count_x, 0)].fg, theme.palette().count);
                if width >= 80 {
                    let label = "GREENHOUSE · sorted by name";
                    let start = (width as usize - Line::raw(label).width()) / 2;
                    let actual: String = (start as u16
                        ..start as u16 + Line::raw(label).width() as u16)
                        .map(|x| buffer[(x, 0)].symbol())
                        .collect();
                    assert_eq!(actual, label);
                }
                assert_eq!(row.contains("your indoor garden"), width >= 120);
            }
        }
        Ok(())
    }
}
