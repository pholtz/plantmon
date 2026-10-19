use crate::{
    app::{App, ConfirmAction, Form, HelpTab, Mode},
    art,
    model::{self, Plant},
    settings::{CardSpacing, Palette, Settings},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Cell, Clear, Paragraph, Row, Table, TableState, Tabs, Wrap},
};

mod header;

const FOOTER_HELP: &str = "  ? help  |  q quit";

pub fn draw(frame: &mut Frame, app: &mut App) {
    let colors = app.settings.theme.palette();
    let area = frame.area();
    let toast_inline = !app.toast.is_empty()
        && Line::raw(format!("{FOOTER_HELP}  ·  {}", app.toast)).width() <= area.width as usize;
    frame.render_widget(
        Block::default().style(Style::default().bg(colors.bg).fg(colors.text)),
        area,
    );
    let content = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let footer = Rect::new(
        area.x,
        area.bottom().saturating_sub(1),
        area.width,
        area.height.min(1),
    );
    if area.width < 38 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("plantmon · please resize to at least 38 × 12")
                .wrap(Wrap { trim: false }),
            content,
        );
    } else {
        let toast = format!("  {}", app.toast);
        let toast_height = if app.toast.is_empty() || toast_inline {
            0
        } else {
            Paragraph::new(toast.as_str())
                .wrap(Wrap { trim: false })
                .line_count(area.width)
                .min(2) as u16
        };
        let regions = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(toast_height),
        ])
        .split(content);
        header::draw(frame, app, regions[0]);
        if app.detail && app.selected_plant().is_some() {
            detail(frame, app, regions[1]);
        } else {
            grid(frame, app, regions[1]);
        }
        frame.render_widget(
            Paragraph::new(toast)
                .style(Style::default().fg(if app.error {
                    colors.error
                } else {
                    colors.accent
                }))
                .wrap(Wrap { trim: false }),
            regions[2],
        );
        let mode = match &app.mode {
            Mode::Help { previous, .. } => previous.as_ref(),
            mode => mode,
        };
        modal(frame, app, content, mode);
        if matches!(app.mode, Mode::Help { .. }) {
            let page = crate::help::page(&app.mode, app.detail, app.selected_plant().is_some());
            help(frame, content, page, app.settings, &mut app.mode);
        }
    }
    let mut footer_spans = vec![Span::styled(FOOTER_HELP, Style::default().fg(colors.muted))];
    if toast_inline {
        footer_spans.push(Span::styled("  ·  ", Style::default().fg(colors.muted)));
        footer_spans.push(Span::styled(
            app.toast.as_str(),
            Style::default().fg(if app.error {
                colors.error
            } else {
                colors.accent
            }),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(footer_spans)), footer);
}

fn modal(frame: &mut Frame, app: &App, area: Rect, mode: &Mode) {
    let colors = app.settings.theme.palette();
    match mode {
        Mode::Browse | Mode::Help { .. } => {}
        Mode::Confirm {
            action,
            prompt,
            confirm,
        } => {
            let inner = popup(frame, area, 76, 9, " Confirm action ", colors);
            let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
            frame.render_widget(
                Paragraph::new(prompt.as_str())
                    .style(Style::default().fg(colors.warning))
                    .wrap(Wrap { trim: false }),
                parts[0],
            );
            let action = match action {
                ConfirmAction::DeletePlant(_) => "Delete",
                ConfirmAction::UndoEvent { .. } => "Undo",
            };
            frame.render_widget(
                buttons(
                    "Cancel",
                    action,
                    if *confirm { Some(1) } else { Some(0) },
                    colors,
                ),
                parts[1],
            );
        }
        Mode::Search { value } => {
            let inner = popup(frame, area, 78, 8, " Find plants ", colors);
            frame.render_widget(
                Paragraph::new(format!(
                    "Search\n{value}▏\n\nName, family, care group, location, or tags."
                ))
                .wrap(Wrap { trim: false }),
                inner,
            );
        }
        Mode::Reorder {
            order,
            selected,
            before_move,
        } => reorder(frame, app, area, order, *selected, before_move.is_some()),
        Mode::Catalog { query, selected } => catalog(frame, app, area, query, *selected),
        Mode::Form(form) => form_view(frame, app, area, form),
    }
}

