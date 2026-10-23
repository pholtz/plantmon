use crate::{
    app::{App, CatalogPurpose, ConfirmAction, Form, HelpTab, InspectionEditor, Mode},
    art,
    health::{self, MetricAssessment, Status},
    knowledge::NumericTarget,
    model::{self, EnvironmentValue, MetricSummary, Plant, Taxon},
    settings::{CardSpacing, Palette, Settings, TemperatureUnit},
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
        Mode::Inspection(editor) => inspection_editor(frame, app, area, editor),
        Mode::FindingPicker {
            query, selected, ..
        } => finding_picker(frame, app, area, query, *selected),
        Mode::Catalog {
            query,
            selected,
            purpose,
            detail,
        } => catalog(frame, app, area, query, *selected, *purpose, *detail),
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

fn inspection_editor(frame: &mut Frame, app: &App, area: Rect, editor: &InspectionEditor) {
    let colors = app.settings.theme.palette();
    let inner = popup(
        frame,
        area,
        106,
        38,
        &format!(" Inspection · {} ", editor.draft.plant_name),
        colors,
    );
    let parts = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(2),
        Constraint::Length(2),
    ])
    .split(inner);
    let summary = if app.error {
        app.toast.clone()
    } else {
        format!(
            "When: {} · {} finding{}\nOverall note: {}",
            editor.draft.when,
            editor.draft.findings.len(),
            if editor.draft.findings.len() == 1 {
                ""
            } else {
                "s"
            },
            if editor.draft.note.is_empty() {
                "—"
            } else {
                &editor.draft.note
            }
        )
    };
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

    let mut rows = editor
        .draft
        .findings
        .iter()
        .map(|finding| {
            let mut attributes = vec![
                finding.kind.category().to_string(),
                format!("trend: {}", finding.trend.label()),
            ];
            if let Some(part) = finding.part {
                attributes.push(part.label().to_string());
            }
            if let Some(severity) = finding.severity {
                attributes.push(severity.label().to_string());
            }
            if let Some(extent) = finding.extent {
                attributes.push(extent.label().to_string());
            }
            if let Some(pest) = finding.pest {
                attributes.push(pest.label().to_string());
            }
            let description = if finding.note.is_empty() {
                attributes.join(" · ")
            } else {
                format!("{}\n{}", attributes.join(" · "), finding.note)
            };
            Row::new([finding.kind.label().to_string(), description])
                .height(if finding.note.is_empty() { 1 } else { 2 })
        })
        .collect::<Vec<_>>();
    rows.extend([
        Row::new([
            "+ Add finding".to_string(),
            "Search the observation list".to_string(),
        ]),
        Row::new([
            "Date / overall note".to_string(),
            "Edit inspection-wide details".to_string(),
        ]),
        Row::new([
            "Save inspection".to_string(),
            "Write the event and all findings".to_string(),
        ]),
        Row::new(["Cancel".to_string(), "Discard this inspection".to_string()]),
    ]);
    let table = Table::new(
        rows,
        [
            Constraint::Length((inner.width / 3).clamp(20, 34)),
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
        &mut TableState::default().with_selected(Some(editor.focused)),
    );
    frame.render_widget(
        Paragraph::new(
            "↑↓ select · Enter edit/activate · a add · d remove · e date/note · Ctrl-S save · Esc cancel",
        )
        .style(Style::default().fg(colors.accent))
        .wrap(Wrap { trim: false }),
        parts[2],
    );
}

