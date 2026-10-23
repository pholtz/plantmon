use crate::{
    db::{Database, PlantInput},
    environment::Daylight,
    knowledge::EnvironmentalRequirements,
    model::{
        self, EnvironmentReadingInput, EnvironmentScope, EnvironmentSnapshot, EnvironmentSource,
        InspectionExtent, InspectionFindingInput, InspectionFindingKind, InspectionSeverity,
        InspectionTrend, Maintenance, PestKind, Plant, PlantEnvironment, PlantPart, SunExposure,
        TaskKind, Taxon,
    },
    settings::{HomeEnvironment, Settings, TemperatureUnit},
};
use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const NOTIFICATION_DURATION: Duration = Duration::from_secs(5);
const SETTINGS_COUNT: usize = 4;
const PART_CHOICES: &[&str] = &[
    "not_set",
    "whole_plant",
    "leaf",
    "new_growth",
    "stem",
    "crown",
    "root",
    "flower",
    "fruit",
    "substrate",
    "container",
];
const SEVERITY_CHOICES: &[&str] = &["not_set", "mild", "moderate", "severe"];
const EXTENT_CHOICES: &[&str] = &["not_set", "single", "few", "localized", "widespread"];
const TREND_CHOICES: &[&str] = &["new", "improving", "stable", "worsening", "resolved"];
const PEST_CHOICES: &[&str] = &[
    "not_set",
    "unknown",
    "aphid",
    "mealybug",
    "root_mealybug",
    "scale",
    "spider_mite",
    "thrips",
    "whitefly",
    "fungus_gnat",
    "caterpillar",
    "beetle_or_weevil",
    "snail_or_slug",
    "other",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grouping {
    Name,
    Family,
    Care,
    Location,
    Manual,
}
impl Grouping {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Family => "family",
            Self::Care => "care group",
            Self::Location => "location",
            Self::Manual => "manual order",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Family,
            Self::Family => Self::Care,
            Self::Care => Self::Location,
            Self::Location => Self::Manual,
            Self::Manual => Self::Name,
        }
    }
    pub fn group(self, p: &Plant) -> &str {
        match self {
            Self::Name | Self::Manual => "",
            Self::Family => &p.taxon.family,
            Self::Care => p.taxon.groups.split(',').next().unwrap_or("other"),
            Self::Location => {
                if p.location.is_empty() {
                    "unplaced"
                } else {
                    &p.location
                }
            }
        }
    }

    fn compare(self, a: &Plant, b: &Plant) -> std::cmp::Ordering {
        if self == Self::Manual {
            return a.display_order.cmp(&b.display_order).then(a.id.cmp(&b.id));
        }
        self.group(a)
            .to_lowercase()
            .cmp(&self.group(b).to_lowercase())
            .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
            .then(a.id.cmp(&b.id))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HelpTab {
    #[default]
    Commands,
    Settings,
}

impl HelpTab {
    fn next(self) -> Self {
        match self {
            Self::Commands => Self::Settings,
            Self::Settings => Self::Commands,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogPurpose {
    View,
    AddPlant,
}

pub enum Mode {
    Browse,
    Catalog {
        query: String,
        selected: usize,
        purpose: CatalogPurpose,
        detail: bool,
    },
    Search {
        value: String,
    },
    Reorder {
        order: Vec<i64>,
        selected: usize,
        before_move: Option<Vec<i64>>,
    },
    Inspection(InspectionEditor),
    FindingPicker {
        editor: Box<InspectionEditor>,
        query: String,
        selected: usize,
    },
    Form(Form),
    Help {
        previous: Box<Mode>,
        selected: usize,
        offset: usize,
        tab: HelpTab,
        setting: usize,
    },
    Confirm {
        action: ConfirmAction,
        prompt: String,
        confirm: bool,
    },
}
pub enum ConfirmAction {
    DeletePlant(i64),
    UndoEvent { plant: i64, event: i64 },
}
pub enum FormAction {
    Add(Box<Taxon>),
    Edit(i64),
    Schedules(i64),
    Log {
        plant: i64,
        kind: TaskKind,
    },
    EnvironmentSettings,
    PlantEnvironment(i64),
    LogEnvironment(i64),
    InspectionDetails(Box<InspectionEditor>),
    InspectionFinding {
        editor: Box<InspectionEditor>,
        kind: InspectionFindingKind,
        index: Option<usize>,
    },
    Custom,
}

#[derive(Clone)]
pub struct InspectionDraft {
    pub plant: i64,
    pub plant_name: String,
    pub when: String,
    pub note: String,
    pub findings: Vec<InspectionFindingInput>,
}

#[derive(Clone)]
pub struct InspectionEditor {
    pub draft: InspectionDraft,
    pub focused: usize,
}

impl InspectionEditor {
    fn new(plant: i64, plant_name: String) -> Self {
        Self {
            draft: InspectionDraft {
                plant,
                plant_name,
                when: "now".into(),
                note: String::new(),
                findings: vec![default_inspection_finding(
                    InspectionFindingKind::NoAdverseSigns,
                )],
            },
            focused: 0,
        }
    }

    pub fn add_index(&self) -> usize {
        self.draft.findings.len()
    }

    pub fn details_index(&self) -> usize {
        self.add_index() + 1
    }

    pub fn save_index(&self) -> usize {
        self.add_index() + 2
    }

    pub fn cancel_index(&self) -> usize {
        self.add_index() + 3
    }
}
pub struct Field {
    pub label: &'static str,
    pub value: String,
    pub choices: &'static [&'static str],
    pub numeric: bool,
}
pub struct Form {
    pub title: String,
    pub hint: String,
    pub fields: Vec<Field>,
    pub focused: usize,
    pub replace_on_type: bool,
    pub action: FormAction,
    pub return_to: Option<Box<Mode>>,
}

impl Form {
    fn new(
        title: impl Into<String>,
        hint: &str,
        action: FormAction,
        fields: Vec<(&'static str, String)>,
    ) -> Self {
        let fields = fields
            .into_iter()
            .enumerate()
            .map(|(index, (label, value))| {
                let choices = match (&action, index) {
                    (FormAction::Log { .. }, 0) => {
                        &["water", "rotate", "fertilize", "soil", "inspect"][..]
                    }
                    (FormAction::EnvironmentSettings, 0) => &["off", "on"][..],
                    (FormAction::EnvironmentSettings, 2) => {
                        &["not set", "celsius", "fahrenheit"][..]
                    }
                    (FormAction::PlantEnvironment(_), 0) => &[
                        "not set",
                        "full_shade",
                        "mostly_shade",
                        "partial_sun",
                        "mostly_sun",
                        "full_sun",
                    ][..],
                    (FormAction::LogEnvironment(_), 0) => &["plant", "home"][..],
                    (FormAction::InspectionFinding { .. }, 0) => PART_CHOICES,
                    (FormAction::InspectionFinding { .. }, 1) => SEVERITY_CHOICES,
                    (FormAction::InspectionFinding { .. }, 2) => EXTENT_CHOICES,
                    (FormAction::InspectionFinding { .. }, 3) => TREND_CHOICES,
                    (FormAction::InspectionFinding { .. }, 4) => PEST_CHOICES,
                    (FormAction::Custom, 6) => crate::art::HABITS,
                    _ => &[],
                };
                let numeric = matches!(
                    (&action, index),
                    (FormAction::Schedules(_), _) | (FormAction::Custom, 7)
                );
                Field {
                    label,
                    value,
                    choices,
                    numeric,
                }
            })
            .collect();
        Self {
            title: title.into(),
            hint: hint.into(),
            action,
            focused: 0,
            replace_on_type: true,
            fields,
            return_to: None,
        }
    }
    fn returning_to(mut self, mode: Mode) -> Self {
        self.return_to = Some(Box::new(mode));
        self
    }
    fn values(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|f| f.value.trim().to_string())
            .collect()
    }
}

pub struct App {
    pub db: Database,
    pub plants: Vec<Plant>,
    pub taxa: Vec<Taxon>,
    pub visible: Vec<usize>,
    pub selected: usize,
    pub detail: bool,
    pub detail_scroll: u16,
    pub history: Vec<Maintenance>,
    pub mode: Mode,
    pub settings: Settings,
    pub environments: HashMap<i64, EnvironmentSnapshot>,
    pub requirements: HashMap<String, EnvironmentalRequirements>,
    pub daylight: Daylight,
    pub query: String,
    pub due_only: bool,
    pub grouping: Grouping,
    pub toast: String,
    pub error: bool,
    toast_expires_at: Option<Instant>,
    pub quit: bool,
    pub now: i64,
    pub frame: u64,
    pub columns: usize,
    pub page_size: usize,
}

impl App {
    pub fn new(db: Database) -> Result<Self> {
        let now = model::now();
        let mut app = Self {
            db,
            plants: vec![],
            taxa: vec![],
            visible: vec![],
            selected: 0,
            detail: false,
            detail_scroll: 0,
            history: vec![],
            mode: Mode::Browse,
            settings: Settings::default(),
            environments: HashMap::new(),
            requirements: HashMap::new(),
            daylight: Daylight::at(now),
            query: String::new(),
            due_only: false,
            grouping: Grouping::Name,
            toast: String::new(),
            error: false,
            toast_expires_at: None,
            quit: false,
            now,
            frame: 0,
            columns: 1,
            page_size: 1,
        };
        app.reload()?;
        if app.settings.manual_order {
            app.grouping = Grouping::Manual;
            app.refilter();
        }
        app.selected = 0;
        app.reload_history()?;
        Ok(app)
    }

    pub fn selected_plant(&self) -> Option<&Plant> {
        self.visible
            .get(self.selected)
            .and_then(|i| self.plants.get(*i))
    }

    pub fn catalog_matches(&self, query: &str) -> Vec<&Taxon> {
        self.taxa.iter().filter(|t| t.matches(query)).collect()
    }

    pub fn inspection_finding_matches(&self, query: &str) -> Vec<InspectionFindingKind> {
        let words = query.to_lowercase();
        InspectionFindingKind::ALL
            .iter()
            .copied()
            .filter(|kind| {
                let haystack =
                    format!("{} {} {}", kind.key(), kind.label(), kind.category()).to_lowercase();
                words.split_whitespace().all(|word| haystack.contains(word))
            })
            .collect()
    }

    pub fn refilter(&mut self) {
        let selected_id = self.selected_plant().map(|p| p.id);
        self.visible = (0..self.plants.len())
            .filter(|i| {
                let p = &self.plants[*i];
                p.matches(&self.query) && (!self.due_only || p.due_count(self.now) > 0)
            })
            .collect();
        self.visible
            .sort_by(|a, b| self.grouping.compare(&self.plants[*a], &self.plants[*b]));
        self.selected = selected_id
            .and_then(|id| self.visible.iter().position(|i| self.plants[*i].id == id))
            .unwrap_or(self.selected.min(self.visible.len().saturating_sub(1)));
    }

    pub fn reload(&mut self) -> Result<()> {
        let selected_id = self.selected_plant().map(|p| p.id);
        self.settings = self.db.settings()?;
        self.taxa = self.db.taxa()?;
        self.plants = self.db.plants()?;
        self.requirements.clear();
        for plant in &self.plants {
            if !self.requirements.contains_key(&plant.taxon.id)
                && let Some(requirement) = crate::knowledge::requirements_for(&plant.taxon)?
            {
                self.requirements
                    .insert(plant.taxon.id.clone(), requirement);
            }
        }
        self.environments.clear();
        for plant in &self.plants {
            self.environments
                .insert(plant.id, self.db.environment_snapshot(plant.id, self.now)?);
        }
        self.daylight =
            Daylight::at_with_latitude(self.now, self.settings.home_environment.latitude);
        // Discard indexes into the old collection before trying to restore selection.
        self.visible.clear();
        self.refilter();
        if let Some(pos) =
            selected_id.and_then(|id| self.visible.iter().position(|i| self.plants[*i].id == id))
        {
            self.selected = pos;
        }
        self.reload_history()
    }

    fn reload_history(&mut self) -> Result<()> {
        self.history = match self.selected_plant() {
            Some(p) => self.db.history(p.id)?,
            None => vec![],
        };
        Ok(())
    }

    pub fn tick(&mut self) {
        self.now = model::now();
        self.daylight =
            Daylight::at_with_latitude(self.now, self.settings.home_environment.latitude);
        self.frame += 1;
        self.refilter();
        if self
            .toast_expires_at
            .is_some_and(|expires_at| Instant::now() >= expires_at)
        {
            self.toast.clear();
            self.error = false;
            self.toast_expires_at = None;
        }
    }

    pub fn notify(&mut self, message: impl Into<String>, error: bool) {
        self.toast = message.into();
        self.error = error && !self.toast.is_empty();
        self.toast_expires_at =
            (!self.toast.is_empty()).then(|| Instant::now() + NOTIFICATION_DURATION);
    }

    pub fn key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        if key.code == KeyCode::Char('?') && !matches!(mode, Mode::Help { .. }) {
            self.mode = Mode::Help {
                previous: Box::new(mode),
                selected: 0,
                offset: 0,
                tab: HelpTab::Commands,
                setting: 0,
            };
            return;
        }
        let text_input = match &mode {
            Mode::Catalog { detail, .. } => !detail,
            Mode::Search { .. } | Mode::FindingPicker { .. } => true,
            Mode::Form(form) => form
                .fields
                .get(form.focused)
                .is_some_and(|field| field.choices.is_empty()),
            _ => false,
        };
        if key.code == KeyCode::Char('q') && !text_input {
            self.quit = true;
            self.mode = mode;
            return;
        }
        match self.handle_mode(mode, key) {
            Ok(mode) => self.mode = mode,
            Err(err) => self.notify(format!("{err:#}"), true),
        }
    }

    fn handle_mode(&mut self, mode: Mode, key: KeyEvent) -> Result<Mode> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match mode {
            Mode::Help {
                previous,
                mut selected,
                mut offset,
                mut tab,
                mut setting,
            } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('?') => return Ok(*previous),
                    KeyCode::Tab | KeyCode::BackTab => tab = tab.next(),
                    _ if tab == HelpTab::Settings => match key.code {
                        KeyCode::Up | KeyCode::Char('k') => setting = setting.saturating_sub(1),
                        KeyCode::Down | KeyCode::Char('j') => {
                            setting = (setting + 1).min(SETTINGS_COUNT - 1)
                        }
                        KeyCode::Home => setting = 0,
                        KeyCode::End => setting = SETTINGS_COUNT - 1,
                        KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                            if setting == 3 {
                                let return_mode = Mode::Help {
                                    previous,
                                    selected,
                                    offset,
                                    tab,
                                    setting,
                                };
                                return Ok(Mode::Form(
                                    environment_settings_form(self.settings)
                                        .returning_to(return_mode),
                                ));
                            }
                            let mut settings = self.settings;
                            match setting {
                                0 => {
                                    settings.theme = settings
                                        .theme
                                        .cycle(if key.code == KeyCode::Left { -1 } else { 1 });
                                }
                                1 => {
                                    settings.display_artifacts = match key.code {
                                        KeyCode::Left => false,
                                        KeyCode::Right => true,
                                        _ => !settings.display_artifacts,
                                    };
                                }
                                2 => {
                                    settings.card_spacing = settings
                                        .card_spacing
                                        .cycle(if key.code == KeyCode::Left { -1 } else { 1 });
                                }
                                _ => unreachable!("settings selection is bounded"),
                            }
                            if settings != self.settings {
                                match self.db.save_settings(settings) {
                                    Ok(()) => {
                                        self.settings = settings;
                                        self.notify("", false);
                                    }
                                    Err(error) => self.notify(
                                        format!("Could not save settings: {error:#}"),
                                        true,
                                    ),
                                }
                            }
                        }
                        _ => {}
                    },
                    KeyCode::Left | KeyCode::Right => tab = HelpTab::Settings,
                    _ => {
                        let page = crate::help::page(
                            &previous,
                            self.detail,
                            self.selected_plant().is_some(),
                        );
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                selected = selected.saturating_sub(1)
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                selected = (selected + 1).min(page.commands.len().saturating_sub(1))
                            }
                            KeyCode::PageUp => selected = selected.saturating_sub(5),
                            KeyCode::PageDown => {
                                selected = (selected + 5).min(page.commands.len().saturating_sub(1))
                            }
                            KeyCode::Home => {
                                selected = 0;
                                offset = 0;
                            }
                            KeyCode::End => selected = page.commands.len().saturating_sub(1),
                            KeyCode::Enter => {
                                if let Some(command) = page.commands.get(selected) {
                                    if command.event.code == KeyCode::Char('q') {
                                        self.quit = true;
                                        return Ok(*previous);
                                    }
                                    return self.handle_mode(*previous, command.event);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Mode::Help {
                    previous,
                    selected,
                    offset,
                    tab,
                    setting,
                })
            }
            Mode::Inspection(mut editor) => {
                let last = editor.cancel_index();
                match key.code {
                    KeyCode::Esc => return Ok(Mode::Browse),
                    KeyCode::Char('s') if ctrl => {
                        return Ok(self.submit_inspection(editor));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        editor.focused = editor.focused.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                        editor.focused = (editor.focused + 1).min(last)
                    }
                    KeyCode::BackTab => editor.focused = editor.focused.saturating_sub(1),
                    KeyCode::Home => editor.focused = 0,
                    KeyCode::End => editor.focused = last,
                    KeyCode::Char('a') => {
                        return Ok(Mode::FindingPicker {
                            editor: Box::new(editor),
                            query: String::new(),
                            selected: 0,
                        });
                    }
                    KeyCode::Char('e') => {
                        return Ok(Mode::Form(inspection_details_form(editor)));
                    }
                    KeyCode::Char('d') if editor.focused < editor.draft.findings.len() => {
                        editor.draft.findings.remove(editor.focused);
                        editor.focused = editor.focused.min(editor.cancel_index());
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if editor.focused < editor.draft.findings.len() {
                            let index = editor.focused;
                            let kind = editor.draft.findings[index].kind;
                            return Ok(Mode::Form(inspection_finding_form(
                                editor,
                                kind,
                                Some(index),
                            )));
                        }
                        if editor.focused == editor.add_index() {
                            return Ok(Mode::FindingPicker {
                                editor: Box::new(editor),
                                query: String::new(),
                                selected: 0,
                            });
                        }
                        if editor.focused == editor.details_index() {
                            return Ok(Mode::Form(inspection_details_form(editor)));
                        }
                        if editor.focused == editor.save_index() {
                            return Ok(self.submit_inspection(editor));
                        }
                        if editor.focused == editor.cancel_index() {
                            return Ok(Mode::Browse);
                        }
                    }
                    _ => {}
                }
                Ok(Mode::Inspection(editor))
            }
            Mode::FindingPicker {
                editor,
                mut query,
                mut selected,
            } => {
                let count = self.inspection_finding_matches(&query).len();
                match key.code {
                    KeyCode::Esc => return Ok(Mode::Inspection(*editor)),
                    KeyCode::Up => selected = selected.saturating_sub(1),
                    KeyCode::Down => selected = (selected + 1).min(count.saturating_sub(1)),
                    KeyCode::PageUp => selected = selected.saturating_sub(10),
                    KeyCode::PageDown => selected = (selected + 10).min(count.saturating_sub(1)),
                    KeyCode::Home => selected = 0,
                    KeyCode::End => selected = count.saturating_sub(1),
                    KeyCode::Enter => {
                        if let Some(kind) = self
                            .inspection_finding_matches(&query)
                            .get(selected)
                            .copied()
                        {
                            return Ok(Mode::Form(inspection_finding_form(*editor, kind, None)));
                        }
                    }
                    KeyCode::Char('u') if ctrl => {
                        query.clear();
                        selected = 0;
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        selected = 0;
                    }
                    KeyCode::Char(character) if !ctrl => {
                        if query.len() < 1000 {
                            query.push(character);
                            selected = 0;
                        }
                    }
                    _ => {}
                }
                Ok(Mode::FindingPicker {
                    editor,
                    query,
                    selected,
                })
            }
            Mode::Reorder {
                mut order,
                mut selected,
                mut before_move,
            } => {
                let last = order.len().saturating_sub(1);
                match key.code {
                    KeyCode::Esc => {
                        if let Some(original) = before_move.take() {
                            let moved = order.get(selected).copied();
                            order = original;
                            selected = moved
                                .and_then(|id| order.iter().position(|candidate| *candidate == id))
                                .unwrap_or(selected.min(last));
                        } else {
                            return Ok(Mode::Browse);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let destination = selected.saturating_sub(1);
                        if before_move.is_some() {
                            order.swap(selected, destination);
                        }
                        selected = destination;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let destination = (selected + 1).min(last);
                        if before_move.is_some() {
                            order.swap(selected, destination);
                        }
                        selected = destination;
                    }
                    KeyCode::PageUp | KeyCode::Home => {
                        let destination = if key.code == KeyCode::Home {
                            0
                        } else {
                            selected.saturating_sub(10)
                        };
                        if before_move.is_some() && destination != selected {
                            let plant = order.remove(selected);
                            order.insert(destination, plant);
                        }
                        selected = destination;
                    }
                    KeyCode::PageDown | KeyCode::End => {
                        let destination = if key.code == KeyCode::End {
                            last
                        } else {
                            (selected + 10).min(last)
                        };
                        if before_move.is_some() && destination != selected {
                            let plant = order.remove(selected);
                            order.insert(destination, plant);
                        }
                        selected = destination;
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if before_move.is_none() {
                            before_move = Some(order.clone());
                        } else {
                            match self.db.reorder_plants(&order) {
                                Ok(()) => {
                                    self.grouping = Grouping::Manual;
                                    self.settings.manual_order = true;
                                    before_move = None;
                                    match self.reload() {
                                        Ok(()) => self.notify("Greenhouse order saved.", false),
                                        Err(error) => self.notify(
                                            format!(
                                                "Order saved, but the collection could not reload: {error:#}"
                                            ),
                                            true,
                                        ),
                                    }
                                }
                                Err(error) => {
                                    self.notify(format!("Could not save order: {error:#}"), true)
                                }
                            }
                        }
                    }
                    _ => {}
                }
                Ok(Mode::Reorder {
                    order,
                    selected,
                    before_move,
                })
            }
            Mode::Confirm {
                action,
                prompt,
                mut confirm,
            } => {
                if key.code == KeyCode::Char('y') || (key.code == KeyCode::Enter && confirm) {
                    match action {
                        ConfirmAction::DeletePlant(id) => {
                            self.db.remove_plant(id)?;
                            self.detail = false;
                            self.notify("Plant and its history removed.", false);
                        }
                        ConfirmAction::UndoEvent { plant, event } => {
                            self.db.remove_event(plant, event)?;
                            self.notify("Last-entered event removed; timers recalculated.", false);
                        }
                    }
                    self.reload()?;
                    Ok(Mode::Browse)
                } else if matches!(key.code, KeyCode::Esc | KeyCode::Char('n') | KeyCode::Enter) {
                    Ok(Mode::Browse)
                } else {
                    match key.code {
                        KeyCode::Left | KeyCode::Up => confirm = false,
                        KeyCode::Right | KeyCode::Down => confirm = true,
                        KeyCode::Tab | KeyCode::BackTab => confirm = !confirm,
                        _ => {}
                    }
                    Ok(Mode::Confirm {
                        action,
                        prompt,
                        confirm,
                    })
                }
            }
            Mode::Search { mut value } => {
                match key.code {
                    KeyCode::Esc => return Ok(Mode::Browse),
                    KeyCode::Enter => {
                        self.query = value;
                        self.refilter();
                        self.reload_history()?;
                        return Ok(Mode::Browse);
                    }
                    KeyCode::Char('u') if ctrl => value.clear(),
                    KeyCode::Backspace => {
                        value.pop();
                    }
                    KeyCode::Char(c) if !ctrl => {
                        if value.len() < 1000 {
                            value.push(c);
                        }
                    }
                    _ => {}
                }
                Ok(Mode::Search { value })
            }
            Mode::Catalog {
                mut query,
                mut selected,
                purpose,
                mut detail,
            } => {
                let match_count = self.catalog_matches(&query).len();
                if detail {
                    let last = match_count.saturating_sub(1);
                    match key.code {
                        KeyCode::Esc => detail = false,
                        KeyCode::Up | KeyCode::Left | KeyCode::Char('k' | 'h') => {
                            selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Right | KeyCode::Char('j' | 'l') => {
                            selected = (selected + 1).min(last);
                        }
                        KeyCode::PageUp => selected = selected.saturating_sub(10),
                        KeyCode::PageDown => selected = (selected + 10).min(last),
                        KeyCode::Home => selected = 0,
                        KeyCode::End => selected = last,
                        _ => {}
                    }
                    return Ok(Mode::Catalog {
                        query,
                        selected,
                        purpose,
                        detail,
                    });
                }
                let count = match_count + usize::from(purpose == CatalogPurpose::AddPlant);
                match key.code {
                    KeyCode::Esc => return Ok(Mode::Browse),
                    KeyCode::Char('n') if ctrl && purpose == CatalogPurpose::AddPlant => {
                        return Ok(Mode::Form(custom_form()));
                    }
                    KeyCode::Up => selected = selected.saturating_sub(1),
                    KeyCode::Down => selected = (selected + 1).min(count.saturating_sub(1)),
                    KeyCode::PageDown => selected = (selected + 10).min(count.saturating_sub(1)),
                    KeyCode::PageUp => selected = selected.saturating_sub(10),
                    KeyCode::Home => selected = 0,
                    KeyCode::End => selected = count.saturating_sub(1),
                    KeyCode::Enter => {
                        if purpose == CatalogPurpose::AddPlant {
                            if selected == match_count {
                                return Ok(Mode::Form(custom_form()));
                            }
                            if let Some(taxon) = self.catalog_matches(&query).get(selected) {
                                return Ok(Mode::Form(plant_form(
                                    FormAction::Add(Box::new((*taxon).clone())),
                                    None,
                                )));
                            }
                        } else if selected < match_count {
                            detail = true;
                        }
                    }
                    KeyCode::Char('u') if ctrl => {
                        query.clear();
                        selected = 0;
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        selected = 0;
                    }
                    KeyCode::Char(c) if !ctrl => {
                        if query.len() < 1000 {
                            query.push(c);
                            selected = 0;
                        }
                    }
                    _ => {}
                }
                Ok(Mode::Catalog {
                    query,
                    selected,
                    purpose,
                    detail,
                })
            }
            Mode::Form(mut form) => {
                let save = form.fields.len();
                let cancel = save + 1;
                match key.code {
                    KeyCode::Esc => {
                        return Ok(form.return_to.take().map_or(Mode::Browse, |mode| *mode));
                    }
                    KeyCode::Char('s') if ctrl => return Ok(self.submit_form(form)),
                    KeyCode::Enter if form.focused == save => return Ok(self.submit_form(form)),
                    KeyCode::Enter if form.focused == cancel => {
                        return Ok(form.return_to.take().map_or(Mode::Browse, |mode| *mode));
                    }
                    KeyCode::Tab | KeyCode::Down => {
                        form.focused = (form.focused + 1) % (cancel + 1);
                        form.replace_on_type = true;
                    }
                    KeyCode::Enter => {
                        form.focused += 1;
                        form.replace_on_type = true;
                    }
                    KeyCode::BackTab | KeyCode::Up => {
                        form.focused = (form.focused + cancel) % (cancel + 1);
                        form.replace_on_type = true;
                    }
                    KeyCode::Left | KeyCode::Right if form.focused >= save => {
                        form.focused = if key.code == KeyCode::Left {
                            save
                        } else {
                            cancel
                        };
                    }
                    KeyCode::Left | KeyCode::Right => {
                        let old_unit = (matches!(form.action, FormAction::EnvironmentSettings)
                            && form.focused == 2)
                            .then(|| TemperatureUnit::parse(&form.fields[2].value))
                            .flatten();
                        let field = &mut form.fields[form.focused];
                        let forward = key.code == KeyCode::Right;
                        if !field.choices.is_empty() {
                            let index = field
                                .choices
                                .iter()
                                .position(|choice| *choice == field.value)
                                .unwrap_or(0);
                            let count = field.choices.len();
                            field.value = field.choices
                                [(index + if forward { 1 } else { count - 1 }) % count]
                                .into();
                        } else if field.numeric {
                            let value = field.value.parse::<i64>().unwrap_or(0);
                            field.value = value
                                .saturating_add(if forward { 1 } else { -1 })
                                .clamp(0, 3650)
                                .to_string();
                        }
                        let new_unit = (matches!(form.action, FormAction::EnvironmentSettings)
                            && form.focused == 2)
                            .then(|| TemperatureUnit::parse(&form.fields[2].value))
                            .flatten();
                        if let (Some(old_unit), Some(new_unit)) = (old_unit, new_unit)
                            && old_unit != new_unit
                            && let Ok(value) = form.fields[3].value.parse::<f64>()
                        {
                            form.fields[3].value = format_number(
                                new_unit.convert_from_celsius(old_unit.to_celsius(value)),
                            );
                        }
                        form.replace_on_type = true;
                    }
                    _ if form.focused >= save => {}
                    KeyCode::Char('u') if ctrl => {
                        let field = &mut form.fields[form.focused];
                        if field.choices.is_empty() {
                            field.value.clear();
                        }
                    }
                    KeyCode::Backspace => {
                        let field = &mut form.fields[form.focused];
                        if field.choices.is_empty() {
                            if form.replace_on_type {
                                field.value.clear();
                            } else {
                                field.value.pop();
                            }
                            form.replace_on_type = false;
                        }
                    }
                    KeyCode::Char(c) if !ctrl => {
                        let field = &mut form.fields[form.focused];
                        if field.choices.is_empty() {
                            if form.replace_on_type {
                                field.value.clear();
                            }
                            if field.value.len() < 4000 {
                                field.value.push(c);
                            }
                            form.replace_on_type = false;
                        }
                    }
                    _ => {}
                }
                Ok(Mode::Form(form))
            }
            Mode::Browse => self.browse_key(key),
        }
    }

    fn browse_key(&mut self, key: KeyEvent) -> Result<Mode> {
        let len = self.visible.len();
        let mut next = self.selected;
        let navigating = matches!(
            key.code,
            KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Right
                | KeyCode::Left
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Char('h' | 'j' | 'k' | 'l')
        );
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc if self.detail => {
                self.detail = false;
                self.detail_scroll = 0;
            }
            KeyCode::Esc => {
                self.query.clear();
                self.due_only = false;
                self.refilter();
            }
            KeyCode::Char('a') => {
                return Ok(Mode::Catalog {
                    query: String::new(),
                    selected: 0,
                    purpose: CatalogPurpose::AddPlant,
                    detail: false,
                });
            }
            KeyCode::Char('c') => {
                return Ok(Mode::Catalog {
                    query: String::new(),
                    selected: 0,
                    purpose: CatalogPurpose::View,
                    detail: false,
                });
            }
            KeyCode::Char('/') => {
                return Ok(Mode::Search {
                    value: self.query.clone(),
                });
            }
            KeyCode::Char('o') => {
                if !self.plants.is_empty() {
                    let selected_id = self.selected_plant().map(|plant| plant.id);
                    let order = self.plants_in_view_order();
                    let selected = selected_id
                        .and_then(|id| order.iter().position(|candidate| *candidate == id))
                        .unwrap_or(0);
                    return Ok(Mode::Reorder {
                        order,
                        selected,
                        before_move: None,
                    });
                }
            }
            KeyCode::Char('d') => {
                self.due_only = !self.due_only;
                self.refilter();
                self.reload_history()?;
            }
            KeyCode::Char('g') => {
                let grouping = self.grouping.next();
                let mut settings = self.settings;
                settings.manual_order = grouping == Grouping::Manual;
                self.db.save_settings(settings)?;
                self.settings = settings;
                self.grouping = grouping;
                self.refilter();
            }
            KeyCode::Char('R') => {
                self.reload()?;
                self.notify("Reloaded from SQLite.", false);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if len > 0 {
                    self.detail = !self.detail;
                    self.detail_scroll = 0;
                    self.reload_history()?;
                }
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                if len > 0 {
                    next = (next + 1) % len;
                }
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                if len > 0 {
                    next = (next + len - 1) % len;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                next =
                    (next + if self.detail { 1 } else { self.columns }).min(len.saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') => {
                next = next.saturating_sub(if self.detail { 1 } else { self.columns })
            }
            KeyCode::Home => next = 0,
            KeyCode::End => next = len.saturating_sub(1),
            KeyCode::PageDown if self.detail => {
                self.detail_scroll = self.detail_scroll.saturating_add(5)
            }
            KeyCode::PageUp if self.detail => {
                self.detail_scroll = self.detail_scroll.saturating_sub(5)
            }
            KeyCode::PageDown => next = (next + self.page_size).min(len.saturating_sub(1)),
            KeyCode::PageUp => next = next.saturating_sub(self.page_size),
            KeyCode::Char('w') => self.quick_log(TaskKind::Water)?,
            KeyCode::Char('r') => self.quick_log(TaskKind::Rotate)?,
            KeyCode::Char('f') => self.quick_log(TaskKind::Fertilize)?,
            KeyCode::Char('i') => {
                if let Some(plant) = self.selected_plant() {
                    return Ok(Mode::Inspection(InspectionEditor::new(
                        plant.id,
                        plant.name().to_string(),
                    )));
                }
            }
            KeyCode::Char('m') | KeyCode::Char('W') => {
                if let Some(p) = self.selected_plant() {
                    return Ok(Mode::Form(Form::new(
                        format!("Maintenance · {}", p.name()),
                        "Record care, optionally with a backdate and a note.",
                        FormAction::Log {
                            plant: p.id,
                            kind: TaskKind::Water,
                        },
                        vec![
                            ("Task", "water".into()),
                            ("When", "now".into()),
                            ("Note (optional)", String::new()),
                        ],
                    )));
                }
            }
            KeyCode::Char('E') => {
                if let Some(p) = self.selected_plant() {
                    return Ok(Mode::Form(plant_environment_form(
                        p,
                        self.settings.home_environment.temperature_unit,
                    )));
                }
            }
            KeyCode::Char('v') => {
                if let Some(p) = self.selected_plant() {
                    return Ok(Mode::Form(environment_log_form(
                        p.id,
                        p.name(),
                        self.settings.home_environment.temperature_unit,
                    )));
                }
            }
            KeyCode::Char('e') => {
                if let Some(p) = self.selected_plant() {
                    return Ok(Mode::Form(plant_form(FormAction::Edit(p.id), Some(p))));
                }
            }
            KeyCode::Char('t') => {
                if let Some(p) = self.selected_plant() {
                    return Ok(Mode::Form(Form::new(
                        format!("Care intervals · {}", p.name()),
                        "Days between reminders; 0 disables a task. These are check reminders, not automatic care instructions.",
                        FormAction::Schedules(p.id),
                        p.schedules
                            .iter()
                            .map(|s| (s.kind.label(), s.interval_days.to_string()))
                            .collect(),
                    )));
                }
            }
            KeyCode::Char('D') => {
                if let Some(p) = self.selected_plant() {
                    return Ok(Mode::Confirm {
                        action: ConfirmAction::DeletePlant(p.id),
                        confirm: false,
                        prompt: format!(
                            "Permanently remove {} and ALL its care history?",
                            p.name()
                        ),
                    });
                }
            }
            KeyCode::Char('u') => {
                if let Some(p) = self.selected_plant() {
                    let id = p.id;
                    if let Some(event) = self.db.history(id)?.into_iter().max_by_key(|e| e.id) {
                        return Ok(Mode::Confirm {
                            action: ConfirmAction::UndoEvent {
                                plant: id,
                                event: event.id,
                            },
                            confirm: false,
                            prompt: format!(
                                "Undo last-entered {} event from {}? Timers will be recalculated.",
                                event.kind.key(),
                                model::date(event.occurred_at)
                            ),
                        });
                    } else {
                        self.notify("No maintenance to undo for this plant.", false);
                    }
                }
            }
            _ => {}
        }
        if navigating && next != self.selected {
            self.selected = next.min(self.visible.len().saturating_sub(1));
            self.detail_scroll = 0;
            self.reload_history()?;
        }
        Ok(Mode::Browse)
    }

    fn plants_in_view_order(&self) -> Vec<i64> {
        let mut plants = self.plants.iter().collect::<Vec<_>>();
        plants.sort_by(|a, b| self.grouping.compare(a, b));
        plants.into_iter().map(|plant| plant.id).collect()
    }

    fn quick_log(&mut self, kind: TaskKind) -> Result<()> {
        if let Some(p) = self.selected_plant() {
            let message = format!("Recorded {} for {}.", kind.key(), p.name());
            self.db.log(p.id, kind, self.now, "", self.now)?;
            self.reload()?;
            self.notify(message, false);
        }
        Ok(())
    }

    fn submit_form(&mut self, mut form: Form) -> Mode {
        match self.save_form(&mut form) {
            Ok(mode) => mode,
            Err(error) => {
                self.notify(format!("{error:#}"), true);
                Mode::Form(form)
            }
        }
    }

    fn submit_inspection(&mut self, editor: InspectionEditor) -> Mode {
        let when = match model::parse_when(&editor.draft.when, self.now) {
            Ok(when) => when,
            Err(error) => {
                self.notify(format!("{error:#}"), true);
                return Mode::Inspection(editor);
            }
        };
        if let Err(error) = self.db.log_inspection(
            editor.draft.plant,
            when,
            &editor.draft.note,
            &editor.draft.findings,
            self.now,
        ) {
            self.notify(format!("{error:#}"), true);
            return Mode::Inspection(editor);
        }
        if let Err(error) = self.reload() {
            self.notify(
                format!("Inspection saved, but the collection could not reload: {error:#}"),
                true,
            );
        } else {
            self.notify(
                format!(
                    "Recorded inspection with {} structured finding{}.",
                    editor.draft.findings.len(),
                    if editor.draft.findings.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
                false,
            );
        }
        Mode::Browse
    }

    pub fn paste(&mut self, text: &str) {
        let clean: String = text
            .chars()
            .filter(|c| !c.is_control())
            .take(4000)
            .collect();
        match &mut self.mode {
            Mode::Form(form) => {
                if let Some(field) = form.fields.get_mut(form.focused)
                    && field.choices.is_empty()
                {
                    if form.replace_on_type {
                        field.value.clear();
                    }
                    field.value.extend(
                        clean
                            .chars()
                            .take(4000usize.saturating_sub(field.value.len())),
                    );
                    form.replace_on_type = false;
                }
            }
            Mode::Catalog {
                query,
                selected,
                detail,
                ..
            } => {
                if !*detail {
                    query.extend(clean.chars().take(1000usize.saturating_sub(query.len())));
                    *selected = 0;
                }
            }
            Mode::Search { value } => {
                value.extend(clean.chars().take(1000usize.saturating_sub(value.len())))
            }
            Mode::FindingPicker {
                query, selected, ..
            } => {
                query.extend(clean.chars().take(1000usize.saturating_sub(query.len())));
                *selected = 0;
            }
            _ => {}
        }
    }

    fn save_form(&mut self, form: &mut Form) -> Result<Mode> {
        let v = form.values();
        match &form.action {
            FormAction::Add(taxon) => {
                let id = self.db.add_plant(taxon, &input(&v), self.now)?;
                self.query.clear();
                self.due_only = false;
                self.reload()?;
                self.selected = self
                    .visible
                    .iter()
                    .position(|i| self.plants[*i].id == id)
                    .unwrap_or(0);
                self.detail = true;
                self.detail_scroll = 0;
                self.reload_history()?;
                self.notify(
                    "Plant added. Hydration stays unknown until a watering is logged.",
                    false,
                );
            }
            FormAction::Edit(id) => {
                self.db.edit_plant(*id, &input(&v))?;
                self.reload()?;
                self.notify("Plant details saved.", false);
            }
            FormAction::Schedules(id) => {
                let days = v
                    .iter()
                    .map(|s| {
                        s.parse::<i64>()
                            .context("Intervals must be whole numbers of days")
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.db.set_intervals(*id, &days, self.now)?;
                self.reload()?;
                self.notify("Care intervals saved.", false);
            }
            FormAction::Log { plant, kind } => {
                let kind = if v[0].is_empty() {
                    *kind
                } else {
                    TaskKind::parse(&v[0].to_lowercase())?
                };
                let when = model::parse_when(&v[1], self.now)?;
                if kind == TaskKind::Inspect {
                    let plant_name = self
                        .plants
                        .iter()
                        .find(|candidate| candidate.id == *plant)
                        .map(|candidate| candidate.name().to_string())
                        .unwrap_or_else(|| format!("Plant {plant}"));
                    let mut editor = InspectionEditor::new(*plant, plant_name);
                    editor.draft.when = v[1].clone();
                    editor.draft.note = v[2].clone();
                    return Ok(Mode::Inspection(editor));
                }
                self.db.log(*plant, kind, when, &v[2], self.now)?;
                self.reload()?;
                self.notify(format!("Recorded {} · {}. Timers use the latest actual completion, not entry order.", kind.key(), model::date(when)), false);
            }
            FormAction::EnvironmentSettings => {
                let unit = match v[2].as_str() {
                    "not set" | "" => None,
                    value => Some(
                        TemperatureUnit::parse(value)
                            .with_context(|| format!("Unknown temperature unit: {value}"))?,
                    ),
                };
                let displayed_temperature = optional_number(&v[3], "Baseline temperature")?;
                let baseline_temperature_c = match (displayed_temperature, unit) {
                    (Some(value), Some(unit)) => Some(unit.to_celsius(value)),
                    (Some(_), None) => {
                        bail!("Choose a temperature unit or clear the baseline temperature")
                    }
                    (None, _) => None,
                };
                let mut settings = self.settings;
                settings.environment_assessments_enabled = match v[0].as_str() {
                    "on" => true,
                    "off" => false,
                    _ => bail!("Estimated assessments must be on or off"),
                };
                settings.home_environment = HomeEnvironment {
                    latitude: optional_number(&v[1], "Approximate latitude")?,
                    temperature_unit: unit,
                    baseline_temperature_c,
                    baseline_humidity_percent: optional_number(&v[4], "Baseline humidity")?,
                };
                self.db.save_settings(settings)?;
                self.settings = settings;
                self.reload()?;
                self.notify("Environment setup saved locally.", false);
            }
            FormAction::PlantEnvironment(id) => {
                let exposure = match v[0].as_str() {
                    "not set" | "" => None,
                    value => Some(
                        SunExposure::parse(value)
                            .with_context(|| format!("Unknown sun exposure: {value}"))?,
                    ),
                };
                let displayed_temperature = optional_number(&v[1], "Temperature override")?;
                let temperature_c = match (
                    displayed_temperature,
                    self.settings.home_environment.temperature_unit,
                ) {
                    (Some(value), Some(unit)) => Some(unit.to_celsius(value)),
                    (Some(_), None) => {
                        bail!("Choose a preferred temperature unit in Environment setup first")
                    }
                    (None, _) => None,
                };
                self.db.set_plant_environment(
                    *id,
                    PlantEnvironment {
                        exposure,
                        temperature_c,
                        humidity_percent: optional_number(&v[2], "Humidity override")?,
                    },
                )?;
                self.reload()?;
                self.notify("Plant environment saved.", false);
            }
            FormAction::LogEnvironment(plant) => {
                let scope = EnvironmentScope::parse(&v[0])
                    .with_context(|| format!("Unknown environment scope: {}", v[0]))?;
                let observed_at = model::parse_when(&v[1], self.now)?;
                let displayed_temperature = optional_number(&v[2], "Temperature")?;
                let temperature_c = match (
                    displayed_temperature,
                    self.settings.home_environment.temperature_unit,
                ) {
                    (Some(value), Some(unit)) => Some(unit.to_celsius(value)),
                    (Some(_), None) => {
                        bail!("Choose a preferred temperature unit in Environment setup first")
                    }
                    (None, _) => None,
                };
                self.db.log_environment(
                    EnvironmentReadingInput {
                        plant_id: (scope == EnvironmentScope::Plant).then_some(*plant),
                        scope,
                        source: EnvironmentSource::Manual,
                        observed_at,
                        temperature_c,
                        humidity_percent: optional_number(&v[3], "Humidity")?,
                        lux: optional_number(&v[4], "Lux")?,
                        ppfd: optional_number(&v[5], "PPFD")?,
                    },
                    self.now,
                )?;
                self.reload()?;
                self.notify("Environment snapshot recorded.", false);
            }
            FormAction::InspectionDetails(editor) => {
                model::parse_when(&v[0], self.now)?;
                let mut editor = editor.as_ref().clone();
                editor.draft.when = v[0].clone();
                editor.draft.note = v[1].clone();
                editor.focused = editor.details_index();
                return Ok(Mode::Inspection(editor));
            }
            FormAction::InspectionFinding {
                editor,
                kind,
                index,
            } => {
                let finding = InspectionFindingInput {
                    kind: *kind,
                    part: parse_optional_choice(&v[0], PlantPart::parse)?,
                    severity: parse_optional_choice(&v[1], InspectionSeverity::parse)?,
                    extent: parse_optional_choice(&v[2], InspectionExtent::parse)?,
                    trend: InspectionTrend::parse(&v[3])?,
                    pest: parse_optional_choice(&v[4], PestKind::parse)?,
                    note: v[5].clone(),
                };
                let mut editor = editor.as_ref().clone();
                if let Some(index) = index {
                    editor.draft.findings[*index] = finding;
                    editor.focused = *index;
                } else {
                    if kind.is_concern() {
                        editor.draft.findings.retain(|existing| {
                            existing.kind != InspectionFindingKind::NoAdverseSigns
                        });
                    }
                    editor.draft.findings.push(finding);
                    editor.focused = editor.draft.findings.len() - 1;
                }
                if editor
                    .draft
                    .findings
                    .iter()
                    .any(|finding| finding.kind == InspectionFindingKind::NoAdverseSigns)
                    && editor
                        .draft
                        .findings
                        .iter()
                        .any(|finding| finding.kind.is_concern())
                {
                    bail!("No adverse signs cannot be combined with a concern");
                }
                return Ok(Mode::Inspection(editor));
            }
            FormAction::Custom => {
                if !crate::art::HABITS.contains(&v[6].as_str()) {
                    bail!("Habit must be: {}", crate::art::HABITS.join(", "));
                }
                let taxon = Taxon {
                    id: Taxon::botanical(&v[0], &v[1], &v[2]),
                    genus: v[0].clone(),
                    species: v[1].clone(),
                    designation: v[2].clone(),
                    common: v[3].clone(),
                    family: v[4].clone(),
                    groups: v[5].clone(),
                    habit: v[6].clone(),
                    water_days: v[7]
                        .parse()
                        .context("Water check interval must be whole days")?,
                    light: v[8].clone(),
                    care: v[9].clone(),
                };
                self.db.add_taxon(&taxon)?;
                self.taxa = self.db.taxa()?;
                self.notify("Custom taxon saved; now add your individual plant.", false);
                return Ok(Mode::Form(plant_form(
                    FormAction::Add(Box::new(taxon)),
                    None,
                )));
            }
        }
        Ok(form.return_to.take().map_or(Mode::Browse, |mode| *mode))
    }
}

fn parse_optional_choice<T>(value: &str, parse: fn(&str) -> Result<T>) -> Result<Option<T>> {
    match value {
        "" | "not_set" | "not set" => Ok(None),
        value => parse(value).map(Some),
    }
}

fn default_inspection_finding(kind: InspectionFindingKind) -> InspectionFindingInput {
    use InspectionFindingKind::*;
    let part = match kind {
        NewLeaf
        | LeafYellowing
        | LeafBrowning
        | LeafBlackening
        | LeafPalingOrBleaching
        | LeafSpotsOrLesions
        | LeafTipBurn
        | LeafEdgeBurn
        | LeafScorch
        | LeafWiltingOrDrooping
        | LeafCurlingOrCupping
        | LeafShrivelingOrWrinkling
        | LeafDrop
        | StuckUnfurling
        | EdemaOrBlistering => Some(PlantPart::Leaf),
        NewShoot | NewBranch | DistortedGrowth | StalledGrowth | Etiolation
        | WeakOrFloppyGrowth => Some(PlantPart::NewGrowth),
        StemShriveling | StemSoftening | StemLesionOrDiscoloration | StemBreakage => {
            Some(PlantPart::Stem)
        }
        CrownSofteningOrCollapse => Some(PlantPart::Crown),
        NewRoot | RootBoundOrCircling | RootsExitingPot | RootDryOrBrittle | RootDarkOrMushy
        | RootLoss | RootDamage | AbnormalRootOdor => Some(PlantPart::Root),
        Budding | Flowering | BudDrop | FlowerDrop | BloomDamage => Some(PlantPart::Flower),
        Fruiting | FruitDamageOrRot => Some(PlantPart::Fruit),
        SubstrateDry | SubstrateSaturated | HydrophobicSubstrate | CompactedSubstrate
        | MoldOrAlgae | FungalFruitingBody | SaltOrMineralBuildup | StandingWater
        | PoorDrainage => Some(PlantPart::Substrate),
        DamagedContainer => Some(PlantPart::Container),
        NoAdverseSigns
        | OffsetOrPup
        | Recovery
        | PhysicalDamage
        | Dieback
        | LeaningOrInstability
        | PestObserved
        | EggsOrLarvae
        | Webbing
        | CottonyResidue
        | ScaleLikeBumps
        | StipplingOrSilvering
        | ChewingOrMining
        | HoneydewOrStickiness
        | Frass
        | PowderyGrowth
        | FuzzyMold
        | WaterSoakedTissue
        | RustLikePustules
        | Ooze
        | FoulOdor
        | SupportOrStakingIssue => None,
    };
    InspectionFindingInput {
        kind,
        part,
        severity: None,
        extent: None,
        trend: InspectionTrend::New,
        pest: (kind == InspectionFindingKind::PestObserved).then_some(PestKind::Unknown),
        note: String::new(),
    }
}

fn inspection_details_form(editor: InspectionEditor) -> Form {
    let previous = Mode::Inspection(editor.clone());
    let fields = vec![
        ("When", editor.draft.when.clone()),
        ("Overall note (optional)", editor.draft.note.clone()),
    ];
    Form::new(
        format!("Inspection details · {}", editor.draft.plant_name),
        "Set the observation time and an optional note applying to the whole inspection.",
        FormAction::InspectionDetails(Box::new(editor)),
        fields,
    )
    .returning_to(previous)
}

fn inspection_finding_form(
    editor: InspectionEditor,
    kind: InspectionFindingKind,
    index: Option<usize>,
) -> Form {
    let finding = index
        .and_then(|index| editor.draft.findings.get(index).cloned())
        .unwrap_or_else(|| default_inspection_finding(kind));
    let previous = Mode::Inspection(editor.clone());
    let title = format!(
        "{} finding · {}",
        if index.is_some() { "Edit" } else { "Add" },
        kind.label()
    );
    Form::new(
        title,
        "Classify only what was observed. Cause and treatment can remain unknown.",
        FormAction::InspectionFinding {
            editor: Box::new(editor),
            kind,
            index,
        },
        vec![
            (
                "Plant part (optional)",
                finding.part.map(PlantPart::key).unwrap_or("not_set").into(),
            ),
            (
                "Severity (optional)",
                finding
                    .severity
                    .map(InspectionSeverity::key)
                    .unwrap_or("not_set")
                    .into(),
            ),
            (
                "Extent (optional)",
                finding
                    .extent
                    .map(InspectionExtent::key)
                    .unwrap_or("not_set")
                    .into(),
            ),
            ("Trend", finding.trend.key().into()),
            (
                "Pest identity (optional)",
                finding.pest.map(PestKind::key).unwrap_or("not_set").into(),
            ),
            ("Finding note (optional)", finding.note),
        ],
    )
    .returning_to(previous)
}

fn input(values: &[String]) -> PlantInput {
    PlantInput {
        nickname: values[0].clone(),
        location: values[1].clone(),
        acquired: values[2].clone(),
        pot: values[3].clone(),
        soil: values[4].clone(),
        tags: values[5].clone(),
        notes: values[6].clone(),
    }
}
fn plant_form(action: FormAction, plant: Option<&Plant>) -> Form {
    let title = match &action {
        FormAction::Add(t) => format!("Add · {}", t.id),
        _ => "Edit plant".into(),
    };
    Form::new(
        title,
        "Nickname and all other fields are optional.",
        action,
        vec![
            (
                "Nickname",
                plant.map_or(String::new(), |p| p.nickname.clone()),
            ),
            (
                "Location / light exposure",
                plant.map_or(String::new(), |p| p.location.clone()),
            ),
            (
                "Acquired (YYYY-MM-DD)",
                plant.map_or(String::new(), |p| p.acquired.clone()),
            ),
            (
                "Pot / size / drainage",
                plant.map_or(String::new(), |p| p.pot.clone()),
            ),
            (
                "Soil / substrate",
                plant.map_or(String::new(), |p| p.soil.clone()),
            ),
            (
                "Personal groups / tags",
                plant.map_or(String::new(), |p| p.tags.clone()),
            ),
            ("Notes", plant.map_or(String::new(), |p| p.notes.clone())),
        ],
    )
}
fn environment_settings_form(settings: Settings) -> Form {
    let environment = settings.home_environment;
    let temperature = match (
        environment.baseline_temperature_c,
        environment.temperature_unit,
    ) {
        (Some(value), Some(unit)) => format_number(unit.convert_from_celsius(value)),
        _ => String::new(),
    };
    Form::new(
        "Environment setup",
        "Optional and local only. Latitude may be rounded; it drives a rough seasonal daylight estimate, never weather, window transmission, or measured plant light.",
        FormAction::EnvironmentSettings,
        vec![
            (
                "Estimated assessments",
                if settings.environment_assessments_enabled {
                    "on"
                } else {
                    "off"
                }
                .into(),
            ),
            (
                "Approximate signed latitude",
                environment.latitude.map(format_number).unwrap_or_default(),
            ),
            (
                "Preferred temperature unit",
                environment
                    .temperature_unit
                    .map(TemperatureUnit::key)
                    .unwrap_or("not set")
                    .into(),
            ),
            ("Baseline indoor temperature", temperature),
            (
                "Baseline humidity (%)",
                environment
                    .baseline_humidity_percent
                    .map(format_number)
                    .unwrap_or_default(),
            ),
        ],
    )
}

fn plant_environment_form(plant: &Plant, unit: Option<TemperatureUnit>) -> Form {
    let temperature = match (plant.environment.temperature_c, unit) {
        (Some(value), Some(unit)) => format_number(unit.convert_from_celsius(value)),
        _ => String::new(),
    };
    let hint = format!(
        "Explicit optional inputs only; free-text location is not evidence. Temperature uses {}. Ctrl-U clears text fields.",
        unit.map(TemperatureUnit::symbol)
            .unwrap_or("the unit chosen in Environment setup")
    );
    Form::new(
        format!("Plant environment · {}", plant.name()),
        &hint,
        FormAction::PlantEnvironment(plant.id),
        vec![
            (
                "Sun exposure (rough five-level rating)",
                plant
                    .environment
                    .exposure
                    .map(SunExposure::key)
                    .unwrap_or("not set")
                    .into(),
            ),
            ("Room temperature override", temperature),
            (
                "Room humidity override (%)",
                plant
                    .environment
                    .humidity_percent
                    .map(format_number)
                    .unwrap_or_default(),
            ),
        ],
    )
}

fn environment_log_form(plant: i64, name: &str, unit: Option<TemperatureUnit>) -> Form {
    let hint = format!(
        "Manual snapshots may be backdated. Enter at least one metric. Temperature uses {}; lux is illuminance and PPFD remains a snapshot, not DLI.",
        unit.map(TemperatureUnit::symbol)
            .unwrap_or("the unit chosen in Environment setup")
    );
    Form::new(
        format!("Log environment · {name}"),
        &hint,
        FormAction::LogEnvironment(plant),
        vec![
            ("Scope", "plant".into()),
            ("When", "now".into()),
            ("Temperature (optional)", String::new()),
            ("Humidity % (optional)", String::new()),
            ("Illuminance lux (optional)", String::new()),
            ("PPFD µmol/m²/s (optional)", String::new()),
        ],
    )
}

fn optional_number(value: &str, label: &str) -> Result<Option<f64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let number = value
        .trim()
        .parse::<f64>()
        .with_context(|| format!("{label} must be a number or blank"))?;
    if !number.is_finite() {
        bail!("{label} must be finite");
    }
    Ok(Some(number))
}

fn format_number(value: f64) -> String {
    let formatted = format!("{value:.1}");
    formatted
        .strip_suffix(".0")
        .unwrap_or(&formatted)
        .to_string()
}

fn custom_form() -> Form {
    Form::new(
        "Custom species / cultivar / hybrid",
        "Use genus + species as the identity. For unknown hybrids, leave species blank and add a designation (e.g. 'Birkin').",
        FormAction::Custom,
        vec![
            ("Genus (required)", String::new()),
            ("Species / hybrid epithet", String::new()),
            ("Cultivar / hybrid designation", String::new()),
            ("Common names (required)", String::new()),
            ("Family", String::new()),
            ("Care groups (comma separated)", String::new()),
            ("Growth habit", "broadleaf".into()),
            ("Water check interval (days)", "10".into()),
            ("Light preference", "Bright indirect".into()),
            ("Care notes", String::new()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, code: KeyCode) {
        app.key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn notifications_expire_after_five_seconds() -> Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        app.notify("Recorded water for Beauty Star.", true);
        assert_eq!(app.toast, "Recorded water for Beauty Star.");
        assert!(app.error);

        app.toast_expires_at = Some(Instant::now() - Duration::from_millis(1));
        app.tick();

        assert!(app.toast.is_empty());
        assert!(!app.error);
        assert!(app.toast_expires_at.is_none());
        Ok(())
    }

    #[test]
    fn startup_selects_the_first_plant_after_restoring_manual_order() -> Result<()> {
        let mut db = Database::memory()?;
        let taxon = db.taxa()?.remove(0);
        let first = db.add_plant(
            &taxon,
            &PlantInput {
                nickname: "Zebra".into(),
                ..Default::default()
            },
            model::now(),
        )?;
        let last = db.add_plant(
            &taxon,
            &PlantInput {
                nickname: "Alpha".into(),
                ..Default::default()
            },
            model::now(),
        )?;
        db.reorder_plants(&[first, last])?;

        let app = App::new(db)?;

        assert_eq!(app.grouping, Grouping::Manual);
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_plant().unwrap().name(), "Zebra");
        Ok(())
    }

    #[test]
    fn regrouping_preserves_the_selected_specimen_not_its_old_index() -> Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        press(&mut app, KeyCode::End);
        let id = app.selected_plant().unwrap().id;
        for _ in 0..5 {
            press(&mut app, KeyCode::Char('g'));
            assert_eq!(app.selected_plant().unwrap().id, id);
        }
        Ok(())
    }

    #[test]
    fn reorder_mode_cancels_a_pickup_or_saves_manual_order_on_drop() -> Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        press(&mut app, KeyCode::Char('o'));
        let initial = match &app.mode {
            Mode::Reorder { order, .. } => order.clone(),
            _ => panic!("reorder list did not open"),
        };

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Esc);
        assert!(
            matches!(&app.mode, Mode::Reorder { order, before_move: None, .. } if order == &initial)
        );

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        let mut expected = initial;
        expected.swap(0, 1);
        assert_eq!(app.grouping, Grouping::Manual);
        assert!(app.settings.manual_order);
        assert!(matches!(
            &app.mode,
            Mode::Reorder {
                order,
                before_move: None,
                ..
            } if order == &expected
        ));
        assert_eq!(
            app.visible
                .iter()
                .map(|index| app.plants[*index].id)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            app.db
                .plants()?
                .into_iter()
                .map(|plant| plant.id)
                .collect::<Vec<_>>(),
            expected
        );
        Ok(())
    }

    #[test]
    fn logging_in_due_only_view_removes_completed_plant_without_stale_indexes() -> Result<()> {
        let mut db = Database::memory()?;
        let taxon = db.taxa()?.remove(0);
        db.add_plant(
            &taxon,
            &PlantInput {
                nickname: "A".into(),
                ..Default::default()
            },
            model::now(),
        )?;
        db.add_plant(
            &taxon,
            &PlantInput {
                nickname: "B".into(),
                ..Default::default()
            },
            model::now(),
        )?;
        let mut app = App::new(db)?;
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Char('w'));
        assert_eq!(app.visible.len(), 1);
        assert_eq!(app.selected_plant().unwrap().name(), "A");
        assert!(app.history.is_empty());
        press(&mut app, KeyCode::Char('w'));
        assert!(app.visible.is_empty());
        app.tick();
        press(&mut app, KeyCode::Char('w'));
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.visible.len(), 2);
        Ok(())
    }

    #[test]
    fn invalid_backdate_preserves_form_and_does_not_write_an_event() -> Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        let count = app.history.len();
        press(&mut app, KeyCode::Char('m'));
        if let Mode::Form(form) = &mut app.mode {
            form.fields[1].value = "-2d".into();
        }
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(app.mode, Mode::Form(_)));
        assert!(app.error);
        assert_eq!(app.history.len(), count);
        if let Mode::Form(form) = &mut app.mode {
            form.fields[1].value = "2d".into();
        }
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(app.mode, Mode::Browse));
        assert!(!app.error);
        assert_eq!(app.history.len(), count + 1);
        Ok(())
    }

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            press(app, KeyCode::Char(character));
        }
    }

    #[test]
    fn catalog_key_opens_a_read_only_browser() -> Result<()> {
        let mut app = App::new(Database::memory()?)?;
        press(&mut app, KeyCode::Char('c'));
        type_text(&mut app, "hoya");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::Catalog {
                query,
                purpose: CatalogPurpose::View,
                detail: true,
                ..
            } if query == "hoya"
        ));
        app.paste(" ignored");
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Esc);
        assert!(matches!(
            &app.mode,
            Mode::Catalog {
                query,
                selected: 1,
                detail: false,
                ..
            } if query == "hoya"
        ));
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Browse));
        Ok(())
    }

    #[test]
    fn inspection_editor_saves_multiple_structured_fields() -> Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        press(&mut app, KeyCode::Char('i'));
        assert!(matches!(
            &app.mode,
            Mode::Inspection(editor)
                if editor.draft.findings[0].kind == InspectionFindingKind::NoAdverseSigns
        ));

        press(&mut app, KeyCode::Char('a'));
        type_text(&mut app, "leaf browning");
        press(&mut app, KeyCode::Enter);
        let Mode::Form(form) = &mut app.mode else {
            panic!("finding form did not open");
        };
        form.fields[1].value = "moderate".into();
        form.fields[2].value = "localized".into();
        form.fields[5].value = "oldest leaf".into();
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let Mode::Inspection(editor) = &app.mode else {
            panic!("inspection editor did not return");
        };
        assert_eq!(editor.draft.findings.len(), 1);
        assert_eq!(
            editor.draft.findings[0].kind,
            InspectionFindingKind::LeafBrowning
        );

        press(&mut app, KeyCode::Char('e'));
        let Mode::Form(form) = &mut app.mode else {
            panic!("inspection details did not open");
        };
        form.fields[0].value = "yesterday".into();
        form.fields[1].value = "weekly check".into();
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

        assert!(matches!(app.mode, Mode::Browse));
        let inspection = app
            .history
            .iter()
            .find(|event| event.kind == TaskKind::Inspect)
            .expect("inspection history");
        assert_eq!(inspection.note, "weekly check");
        assert_eq!(inspection.findings.len(), 1);
        assert_eq!(
            inspection.findings[0].severity,
            Some(InspectionSeverity::Moderate)
        );
        assert_eq!(
            inspection.findings[0].extent,
            Some(InspectionExtent::Localized)
        );
        assert_eq!(inspection.findings[0].note, "oldest leaf");
        Ok(())
    }

    #[test]
    fn adding_a_plant_needs_only_typing_arrows_and_enter() -> Result<()> {
        let mut app = App::new(Database::memory()?)?;
        // The first command in greenhouse help is Add; no mnemonic is needed.
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Enter);
        type_text(&mut app, "Thai Constellation");
        press(&mut app, KeyCode::Enter);
        type_text(&mut app, "Queen Mona");
        press(&mut app, KeyCode::Down);
        type_text(&mut app, "East window");
        for _ in 1..7 {
            press(&mut app, KeyCode::Enter);
        }
        app.paste("ignored while Save is focused");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode, Mode::Browse));
        let plant = app.selected_plant().unwrap();
        assert_eq!(plant.nickname, "Queen Mona");
        assert_eq!(plant.location, "East window");
        assert_eq!(app.plants.len(), 1);
        assert!(!app.quit);
        assert!(!app.error);
        Ok(())
    }

    #[test]
    fn help_preserves_catalog_form_and_confirmation_state() -> Result<()> {
        let mut app = App::new(crate::demo_database()?)?;
        press(&mut app, KeyCode::Char('a'));
        type_text(&mut app, "hoya");
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('?'));
        assert!(matches!(
            &app.mode,
            Mode::Catalog {
                query,
                selected: 1,
                ..
            } if query == "hoya"
        ));
        press(&mut app, KeyCode::Enter);
        type_text(&mut app, "quiet Hoya");
        press(&mut app, KeyCode::Down);
        type_text(&mut app, "Shelf");
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Esc);
        let Mode::Form(form) = &app.mode else {
            panic!("lost the form")
        };
        assert_eq!(form.focused, 1);
        assert_eq!(form.fields[0].value, "quiet Hoya");
        assert_eq!(form.fields[1].value, "Shelf");
        press(&mut app, KeyCode::Esc);
        let id = app.selected_plant().unwrap().id;
        press(&mut app, KeyCode::Char('D'));
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Confirm { confirm: true, .. }));
        press(&mut app, KeyCode::Left);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.selected_plant().unwrap().id, id);
        assert!(!app.quit);
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
        Ok(())
    }

    #[test]
    fn custom_types_and_backdated_care_are_arrow_selectable() -> Result<()> {
        let mut app = App::new(Database::memory()?)?;
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Enter);
        for value in ["Hoya", "testii", "", "Test hoya", "Apocynaceae", "hoyas"] {
            type_text(&mut app, value);
            press(&mut app, KeyCode::Enter);
        }
        press(&mut app, KeyCode::Right); // broadleaf -> vine
        press(&mut app, KeyCode::Enter);
        type_text(&mut app, "12"); // replaces the prefilled 10
        for _ in 0..4 {
            press(&mut app, KeyCode::Enter);
        }
        assert!(matches!(&app.mode, Mode::Form(form) if matches!(form.action, FormAction::Add(_))));
        for _ in 0..8 {
            press(&mut app, KeyCode::Enter);
        }
        assert_eq!(app.selected_plant().unwrap().taxon.habit, "vine");
        assert_eq!(app.selected_plant().unwrap().water().interval_days, 12);
        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Right); // water -> rotate
        press(&mut app, KeyCode::Enter);
        type_text(&mut app, "2d"); // replaces now
        for _ in 0..3 {
            press(&mut app, KeyCode::Enter);
        }
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].kind, TaskKind::Rotate);
        assert_eq!(app.history[0].occurred_at, app.now - 2 * model::DAY);
        assert!(!app.error);
        Ok(())
    }

    #[test]
    fn help_settings_save_immediately_without_losing_an_edit_or_command_selection() -> Result<()> {
        use crate::settings::{CardSpacing, Theme};
        let mut app = App::new(crate::demo_database()?)?;
        assert!(app.toast.is_empty());
        assert_eq!(app.settings, Settings::default());
        press(&mut app, KeyCode::Char('e'));
        type_text(&mut app, "quiet fern");
        press(&mut app, KeyCode::Down);
        type_text(&mut app, "Office / shade");
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Tab);
        assert!(matches!(
            app.mode,
            Mode::Help {
                tab: HelpTab::Settings,
                selected: 1,
                setting: 0,
                ..
            }
        ));
        press(&mut app, KeyCode::Left);
        assert_eq!(app.settings.theme, Theme::Default);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.settings.theme, Theme::Dark);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.settings.theme, Theme::Default);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.settings.theme, Theme::Greenhouse);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.settings.theme, Theme::Light);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert!(app.settings.display_artifacts);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.settings.card_spacing, CardSpacing::Compact);
        assert_eq!(app.db.settings()?, app.settings);
        press(&mut app, KeyCode::BackTab);
        assert!(matches!(
            app.mode,
            Mode::Help {
                tab: HelpTab::Commands,
                selected: 1,
                ..
            }
        ));
        press(&mut app, KeyCode::Tab);
        assert!(matches!(
            app.mode,
            Mode::Help {
                tab: HelpTab::Settings,
                setting: 2,
                ..
            }
        ));
        press(&mut app, KeyCode::Esc);
        let Mode::Form(form) = &app.mode else {
            panic!("lost edit")
        };
        assert_eq!(form.focused, 1);
        assert_eq!(form.fields[0].value, "quiet fern");
        assert_eq!(form.fields[1].value, "Office / shade");
        app.settings = Settings::default();
        app.reload()?;
        assert_eq!(app.settings.theme, Theme::Light);
        assert!(app.settings.display_artifacts);
        assert_eq!(app.settings.card_spacing, CardSpacing::Compact);
        Ok(())
    }

    #[test]
    fn environment_setup_is_opt_in_clearable_and_keeps_inputs_when_disabled() -> Result<()> {
        let mut db = Database::memory()?;
        let taxon = db
            .taxa()?
            .into_iter()
            .find(|taxon| taxon.id == "Goeppertia warszewiczii")
            .expect("pilot taxon");
        db.add_plant(&taxon, &PlantInput::default(), model::now())?;
        let mut app = App::new(db)?;
        assert!(!app.settings.environment_assessments_enabled);

        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Enter);
        let Mode::Form(form) = &mut app.mode else {
            panic!("environment setup did not open")
        };
        form.fields[0].value = "on".into();
        form.fields[1].value = "34.1".into();
        form.fields[2].value = "celsius".into();
        form.fields[3].value = "22".into();
        form.fields[4].value = "55".into();
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(app.settings.environment_assessments_enabled);
        assert_eq!(app.settings.home_environment.latitude, Some(34.1));
        assert!(matches!(app.mode, Mode::Help { .. }));
        press(&mut app, KeyCode::Esc);

        press(&mut app, KeyCode::Char('E'));
        let Mode::Form(form) = &mut app.mode else {
            panic!("plant environment form did not open")
        };
        form.fields[0].value = "partial_sun".into();
        form.fields[1].value = "23".into();
        form.fields[2].value = "65".into();
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let plant = app.selected_plant().unwrap();
        assert_eq!(plant.environment.exposure, Some(SunExposure::PartialSun));
        assert_eq!(plant.environment.temperature_c, Some(23.0));
        let plant_id = plant.id;

        press(&mut app, KeyCode::Char('v'));
        let Mode::Form(form) = &mut app.mode else {
            panic!("environment log form did not open")
        };
        form.fields[3].value = "61".into();
        form.fields[4].value = "700".into();
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let snapshot = app.environments.get(&plant_id).unwrap();
        assert_eq!(snapshot.humidity.map(|value| value.value), Some(61.0));
        assert_eq!(snapshot.lux.map(|value| value.value), Some(700.0));

        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::Enter);
        let Mode::Form(form) = &mut app.mode else {
            panic!("environment setup did not reopen")
        };
        form.fields[0].value = "off".into();
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(!app.settings.environment_assessments_enabled);
        assert_eq!(app.settings.home_environment.latitude, Some(34.1));
        assert_eq!(
            app.selected_plant().unwrap().environment.exposure,
            Some(SunExposure::PartialSun)
        );
        assert!(
            app.environments
                .get(&plant_id)
                .unwrap()
                .latest_lux_reading
                .is_some()
        );
        Ok(())
    }
}