fn buttons<'a>(
    first: &'a str,
    second: &'a str,
    selected: Option<usize>,
    colors: Palette,
) -> Paragraph<'a> {
    Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" [ {first} ] "),
            Style::default()
                .fg(if selected == Some(0) {
                    colors.on_accent
                } else {
                    colors.muted
                })
                .bg(if selected == Some(0) {
                    colors.accent
                } else {
                    colors.panel
                }),
        ),
        Span::raw("  "),
        Span::styled(
            format!(" [ {second} ] "),
            Style::default()
                .fg(if selected == Some(1) {
                    colors.on_accent
                } else {
                    colors.muted
                })
                .bg(if selected == Some(1) {
                    colors.accent
                } else {
                    colors.panel
                }),
        ),
    ]))
}

fn form_view(frame: &mut Frame, app: &App, area: Rect, form: &Form) {
    let colors = app.settings.theme.palette();
    let inner = popup(
        frame,
        area,
        92,
        (form.fields.len() as u16 * 3 + 8).min(42),
        &format!(" {} ", form.title),
        colors,
    );
    let summary = if app.error {
        app.toast.as_str()
    } else {
        form.hint.as_str()
    };
    let parts = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(summary)
            .style(Style::default().fg(if app.error {
                colors.error
            } else {
                colors.muted
            }))
            .wrap(Wrap { trim: false }),
        parts[0],
    );
    let count = (parts[1].height as usize / 3).max(1);
    let start = form.focused.min(form.fields.len().saturating_sub(1)) / count * count;
    let mut lines = vec![];
    for (index, field) in form.fields.iter().enumerate().skip(start).take(count) {
        let focused = index == form.focused;
        lines.push(Line::from(Span::styled(
            format!("{} {}", if focused { "›" } else { " " }, field.label),
            Style::default().fg(if focused { colors.accent } else { colors.muted }),
        )));
        let value = if !field.choices.is_empty() {
            format!("  ‹ {} ›", field.value)
        } else if focused {
            let tail: String = field
                .value
                .chars()
                .rev()
                .take(inner.width.saturating_sub(6) as usize)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("  {tail}▏")
        } else {
            format!("  {}", field.value)
        };
        let mut style =
            Style::default()
                .fg(colors.text)
                .bg(if focused { colors.border } else { colors.panel });
        if focused && form.replace_on_type && !field.value.is_empty() && field.choices.is_empty() {
            style = style.add_modifier(Modifier::REVERSED);
        }
        lines.push(Line::from(Span::styled(value, style)));
        lines.push(Line::raw(""));
    }
    frame.render_widget(Paragraph::new(lines), parts[1]);
    let selected = form.focused.checked_sub(form.fields.len());
    frame.render_widget(buttons("Save", "Cancel", selected, colors), parts[2]);
    let position = if form.focused < form.fields.len() {
        format!("Field {} of {}", form.focused + 1, form.fields.len())
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(position).style(Style::default().fg(colors.muted)),
        parts[3],
    );
}

fn reorder(frame: &mut Frame, app: &App, area: Rect, order: &[i64], selected: usize, moving: bool) {
    let colors = app.settings.theme.palette();
    let inner = popup(frame, area, 96, 38, " Reorder greenhouse ", colors);
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(2),
        Constraint::Length(2),
    ])
    .split(inner);
    let summary = if moving {
        "Moving selected plant · use ↑/↓ to place it, then Enter/Space to drop and save."
    } else {
        "Choose a plant with ↑/↓, then press Enter/Space to pick it up."
    };
    frame.render_widget(
        Paragraph::new(summary)
            .style(Style::default().fg(if moving { colors.warning } else { colors.text }))
            .wrap(Wrap { trim: false }),
        parts[0],
    );
    let rows = order.iter().enumerate().filter_map(|(index, plant_id)| {
        let plant = app.plants.iter().find(|plant| plant.id == *plant_id)?;
        let location = if plant.location.is_empty() {
            "unplaced"
        } else {
            &plant.location
        };
        Some(
            Row::new([
                format!("{}", index + 1),
                plant.name().to_string(),
                format!("{}\n{}", plant.taxon.id, location),
            ])
            .height(2),
        )
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length((inner.width / 4).clamp(10, 24)),
            Constraint::Min(1),
        ],
    )
    .column_spacing(1)
    .highlight_symbol(if moving { "↕ " } else { "› " })
    .row_highlight_style(
        Style::default()
            .fg(if moving {
                colors.warning
            } else {
                colors.accent
            })
            .bg(colors.border)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(
        table,
        parts[1],
        &mut TableState::default().with_selected(Some(selected)),
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{} of {} · Home/End and PgUp/PgDn also move · Esc {}",
            selected.saturating_add(1).min(order.len()),
            order.len(),
            if moving {
                "cancels this move"
            } else {
                "closes"
            }
        ))
        .style(Style::default().fg(colors.muted))
        .wrap(Wrap { trim: false }),
        parts[2],
    );
}