fn finding_picker(frame: &mut Frame, app: &App, area: Rect, query: &str, selected: usize) {
    let colors = app.settings.theme.palette();
    let inner = popup(frame, area, 88, 38, " Add inspection finding ", colors);
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);
    let matches = app.inspection_finding_matches(query);
    frame.render_widget(
        Paragraph::new(format!("Search: {query}▏\n{} matches", matches.len()))
            .style(Style::default().fg(colors.accent)),
        parts[0],
    );
    let count = (parts[1].height as usize / 2).max(1);
    let start = selected.min(matches.len().saturating_sub(1)) / count * count;
    let lines = matches
        .iter()
        .enumerate()
        .skip(start)
        .take(count)
        .flat_map(|(index, kind)| {
            let focused = index == selected;
            [
                Line::from(Span::styled(
                    format!("{} {}", if focused { "›" } else { " " }, kind.label()),
                    Style::default()
                        .fg(if focused { colors.accent } else { colors.text })
                        .add_modifier(if focused {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )),
                Line::from(Span::styled(
                    format!("  {}", kind.category()),
                    Style::default().fg(colors.muted),
                )),
            ]
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(if lines.is_empty() {
            vec![Line::raw("No matching findings.")]
        } else {
            lines
        }),
        parts[1],
    );
    frame.render_widget(
        Paragraph::new("Type to search · ↑↓ / PgUp/PgDn choose · Enter add · Esc return")
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
    let environment = environment_lines(app, plant, colors);
    if !environment.is_empty() {
        identity.push(Line::raw(""));
        identity.extend(environment);
    }
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
        for finding in &event.findings {
            let mut context = vec![finding.trend.label().to_string()];
            if let Some(part) = finding.part {
                context.push(part.label().to_string());
            }
            if let Some(severity) = finding.severity {
                context.push(severity.label().to_string());
            }
            if let Some(extent) = finding.extent {
                context.push(extent.label().to_string());
            }
            if let Some(pest) = finding.pest {
                context.push(pest.label().to_string());
            }
            care.push(Line::from(vec![
                Span::styled(
                    if finding.kind.is_concern() {
                        "  ! "
                    } else {
                        "  + "
                    },
                    Style::default().fg(if finding.kind.is_concern() {
                        colors.warning
                    } else {
                        colors.accent
                    }),
                ),
                Span::styled(
                    finding.kind.label().to_string(),
                    Style::default()
                        .fg(if finding.kind.is_concern() {
                            colors.warning
                        } else {
                            colors.accent
                        })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {}", context.join(" · ")),
                    Style::default().fg(colors.muted),
                ),
            ]));
            if !finding.note.is_empty() {
                care.push(Line::from(Span::styled(
                    format!("      {}", finding.note),
                    Style::default().fg(colors.muted),
                )));
            }
        }
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
fn environment_lines(app: &App, plant: &Plant, colors: Palette) -> Vec<Line<'static>> {
    let Some(snapshot) = app.environments.get(&plant.id) else {
        return vec![];
    };
    let home = app.settings.home_environment;
    let has_reading = snapshot.latest_temperature_reading.is_some()
        || snapshot.latest_humidity_reading.is_some()
        || snapshot.latest_lux_reading.is_some()
        || snapshot.latest_ppfd_reading.is_some();
    let has_config = !plant.environment.is_empty()
        || home.latitude.is_some()
        || home.baseline_temperature_c.is_some()
        || home.baseline_humidity_percent.is_some();
    if !has_reading && !has_config {
        return vec![];
    }

    let mut lines = vec![
        Line::from(Span::styled(
            "ENVIRONMENT",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Optional local inputs. Light fit is a rough estimate, never measured plant light.",
            Style::default().fg(colors.muted),
        )),
    ];
    if let Some(latitude) = home.latitude {
        let daylight = chrono::DateTime::from_timestamp(app.now, 0)
            .map(|date| date.with_timezone(&chrono::Local).date_naive())
            .and_then(|date| crate::environment::daylight_hours(latitude, date));
        lines.push(Line::raw(match daylight {
            Some(hours) => format!(
                "Approximate latitude: {latitude:+.1}° · estimated daylight today: {hours:.1}h"
            ),
            None => format!("Approximate latitude: {latitude:+.1}°"),
        }));
    }
    if let Some(unit) = home.temperature_unit {
        lines.push(Line::raw(format!(
            "Preferred temperature unit: {}",
            unit.label()
        )));
    }
    if let Some(exposure) = plant.environment.exposure {
        lines.push(Line::raw(format!(
            "Explicit exposure: {} · plant setting",
            exposure.label()
        )));
    }
    if let Some(value) = home.baseline_temperature_c {
        lines.push(Line::raw(format!(
            "Home baseline temperature: {}",
            temperature(value, home.temperature_unit)
        )));
    }
    if let Some(value) = plant.environment.temperature_c {
        lines.push(Line::raw(format!(
            "Plant temperature override: {}",
            temperature(value, home.temperature_unit)
        )));
    }
    if let Some(value) = home.baseline_humidity_percent {
        lines.push(Line::raw(format!("Home baseline humidity: {value:.0}%")));
    }
    if let Some(value) = plant.environment.humidity_percent {
        lines.push(Line::raw(format!("Plant humidity override: {value:.0}%")));
    }

    push_observation(
        &mut lines,
        "Temperature snapshot",
        snapshot.latest_temperature_reading,
        snapshot
            .latest_temperature_reading
            .map(|value| temperature(value.value, home.temperature_unit)),
        app.now,
        colors,
        false,
    );
    push_observation(
        &mut lines,
        "Humidity snapshot",
        snapshot.latest_humidity_reading,
        snapshot
            .latest_humidity_reading
            .map(|value| format!("{:.0}%", value.value)),
        app.now,
        colors,
        false,
    );
    push_observation(
        &mut lines,
        "Illuminance snapshot",
        snapshot.latest_lux_reading,
        snapshot
            .latest_lux_reading
            .map(|value| format!("{:.0} lux", value.value)),
        app.now,
        colors,
        true,
    );
    push_observation(
        &mut lines,
        "PPFD snapshot (no DLI)",
        snapshot.latest_ppfd_reading,
        snapshot
            .latest_ppfd_reading
            .map(|value| format!("{:.1} µmol/m²/s", value.value)),
        app.now,
        colors,
        true,
    );
    push_summary(
        &mut lines,
        "24h",
        snapshot.daily,
        home.temperature_unit,
        colors,
    );
    push_summary(
        &mut lines,
        "7d",
        snapshot.weekly,
        home.temperature_unit,
        colors,
    );

    let assessment = health::assess(
        app.settings.environment_assessments_enabled,
        app.requirements.get(&plant.taxon.id),
        plant.environment.exposure,
        snapshot,
        app.now,
    );
    let Some(assessment) = assessment else {
        lines.push(Line::from(Span::styled(
            "Estimated assessments are disabled; saved inputs remain available.",
            Style::default().fg(colors.muted),
        )));
        return lines;
    };
    if !assessment.requirements_available {
        lines.push(Line::from(Span::styled(
            "Assessment: unknown · source-backed targets are unavailable for this taxon.",
            Style::default().fg(colors.muted),
        )));
        return lines;
    }
    let requirements = app
        .requirements
        .get(&plant.taxon.id)
        .expect("assessment requirements were available");
    lines.push(Line::from(Span::styled(
        format!(
            "Sourced targets · reviewed {} · content v{}{}",
            requirements.reviewed_on,
            requirements.content_version,
            requirements
                .fallback_group
                .as_ref()
                .map(|group| format!(" · care-group fallback: {group}"))
                .unwrap_or_default()
        ),
        Style::default().fg(colors.muted),
    )));
    push_assessment(
        &mut lines,
        "Rough light estimate",
        &assessment.light,
        format!(
            "{}–{} [{}]",
            requirements.exposure.minimum.label(),
            requirements.exposure.maximum.label(),
            requirements.exposure.source_ids.join(", ")
        ),
        colors,
    );
    push_assessment(
        &mut lines,
        "Temperature",
        &assessment.temperature,
        format!(
            "{} [{}]",
            numeric_target(&requirements.temperature_c, home.temperature_unit, true),
            requirements.temperature_c.source_ids.join(", ")
        ),
        colors,
    );
    let humidity_target = requirements
        .humidity_percent
        .as_ref()
        .map(|target| {
            format!(
                "{} [{}]",
                numeric_target(target, home.temperature_unit, false),
                target.source_ids.join(", ")
            )
        })
        .unwrap_or_else(|| "not available".into());
    push_assessment(
        &mut lines,
        "Humidity",
        &assessment.humidity,
        humidity_target,
        colors,
    );
    lines
}

fn push_observation(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    input: Option<EnvironmentValue>,
    displayed: Option<String>,
    now: i64,
    colors: Palette,
    always_snapshot: bool,
) {
    let (Some(input), Some(displayed)) = (input, displayed) else {
        return;
    };
    let stale = input
        .observed_at
        .is_some_and(|observed| now.saturating_sub(observed) > model::DAY);
    let context = health::value_context(input, now);
    let qualifier = if stale {
        " · stale; excluded from assessment"
    } else if always_snapshot {
        " · snapshot"
    } else {
        ""
    };
    lines.push(Line::from(Span::styled(
        format!("{label}: {displayed} · {context}{qualifier}"),
        Style::default().fg(if stale { colors.muted } else { colors.text }),
    )));
}

fn push_summary(
    lines: &mut Vec<Line<'static>>,
    window: &str,
    summary: crate::model::EnvironmentSummary,
    unit: Option<TemperatureUnit>,
    colors: Palette,
) {
    let mut metrics = Vec::new();
    if let Some(metric) = summary.temperature.filter(|metric| metric.count > 1) {
        metrics.push(format!(
            "temperature {} avg (n={})",
            temperature(metric.mean, unit),
            metric.count
        ));
    }
    if let Some(metric) = summary.humidity.filter(|metric| metric.count > 1) {
        metrics.push(format!(
            "humidity {:.0}% avg (n={})",
            metric.mean, metric.count
        ));
    }
    if let Some(metric) = summary.lux.filter(|metric| metric.count > 1) {
        metrics.push(summary_metric("illuminance", metric, "lux"));
    }
    if let Some(metric) = summary.ppfd.filter(|metric| metric.count > 1) {
        metrics.push(summary_metric("PPFD", metric, "µmol/m²/s"));
    }
    if !metrics.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("{window} observations: {}", metrics.join(" · ")),
            Style::default().fg(colors.muted),
        )));
    }
}

