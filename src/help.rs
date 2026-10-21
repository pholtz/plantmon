use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{CatalogPurpose, FormAction, Mode};

pub struct Command {
    pub keys: &'static str,
    pub description: &'static str,
    pub event: KeyEvent,
}

pub struct Page {
    pub title: &'static str,
    pub summary: String,
    pub commands: Vec<Command>,
    pub navigation: &'static str,
}

fn command(keys: &'static str, description: &'static str, code: KeyCode) -> Command {
    Command {
        keys,
        description,
        event: KeyEvent::new(code, KeyModifiers::NONE),
    }
}

fn control(keys: &'static str, description: &'static str, code: char) -> Command {
    Command {
        keys,
        description,
        event: KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL),
    }
}

pub fn page(mode: &Mode, detail: bool, has_plant: bool) -> Page {
    use KeyCode::{Char, Enter, Esc};
    let mut page = match mode {
        Mode::Browse => {
            let mut commands = vec![
                command("a", "Add a plant", Char('a')),
                command("c", crate::catalog::CATALOG_TITLE, Char('c')),
                command("/", "Find plants", Char('/')),
                command("g", "Change grouping", Char('g')),
                command("d", "Toggle due-only view", Char('d')),
            ];
            if has_plant {
                commands.push(command("o", "Reorder greenhouse", Char('o')));
                commands.push(command("Enter / Space", if detail { "Return to greenhouse" } else { "Open specimen details" }, Enter));
                commands.extend([
                    command("m / W", "Log care or a backdate", Char('m')),
                    command("w", "Record watering now", Char('w')),
                    command("r", "Record rotation now", Char('r')),
                    command("f", "Record fertilizing now", Char('f')),
                    command("i", "Record inspection now", Char('i')),
                    command("t", "Adjust care intervals", Char('t')),
                    command("e", "Edit plant details", Char('e')),
                    command("E", "Configure plant environment", Char('E')),
                    command("v", "Log environment snapshot", Char('v')),
                    command("u", "Undo last-entered care", Char('u')),
                    command("D", "Delete plant", Char('D')),
                ]);
            }
            commands.extend([
                command("R", "Reload database", Char('R')),
                command("Esc", if detail { "Return to greenhouse" } else { "Clear collection filters" }, Esc),
            ]);
            Page {
                title: if detail { "Specimen help" } else { "Greenhouse help" },
                summary: if detail {
                    "Review this plant's identity, care timers, and journal. Water levels are estimates, not moisture readings."
                } else {
                    "Browse your plants and their care reminders. Choose a command below to get started."
                }.into(),
                commands,
                navigation: "Arrows / hjkl: select plant. Tab / Shift-Tab: next / previous. Home / End: first / last. PgUp / PgDn: page cards or scroll details.",
            }
        }
        Mode::Reorder {
            before_move, ..
        } => Page {
            title: "Reorder greenhouse help",
            summary: if before_move.is_some() {
                "Move the picked-up plant to its new position. Dropping saves the complete greenhouse order immediately."
            } else {
                "Arrange every plant in a persistent manual order. Pick up a row, move it, and drop it to save."
            }
            .into(),
            commands: vec![
                command(
                    "Enter / Space",
                    if before_move.is_some() {
                        "Drop plant and save order"
                    } else {
                        "Pick up selected plant"
                    },
                    Enter,
                ),
                command(
                    "Esc",
                    if before_move.is_some() {
                        "Cancel current move"
                    } else {
                        "Close reorder list"
                    },
                    Esc,
                ),
            ],
            navigation: "Up / Down or j / k: select a row, or move a picked-up plant. Home / End: first / last. PgUp / PgDn: move by ten.",
        },
        Mode::Catalog {
            purpose,
            detail: catalog_detail,
            ..
        } => {
            if *catalog_detail {
                return Page {
                    title: "Catalog detail help",
                    summary: "Review the selected plant type, its catalog care notes, and growth-habit illustration.".into(),
                    commands: vec![
                        command("Esc", "Return to catalog results", Esc),
                        command("q / Ctrl-C", "Quit plantmon", Char('q')),
                    ],
                    navigation: "Arrows / hjkl: previous / next catalog entry. Home / End: first / last. PgUp / PgDn: move by ten.",
                };
            }
            let adding = *purpose == CatalogPurpose::AddPlant;
            let mut commands = vec![];
            if adding {
                commands.extend([
                    command("Enter", "Choose selected type", Enter),
                    control("Ctrl-N", "Create custom plant type", 'n'),
                ]);
            } else {
                commands.push(command("Enter", "Open selected details", Enter));
            }
            commands.extend([
                control("Ctrl-U", "Clear search", 'u'),
                command(
                    "Esc",
                    if adding {
                        "Cancel adding a plant"
                    } else {
                        "Return to greenhouse"
                    },
                    Esc,
                ),
            ]);
            Page {
                title: if adding {
                    "Plant catalog help"
                } else {
                    "Catalog help"
                },
                summary: if adding {
                    "Find a species, cultivar, or hybrid by botanical name, common name, family, or care group. The custom type option is always available below the results."
                } else {
                    "Browse the bundled plant catalog by botanical name, common name, family, or care group."
                }
                .into(),
                commands,
                navigation: if adding {
                    "Type to search; q is text here. Up / Down: select a result or custom type. PgUp / PgDn: page results. Home / End: first result / custom type."
                } else {
                    "Type to search; q is text here. Up / Down: select a result. PgUp / PgDn: page results. Home / End: first / last result."
                },
            }
        }
        Mode::Search { .. } => Page {
            title: "Find plants help",
            summary: "Filter your collection by botanical or common name, family, care group, location, or personal tags. An empty search shows all plants.".into(),
            commands: vec![
                command("Enter", "Apply search", Enter),
                control("Ctrl-U", "Clear search text", 'u'),
                command("Esc", "Cancel search", Esc),
            ],
            navigation: "Type to search; q is text here. Backspace: erase. Enter: apply. Escape leaves the input without changing your filter.",
        },
        Mode::Form(form) => {
            let title = match &form.action {
                FormAction::Add(_) => "Add plant help",
                FormAction::Edit(_) => "Edit plant help",
                FormAction::Schedules(_) => "Care intervals help",
                FormAction::Log { .. } => "Maintenance help",
                FormAction::Custom => "Custom plant type help",
                FormAction::EnvironmentSettings => "Environment setup help",
                FormAction::PlantEnvironment(_) => "Plant environment help",
                FormAction::LogEnvironment(_) => "Environment log help",
            };
            let mut summary = form.hint.clone();
            if matches!(form.action, FormAction::Log { .. } | FormAction::LogEnvironment(_)) {
                summary.push_str(" Dates: now, yesterday, 2d, 12h, 30m, YYYY-MM-DD, or YYYY-MM-DD HH:MM. Future times are rejected.");
            }
            let mut commands = vec![control("Ctrl-S", "Save", 's')];
            if form.fields.get(form.focused).is_some_and(|field| field.choices.is_empty()) {
                commands.push(control("Ctrl-U", "Clear current field", 'u'));
            }
            commands.push(command("Esc", "Cancel without saving", Esc));
            Page {
                title,
                summary,
                commands,
                navigation: "Up / Down or Tab: move between fields, Save, and Cancel. Enter: advance or activate. Left / Right: choose options or adjust days. Typing replaces a focused text value; Backspace erases. q is text inside text fields.",
            }
        }
        Mode::Confirm { prompt, .. } => Page {
            title: "Confirmation help",
            summary: format!("{prompt} Cancel is selected by default. Nothing changes until you explicitly confirm."),
            commands: vec![
                command("y", "Confirm action", Char('y')),
                command("n / Esc", "Cancel action", Esc),
            ],
            navigation: "Arrows or Tab: select Cancel or Confirm. Enter: activate selection. y confirms immediately; n cancels.",
        },
        Mode::Help { previous, .. } => return page(previous, detail, has_plant),
    };
    page.commands
        .push(command("q / Ctrl-C", "Quit plantmon", Char('q')));
    page
}