fn grid(frame: &mut Frame, app: &mut App, area: Rect) {
    let colors = app.settings.theme.palette();
    if app.visible.is_empty() {
        let message = if app.plants.is_empty() {
            "A little room to grow.\n\nYour plants and care history are saved automatically.\n\nWater meters are estimates, not soil sensors."
        } else {
            "No plants match this view."
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .block(panel(" The greenhouse ", false, colors))
                .wrap(Wrap { trim: false }),
            area,
        );
        app.columns = 1;
        app.page_size = 1;
        return;
    }
    let columns = (area.width as usize / 31)
        .clamp(1, 6)
        .min(app.visible.len());
    let minimum_card_height = match app.settings.card_spacing {
        CardSpacing::Relaxed => 16,
        CardSpacing::Compact => 9,
    };
    let rows = (area.height as usize / minimum_card_height)
        .max(1)
        .min(app.visible.len().div_ceil(columns));
    let page_size = columns * rows;
    app.columns = columns;
    app.page_size = page_size;
    let page = app.selected / page_size;
    let start = page * page_size;
    let row_rects = Layout::vertical(vec![Constraint::Ratio(1, rows as u32); rows]).split(area);
    for (row, row_rect) in row_rects.iter().enumerate() {
        let cells = Layout::horizontal(vec![Constraint::Ratio(1, columns as u32); columns])
            .split(*row_rect);
        for (column, cell) in cells.iter().enumerate() {
            let index = start + row * columns + column;
            if let Some(plant_index) = app.visible.get(index) {
                card(
                    frame,
                    &app.plants[*plant_index],
                    index == app.selected,
                    app,
                    *cell,
                );
            }
        }
    }
    let pages = app.visible.len().div_ceil(page_size);
    if pages > 1 {
        let text = format!(
            " page {}/{} · {} visible ",
            page + 1,
            pages,
            app.visible.len()
        );
        let width = (text.len() as u16).min(area.width);
        frame.render_widget(
            Paragraph::new(text).style(Style::default().bg(colors.bg).fg(colors.muted)),
            Rect::new(area.right() - width, area.bottom() - 1, width, 1),
        );
    }
}

fn card(frame: &mut Frame, plant: &Plant, selected: bool, app: &App, area: Rect) {
    let colors = app.settings.theme.palette();
    let block = panel(
        &format!(" {}{} ", if selected { "› " } else { "" }, plant.name()),
        selected,
        colors,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let compact = inner.height < 13;
    let art_height = if compact { 0 } else { 8 };
    let gap = u16::from(app.settings.card_spacing == CardSpacing::Relaxed && inner.height >= 16);
    let parts = Layout::vertical([
        Constraint::Length(art_height),
        Constraint::Length(gap),
        Constraint::Length(2),
        Constraint::Length(gap),
        Constraint::Length(1),
        Constraint::Length(gap),
        Constraint::Min(1),
    ])
    .split(inner);
    if art_height > 0 {
        frame.render_widget(
            Paragraph::new(centered_art(plant, app))
                .alignment(Alignment::Center)
                .style(Style::default().fg(colors.accent)),
            parts[0],
        );
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(plant.taxon.id.as_str()),
            Line::from(Span::styled(
                format!(
                    "{}{}",
                    if plant.location.is_empty() {
                        "unplaced"
                    } else {
                        &plant.location
                    },
                    if app.grouping.group(plant).is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", app.grouping.group(plant))
                    }
                ),
                Style::default().fg(colors.muted),
            )),
        ]),
        parts[2],
    );
    water_bar(frame, plant, app.now, parts[4], colors);
    let water = plant.water();
    let water_text = if let Some(due) = water.due_at() {
        if water.last_completed.is_none() {
            "Watering history unknown · check soil".into()
        } else {
            format!("Water check {}", model::countdown(due, app.now))
        }
    } else {
        "Water reminders disabled".into()
    };
    let due = plant.due_count(app.now);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                water_text,
                Style::default().fg(if water.due_at().is_some_and(|d| d <= app.now) {
                    colors.warning
                } else {
                    colors.muted
                }),
            )),
            Line::from(Span::styled(
                if due > 0 {
                    format!("● {due} care reminders due")
                } else {
                    "✓ all care reminders clear".into()
                },
                Style::default().fg(if due > 0 {
                    colors.warning
                } else {
                    colors.accent
                }),
            )),
        ]),
        parts[6],
    );
}