fn summary_metric(label: &str, metric: MetricSummary, unit: &str) -> String {
    format!("{label} {:.1} {unit} avg (n={})", metric.mean, metric.count)
}

fn push_assessment(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    assessment: &MetricAssessment,
    target: String,
    colors: Palette,
) {
    let color = match assessment.status {
        Status::WithinTarget => colors.accent,
        Status::BelowTarget | Status::AboveTarget => colors.warning,
        Status::Unknown => colors.muted,
    };
    lines.push(Line::from(vec![
        Span::raw(format!("{label}: ")),
        Span::styled(assessment.status.label(), Style::default().fg(color)),
        Span::styled(
            format!(" · sourced target {target}"),
            Style::default().fg(colors.muted),
        ),
    ]));
    if let Some(recommendation) = &assessment.recommendation {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}{}",
                assessment
                    .input_context
                    .as_ref()
                    .map(|context| format!("{context} · "))
                    .unwrap_or_default(),
                recommendation
            ),
            Style::default().fg(color),
        )));
    }
}

fn temperature(celsius: f64, unit: Option<TemperatureUnit>) -> String {
    let unit = unit.unwrap_or(TemperatureUnit::Celsius);
    format!("{:.1}{}", unit.convert_from_celsius(celsius), unit.symbol())
}