fn water_bar(frame: &mut Frame, plant: &Plant, now: i64, area: Rect, colors: Palette) {
    let level = plant.water().water_level(now);
    let Some(level) = level else {
        frame.render_widget(
            Paragraph::new("water estimate — unknown / off")
                .style(Style::default().fg(colors.muted)),
            area,
        );
        return;
    };
    let label = format!("water estimate {:>3.0}% ", level * 100.0);
    let bar_width = (area.width as usize).saturating_sub(Line::raw(&label).width());
    let filled = ((level.clamp(0.0, 1.0) * bar_width as f64).round() as usize).min(bar_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(label, Style::default().fg(colors.muted)),
            Span::styled(
                "█".repeat(filled),
                Style::default().fg(if level < 0.2 {
                    colors.warning
                } else {
                    colors.water
                }),
            ),
            Span::styled(
                "░".repeat(bar_width - filled),
                Style::default().fg(colors.border),
            ),
        ])),
        area,
    );
}

fn detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let colors = app.settings.theme.palette();
    let plant = app.selected_plant().unwrap();
    let mut identity: Vec<Line> = vec![];
    identity.extend(centered_art(plant, app).lines);
    identity.extend([
        Line::raw(""),
        Line::from(Span::styled(
            plant.name().to_string(),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(plant.taxon.id.clone()),
        Line::from(Span::styled(
            plant.taxon.common.clone(),
            Style::default().fg(colors.muted),
        )),
        Line::raw(format!("Family: {}", plant.taxon.family)),
        Line::raw(format!("Care groups: {}", plant.taxon.groups)),
        Line::raw(format!("Location: {}", blank(&plant.location))),
        Line::raw(format!("Acquired: {}", blank(&plant.acquired))),
        Line::raw(format!("Pot: {}", blank(&plant.pot))),
        Line::raw(format!("Substrate: {}", blank(&plant.soil))),
        Line::raw(format!("Tags: {}", blank(&plant.tags))),
        Line::raw(format!("Light: {}", plant.taxon.light)),
        Line::raw(""),
        Line::from(Span::styled(
            plant.taxon.care.clone(),
            Style::default().fg(colors.warning),
        )),
        Line::raw(""),
        Line::raw(format!("Notes: {}", blank(&plant.notes))),
    ]);
    let mut care = vec![
        Line::from(Span::styled(
            "CARE CALENDAR",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Reminders to check, not instructions to water blindly.",
            Style::default().fg(colors.muted),
        )),
        Line::raw(""),
    ];
    for schedule in &plant.schedules {
        let due = schedule.due_at();
        let overdue = due.is_some_and(|d| d <= app.now);
        care.push(Line::from(Span::styled(
            format!(
                "{} {} · {}",
                if overdue { "●" } else { "○" },
                schedule.kind.label(),
                if schedule.interval_days == 0 {
                    "off".into()
                } else {
                    format!("every {}d", schedule.interval_days)
                }
            ),
            Style::default().fg(if overdue { colors.warning } else { colors.text }),
        )));
        care.push(Line::from(Span::styled(
            format!(
                "  {}",
                due.map(|d| model::countdown(d, app.now))
                    .unwrap_or_else(|| "disabled".into())
            ),
            Style::default().fg(if overdue {
                colors.warning
            } else {
                colors.muted
            }),
        )));
        care.push(Line::from(Span::styled(
            format!(
                "  Last: {}",
                schedule
                    .last_completed
                    .map(model::date)
                    .unwrap_or_else(|| "not recorded".into())
            ),
            Style::default().fg(colors.muted),
        )));
    }
    let level = plant.water().water_level(app.now);
    care.extend([
        Line::raw(""),
        Line::from(Span::styled(
            level
                .map(|l| format!("Water estimate: {:.0}%", l * 100.0))
                .unwrap_or_else(|| "Water estimate: unknown / disabled".into()),
            Style::default().fg(colors.accent),
        )),
        Line::from(Span::styled(
            "Linear time estimate since watering; NOT a soil sensor.",
            Style::default().fg(colors.muted),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            format!("CARE JOURNAL · {} entries", app.history.len()),
            Style::default().fg(colors.accent),
        )),
    ]);
    if app.history.is_empty() {
        care.push(Line::raw("No maintenance recorded."));
    }
    for event in &app.history {
        care.push(Line::raw(format!(
            "{} · {}",
            model::date(event.occurred_at),
            event.kind.key()
        )));
        if !event.note.is_empty() {
            care.push(Line::from(Span::styled(
                format!("  {}", event.note),
                Style::default().fg(colors.muted),
            )));
        }
    }
    if area.width >= 90 {
        let split = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        let max_height = Paragraph::new(Text::from(identity.clone()))
            .wrap(Wrap { trim: false })
            .line_count(split[0].width.saturating_sub(2))
            .max(
                Paragraph::new(Text::from(care.clone()))
                    .wrap(Wrap { trim: false })
                    .line_count(split[1].width.saturating_sub(2)),
            );
        app.detail_scroll = app
            .detail_scroll
            .min(max_height.saturating_sub(area.height.saturating_sub(2) as usize) as u16);
        frame.render_widget(
            Paragraph::new(identity)
                .block(panel(" Specimen ", true, colors))
                .wrap(Wrap { trim: false })
                .scroll((app.detail_scroll, 0)),
            split[0],
        );
        frame.render_widget(
            Paragraph::new(care)
                .block(panel(" Timers ", false, colors))
                .wrap(Wrap { trim: false })
                .scroll((app.detail_scroll, 0)),
            split[1],
        );
    } else {
        identity.push(Line::raw(""));
        identity.extend(care);
        let height = Paragraph::new(Text::from(identity.clone()))
            .wrap(Wrap { trim: false })
            .line_count(area.width.saturating_sub(2));
        app.detail_scroll = app
            .detail_scroll
            .min(height.saturating_sub(area.height.saturating_sub(2) as usize) as u16);
        frame.render_widget(
            Paragraph::new(identity)
                .block(panel(" Specimen ", true, colors))
                .wrap(Wrap { trim: false })
                .scroll((app.detail_scroll, 0)),
            area,
        );
    }
}

fn catalog(frame: &mut Frame, app: &App, area: Rect, query: &str, selected: usize) {
    let colors = app.settings.theme.palette();
    let inner = popup(frame, area, 106, 38, " Plant catalog ", colors);
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .split(inner);
    let matches = app.catalog_matches(query);
    frame.render_widget(
        Paragraph::new(format!("Search: {query}▏\n{} matches", matches.len()))
            .style(Style::default().fg(colors.accent)),
        parts[0],
    );
    let count = (parts[1].height as usize / 2).max(1);
    let start = selected.min(matches.len().saturating_sub(1)) / count * count;
    let mut lines = vec![];
    for (index, taxon) in matches.iter().enumerate().skip(start).take(count) {
        let focused = index == selected;
        lines.push(Line::from(Span::styled(
            format!("{} {}", if focused { "›" } else { " " }, taxon.id),
            Style::default()
                .fg(if focused { colors.accent } else { colors.text })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        )));
        lines.push(Line::from(Span::styled(
            format!("  {} · {}", taxon.common, taxon.family),
            Style::default().fg(colors.muted),
        )));
    }
    if matches.is_empty() {
        lines.push(Line::raw("No matching plant types."));
    }
    frame.render_widget(Paragraph::new(lines), parts[1]);
    if let Some(taxon) = matches.get(selected) {
        frame.render_widget(
            Paragraph::new(format!(
                "{}\n{} · water check ~{}d\n{}",
                taxon.groups, taxon.light, taxon.water_days, taxon.care
            ))
            .style(Style::default().fg(colors.warning))
            .wrap(Wrap { trim: false }),
            parts[2],
        );
    }
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!(
                "{} Custom plant type…",
                if selected == matches.len() {
                    "›"
                } else {
                    " "
                }
            ),
            Style::default().fg(if selected == matches.len() {
                colors.accent
            } else {
                colors.muted
            }),
        )),
        parts[3],
    );
}

fn help(
    frame: &mut Frame,
    area: Rect,
    page: crate::help::Page,
    settings: Settings,
    mode: &mut Mode,
) {
    let Mode::Help {
        selected,
        offset,
        tab,
        setting,
        ..
    } = mode
    else {
        return;
    };
    let colors = settings.theme.palette();
    let inner = popup(
        frame,
        area,
        92,
        38,
        &format!(
            " {} ",
            if *tab == HelpTab::Settings {
                "Settings"
            } else {
                page.title
            }
        ),
        colors,
    );
    let parts = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
    frame.render_widget(
        Tabs::new(["Commands", "Settings"])
            .select(if *tab == HelpTab::Settings { 1 } else { 0 })
            .style(Style::default().fg(colors.muted))
            .highlight_style(
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        parts[0],
    );
    if *tab == HelpTab::Settings {
        settings_view(frame, parts[1], settings, *setting);
    } else {
        commands(frame, parts[1], page, selected, offset, colors);
    }
}

fn settings_view(frame: &mut Frame, area: Rect, settings: Settings, selected: usize) {
    let colors = settings.theme.palette();
    let compact = area.height < 12;
    let hint = "Tab pages · ↑↓ select · ←→ / Enter change · Esc return";
    let hint_height = Paragraph::new(hint)
        .wrap(Wrap { trim: false })
        .line_count(area.width)
        .min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Length(if compact { 0 } else { 2 }),
        Constraint::Min(2),
        Constraint::Length(if compact { 0 } else { 5 }),
        Constraint::Length(hint_height),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new("Personalize your garden. Changes are saved automatically in SQLite.")
            .wrap(Wrap { trim: false }),
        parts[0],
    );
    let rows = [
        Row::new([
            "Theme".to_string(),
            format!("‹ {} ›", settings.theme.label()),
        ]),
        Row::new([
            "Display artifacts".to_string(),
            if settings.display_artifacts {
                "[✓] On"
            } else {
                "[ ] Off"
            }
            .to_string(),
        ]),
        Row::new([
            "Card spacing".to_string(),
            format!("‹ {} ›", settings.card_spacing.label()),
        ]),
    ]
    .map(|row| row.height(if compact { 1 } else { 2 }));
    let table = Table::new(
        rows,
        [
            Constraint::Length(if area.width < 60 { 17 } else { 24 }),
            Constraint::Min(1),
        ],
    )
    .column_spacing(1)
    .highlight_symbol("› ")
    .row_highlight_style(
        Style::default()
            .fg(colors.accent)
            .bg(colors.border)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(
        table,
        parts[1],
        &mut TableState::default().with_selected(Some(selected)),
    );
    let description = match selected {
        0 => {
            "Default follows your terminal's background, foreground, and ANSI colors. Greenhouse is the initial leafy palette; Light and Dark use custom palettes. Changes preview immediately."
        }
        1 => {
            "Full sun sparkles often; part sun sparkles occasionally. Shade and nighttime show a small moon. Explicit sun/shade in a plant's location overrides catalog light preferences.\nDaylight is estimated as 6am–6pm local time, not weather or astronomical sunrise."
        }
        _ => {
            "Relaxed adds breathing room between card details when height allows. Compact targets shorter rows, hides artwork as needed, and can fit more plants on one page. Both adapt to the terminal size."
        }
    };
    frame.render_widget(
        Paragraph::new(description)
            .style(Style::default().fg(colors.muted))
            .wrap(Wrap { trim: false }),
        parts[2],
    );
    frame.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(colors.accent))
            .wrap(Wrap { trim: false }),
        parts[3],
    );
}