fn numeric_target(
    target: &NumericTarget,
    unit: Option<TemperatureUnit>,
    is_temperature: bool,
) -> String {
    let value = |value: f64| {
        if is_temperature {
            temperature(value, unit)
        } else {
            format!("{value:.0}%")
        }
    };
    match target.maximum {
        Some(maximum) => format!("{}–{}", value(target.minimum), value(maximum)),
        None => format!("at least {}", value(target.minimum)),
    }
}

fn catalog(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    query: &str,
    selected: usize,
    purpose: CatalogPurpose,
    detail: bool,
) {
    let colors = app.settings.theme.palette();
    let adding = purpose == CatalogPurpose::AddPlant;
    let inner = popup(
        frame,
        area,
        106,
        38,
        &format!(" {} ", crate::catalog::CATALOG_TITLE),
        colors,
    );
    let matches = app.catalog_matches(query);
    if detail && let Some(taxon) = matches.get(selected) {
        catalog_detail(frame, app, inner, taxon, selected, matches.len());
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(4),
        Constraint::Length(u16::from(adding)),
    ])
    .split(inner);
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
    if adding {
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
}

fn catalog_detail(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    taxon: &Taxon,
    selected: usize,
    total: usize,
) {
    let colors = app.settings.theme.palette();
    let details = Paragraph::new(vec![
        botanical_name(taxon, colors),
        Line::from(Span::styled(
            taxon.common.clone(),
            Style::default()
                .fg(colors.muted)
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        detail_field("Family", taxon.family.clone(), colors),
        detail_field("Care groups", taxon.groups.clone(), colors),
        detail_field("Growth habit", taxon.habit.clone(), colors),
        Line::raw(""),
        detail_label("LIGHT & WATER", colors),
        detail_field("Light", taxon.light.clone(), colors),
        detail_field(
            "Water check",
            format!("{} days (starting interval)", taxon.water_days),
            colors,
        ),
        Line::raw(""),
        detail_label("CARE NOTES", colors),
        Line::from(Span::styled(
            taxon.care.clone(),
            Style::default()
                .fg(colors.warning)
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Catalog guidance is a starting point; adjust care to the plant and its conditions.",
            Style::default().fg(colors.muted),
        )),
    ])
    .block(panel(
        &format!(" Details · {} of {} ", selected + 1, total),
        true,
        colors,
    ))
    .wrap(Wrap { trim: false });

    let portrait = Paragraph::new(styled_art(art::silhouette(taxon, app.frame / 2), colors))
        .alignment(Alignment::Center)
        .block(panel(&format!(" {} ", taxon.habit), false, colors));

    if area.width >= 72 && area.height >= 14 {
        let parts = Layout::horizontal([
            Constraint::Length((area.width / 3).clamp(24, 34)),
            Constraint::Min(1),
        ])
        .split(area);
        frame.render_widget(portrait, parts[0]);
        frame.render_widget(details, parts[1]);
    } else if area.height >= 26 {
        let parts = Layout::vertical([Constraint::Length(11), Constraint::Min(1)]).split(area);
        frame.render_widget(portrait, parts[0]);
        frame.render_widget(details, parts[1]);
    } else {
        frame.render_widget(details, area);
    }
}

fn botanical_name(taxon: &Taxon, colors: Palette) -> Line<'static> {
    let italic = Style::default()
        .fg(colors.accent)
        .add_modifier(Modifier::BOLD | Modifier::ITALIC);
    let designation = Style::default()
        .fg(colors.accent)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::styled(taxon.genus.clone(), italic)];
    if !taxon.species.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(taxon.species.clone(), italic));
    }
    if !taxon.designation.is_empty() {
        spans.push(Span::styled(format!(" {}", taxon.designation), designation));
    }
    Line::from(spans)
}