fn commands(
    frame: &mut Frame,
    inner: Rect,
    page: crate::help::Page,
    selected: &mut usize,
    offset: &mut usize,
    colors: Palette,
) {
    let compact = inner.height < 16;
    let summary_height = Paragraph::new(page.summary.as_str())
        .wrap(Wrap { trim: false })
        .line_count(inner.width)
        .min(if compact { 2 } else { 4 }) as u16;
    let navigation_height = if compact {
        0
    } else {
        Paragraph::new(page.navigation)
            .wrap(Wrap { trim: false })
            .line_count(inner.width)
            .min(5) as u16
            + 1
    };
    let hint_height = Paragraph::new("Tab pages · ↑↓ choose · Enter run · ? / Esc return")
        .wrap(Wrap { trim: false })
        .line_count(inner.width)
        .min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Length(summary_height + u16::from(inner.height >= 10)),
        Constraint::Min(2),
        Constraint::Length(navigation_height),
        Constraint::Length(hint_height),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(page.summary.as_str()).wrap(Wrap { trim: false }),
        parts[0],
    );
    let key_width = (inner.width / 3).clamp(10, 16);
    let description_width = inner.width.saturating_sub(key_width + 4).max(1);
    let rows = page.commands.iter().map(|command| {
        let keys = wrapped(command.keys, key_width);
        let description = wrapped(command.description, description_width);
        let height = keys.len().max(description.len()) as u16;
        Row::new([
            Cell::from(keys.join("\n")),
            Cell::from(description.join("\n")),
        ])
        .height(height)
    });
    *selected = (*selected).min(page.commands.len().saturating_sub(1));
    let table = Table::new(rows, [Constraint::Length(key_width), Constraint::Min(1)])
        .header(if inner.height < 12 {
            Row::default().height(0)
        } else {
            Row::new([
                "Key".to_string(),
                format!("Command · {} of {}", *selected + 1, page.commands.len()),
            ])
            .style(Style::default().fg(colors.muted))
            .bottom_margin(1)
        })
        .column_spacing(2)
        .highlight_symbol("› ")
        .row_highlight_style(
            Style::default()
                .fg(colors.accent)
                .bg(colors.border)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = TableState::default()
        .with_selected(Some(*selected))
        .with_offset(*offset);
    frame.render_stateful_widget(table, parts[1], &mut state);
    *offset = state.offset();
    frame.render_widget(
        Paragraph::new(page.navigation)
            .style(Style::default().fg(colors.muted))
            .wrap(Wrap { trim: false }),
        parts[2],
    );
    frame.render_widget(
        Paragraph::new("Tab pages · ↑↓ choose · Enter run · ? / Esc return")
            .style(Style::default().fg(colors.accent))
            .wrap(Wrap { trim: false }),
        parts[3],
    );
}

fn wrapped(text: &str, width: u16) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && Line::raw(format!("{line} {word}")).width() > width as usize {
            lines.push(word.to_string());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

fn panel(title: &str, selected: bool, colors: Palette) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title.to_string())
        .border_style(Style::default().fg(if selected {
            colors.accent
        } else {
            colors.border
        }))
        .style(Style::default().bg(colors.panel).fg(colors.text))
}
fn popup(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    colors: Palette,
) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let block = panel(title, true, colors);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}
fn blank(value: &str) -> &str {
    if value.is_empty() { "—" } else { value }
}

fn centered_art(plant: &Plant, app: &App) -> Text<'static> {
    let colors = app.settings.theme.palette();
    let drawing = art::portrait(
        plant,
        app.frame,
        app.daylight,
        app.settings.display_artifacts,
    );
    let width = drawing
        .lines()
        .map(|line| Line::raw(line).width())
        .max()
        .unwrap_or(0);
    Text::from(
        drawing
            .lines()
            .map(|line| {
                let mut spans: Vec<Span<'static>> = line
                    .chars()
                    .map(|glyph| {
                        Span::styled(
                            glyph.to_string(),
                            Style::default().fg(match glyph {
                                '✧' | '✦' | '·' => colors.warning,
                                '☾' => colors.water,
                                _ => colors.accent,
                            }),
                        )
                    })
                    .collect();
                spans.push(Span::raw(
                    " ".repeat(width.saturating_sub(Line::raw(line).width())),
                ));
                Line::from(spans)
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn resizing_and_scrolling_every_dialog_never_loses_the_selected_plant() -> anyhow::Result<()> {
        for (width, height) in [(120, 40), (90, 30), (80, 24), (38, 12), (20, 5)] {
            let mut app = App::new(crate::demo_database()?)?;
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            let id = app.selected_plant().unwrap().id;
            for key in [
                KeyCode::Null,
                KeyCode::Enter,
                KeyCode::PageDown,
                KeyCode::PageUp,
                KeyCode::Char('m'),
                KeyCode::Tab,
                KeyCode::Esc,
                KeyCode::Char('t'),
                KeyCode::Esc,
                KeyCode::Char('e'),
                KeyCode::Tab,
                KeyCode::Esc,
                KeyCode::Char('a'),
                KeyCode::Char('s'),
                KeyCode::PageDown,
                KeyCode::Esc,
                KeyCode::Char('?'),
                KeyCode::PageDown,
                KeyCode::Esc,
                KeyCode::Char('D'),
                KeyCode::Char('n'),
                KeyCode::Char('o'),
                KeyCode::Down,
                KeyCode::Enter,
                KeyCode::Down,
                KeyCode::Esc,
                KeyCode::Esc,
            ] {
                app.key(KeyEvent::new(key, KeyModifiers::NONE));
                terminal.draw(|frame| draw(frame, &mut app))?;
                assert_eq!(app.selected_plant().unwrap().id, id);
            }
            assert!(!app.error);
        }
        Ok(())
    }

    #[test]
    fn every_page_and_its_contextual_help_keep_the_same_minimal_footer() -> anyhow::Result<()> {
        for (width, height) in [(80, 24), (38, 12)] {
            let mut app = App::new(crate::demo_database()?)?;
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            for (key, title) in [
                (KeyCode::Null, "Greenhouse help"),
                (KeyCode::Enter, "Specimen help"),
                (KeyCode::Char('a'), "Plant catalog help"),
                (KeyCode::Esc, "Specimen help"),
                (KeyCode::Char('m'), "Maintenance help"),
                (KeyCode::Esc, "Specimen help"),
                (KeyCode::Char('t'), "Care intervals help"),
                (KeyCode::Esc, "Specimen help"),
                (KeyCode::Char('e'), "Edit plant help"),
                (KeyCode::Esc, "Specimen help"),
                (KeyCode::Char('/'), "Find plants help"),
                (KeyCode::Esc, "Specimen help"),
                (KeyCode::Char('D'), "Confirmation help"),
                (KeyCode::Char('n'), "Specimen help"),
                (KeyCode::Char('o'), "Reorder greenhouse help"),
                (KeyCode::Esc, "Specimen help"),
            ] {
                app.key(KeyEvent::new(key, KeyModifiers::NONE));
                for help_open in [false, true] {
                    if help_open {
                        app.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
                        let Mode::Help { previous, .. } = &app.mode else {
                            panic!("help did not open")
                        };
                        assert_eq!(crate::help::page(previous, app.detail, true).title, title);
                    }
                    terminal.draw(|frame| draw(frame, &mut app))?;
                    let buffer = terminal.backend().buffer();
                    let footer: String = (0..width)
                        .map(|x| buffer[(x, height - 1)].symbol())
                        .collect();
                    assert_eq!(footer.trim(), "? help  |  q quit");
                }
                app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            }
        }
        Ok(())
    }

    #[test]
    fn notifications_share_the_footer_when_the_complete_message_fits() -> anyhow::Result<()> {
        let message = "Recorded water for Beauty Star.";
        for (width, height, inline) in [(80, 24, true), (38, 12, false)] {
            let mut app = App::new(crate::demo_database()?)?;
            app.notify(message, false);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| draw(frame, &mut app))?;
            let buffer = terminal.backend().buffer();
            let footer: String = (0..width)
                .map(|x| buffer[(x, height - 1)].symbol())
                .collect();
            assert_eq!(footer.contains(message), inline);
            assert!(footer.contains("? help  |  q quit"));
            let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
            assert!(screen.contains(message));
        }
        Ok(())
    }

    #[test]
    fn settings_tabs_and_artifacts_render_in_every_theme_and_terminal_size() -> anyhow::Result<()> {
        use crate::settings::{CardSpacing, Theme};
        for theme in Theme::ALL {
            for (width, height) in [(120, 40), (80, 24), (38, 12)] {
                let mut app = App::new(crate::demo_database()?)?;
                app.settings.theme = theme;
                app.settings.display_artifacts = true;
                app.settings.card_spacing = CardSpacing::Compact;
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|frame| draw(frame, &mut app))?;
                app.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
                app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
                terminal.draw(|frame| draw(frame, &mut app))?;
                let buffer = terminal.backend().buffer();
                let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
                assert!(text.contains("Commands"));
                assert!(text.contains("Display artifacts"));
                assert!(text.contains("Card spacing"));
                assert!(text.contains("Compact"));
                assert!(text.contains(theme.label()));
                assert_eq!(buffer[(0, 0)].bg, theme.palette().bg);
                let footer: String = (0..width)
                    .map(|x| buffer[(x, height - 1)].symbol())
                    .collect();
                assert_eq!(footer.trim(), "? help  |  q quit");
                assert!(!text.contains("Welcome to your indoor garden"));
            }
        }
        Ok(())
    }

    #[test]
    fn compact_cards_fit_more_rows_and_water_bars_do_not_fill_backgrounds() -> anyhow::Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        let mut terminal = Terminal::new(TestBackend::new(100, 24))?;
        terminal.draw(|frame| draw(frame, &mut app))?;
        assert_eq!(app.page_size, 3);
        let buffer = terminal.backend().buffer();
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "█"));
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "░"));
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| cell.bg != app.settings.theme.palette().water)
        );

        app.settings.card_spacing = CardSpacing::Compact;
        terminal.draw(|frame| draw(frame, &mut app))?;
        assert_eq!(app.page_size, 6);
        Ok(())
    }

    #[test]
    fn artifact_setting_actually_decorates_cards_and_specimen_details() -> anyhow::Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        app.daylight = crate::environment::Daylight::Night;
        let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
        for detail in [false, true] {
            app.detail = detail;
            for enabled in [false, true] {
                app.settings.display_artifacts = enabled;
                terminal.draw(|frame| draw(frame, &mut app))?;
                let buffer = terminal.backend().buffer();
                let moons = buffer
                    .content()
                    .iter()
                    .filter(|cell| cell.symbol() == "☾")
                    .count();
                assert_eq!(
                    moons > 1,
                    enabled,
                    "detail={detail}, enabled={enabled}, moons={moons}"
                );
            }
        }
        Ok(())
    }
}