fn detail_label(label: &'static str, colors: Palette) -> Line<'static> {
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    ))
}

fn detail_field(label: &'static str, value: String, colors: Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(colors.muted)),
        Span::raw(value),
    ])
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
        Row::new([
            "Environment setup".to_string(),
            format!(
                "[{}] {} · Enter to configure",
                if settings.environment_assessments_enabled {
                    "✓"
                } else {
                    " "
                },
                if settings.home_environment.is_empty() {
                    "No profile"
                } else {
                    "Profile saved locally"
                }
            ),
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
        2 => {
            "Relaxed adds breathing room between card details when height allows. Compact targets shorter rows, hides artwork as needed, and can fit more plants on one page. Both adapt to the terminal size."
        }
        _ => {
            "Optional environment data stays in SQLite. Enter opens setup for approximate latitude, preferred unit, and baselines. Assessments are off by default and can be disabled without deleting inputs. No address, geocoding, weather, or network calls are used."
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
    styled_art(&drawing, colors)
}

fn styled_art(drawing: &str, colors: Palette) -> Text<'static> {
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
                KeyCode::Char('i'),
                KeyCode::Char('a'),
                KeyCode::Esc,
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
                (KeyCode::Char('c'), "Catalog help"),
                (KeyCode::Esc, "Specimen help"),
                (KeyCode::Char('m'), "Maintenance help"),
                (KeyCode::Esc, "Specimen help"),
                (KeyCode::Char('i'), "Inspection help"),
                (KeyCode::Char('a'), "Inspection finding help"),
                (KeyCode::Esc, "Inspection help"),
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
    fn catalog_browser_uses_the_default_title_without_add_controls() -> anyhow::Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
        terminal.draw(|frame| draw(frame, &mut app))?;
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains(crate::catalog::CATALOG_TITLE));
        assert!(!text.contains("Field Guide"));
        assert!(!text.contains("Custom plant type"));

        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        terminal.draw(|frame| draw(frame, &mut app))?;
        let buffer = terminal.backend().buffer();
        let detail: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(detail.contains("LIGHT & WATER"));
        assert!(detail.contains("CARE NOTES"));
        assert!(detail.contains("starting interval"));
        assert!(
            buffer
                .content()
                .iter()
                .any(|cell| cell.modifier.contains(Modifier::ITALIC))
        );
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "█"));
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

    #[test]
    fn environment_detail_respects_opt_in_and_unsupported_taxa_without_changing_cards()
    -> anyhow::Result<()> {
        use crate::{
            db::PlantInput,
            model::{PlantEnvironment, SunExposure},
            settings::{HomeEnvironment, TemperatureUnit},
        };

        let mut db = crate::db::Database::memory()?;
        let taxon = db
            .taxa()?
            .into_iter()
            .find(|taxon| taxon.id == "Goeppertia warszewiczii")
            .unwrap();
        let id = db.add_plant(&taxon, &PlantInput::default(), crate::model::now())?;
        db.set_plant_environment(
            id,
            PlantEnvironment {
                exposure: Some(SunExposure::PartialSun),
                humidity_percent: Some(65.0),
                ..Default::default()
            },
        )?;
        let mut settings = db.settings()?;
        settings.home_environment = HomeEnvironment {
            temperature_unit: Some(TemperatureUnit::Celsius),
            baseline_temperature_c: Some(22.0),
            ..Default::default()
        };
        db.save_settings(settings)?;
        let mut app = App::new(db)?;
        app.detail = true;
        let mut terminal = Terminal::new(TestBackend::new(120, 60))?;
        terminal.draw(|frame| draw(frame, &mut app))?;
        let disabled: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(disabled.contains("ENVIRONMENT"));
        assert!(disabled.contains("assessments are disabled"));
        assert!(!disabled.contains("within target"));

        settings.environment_assessments_enabled = true;
        app.db.save_settings(settings)?;
        app.reload()?;
        terminal.draw(|frame| draw(frame, &mut app))?;
        let enabled: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(enabled.contains("within target"));
        assert!(enabled.contains("Sourced targets"));

        let mut unsupported_db = crate::db::Database::memory()?;
        let unsupported = unsupported_db
            .taxa()?
            .into_iter()
            .find(|taxon| taxon.id == "Monstera deliciosa")
            .unwrap();
        let unsupported_id =
            unsupported_db.add_plant(&unsupported, &PlantInput::default(), crate::model::now())?;
        unsupported_db.set_plant_environment(
            unsupported_id,
            PlantEnvironment {
                exposure: Some(SunExposure::PartialSun),
                ..Default::default()
            },
        )?;
        let mut unsupported_settings = unsupported_db.settings()?;
        unsupported_settings.environment_assessments_enabled = true;
        unsupported_db.save_settings(unsupported_settings)?;
        let mut unsupported_app = App::new(unsupported_db)?;
        unsupported_app.detail = true;
        terminal.draw(|frame| draw(frame, &mut unsupported_app))?;
        let unsupported_text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            unsupported_text.contains("source-backed") && unsupported_text.contains("unavailable")
        );

        unsupported_app.detail = false;
        let mut compact = Terminal::new(TestBackend::new(38, 12))?;
        compact.draw(|frame| draw(frame, &mut unsupported_app))?;
        let card: String = compact
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!card.contains("ENVIRONMENT"));
        Ok(())
    }
}
