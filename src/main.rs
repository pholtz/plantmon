mod app;
mod art;
mod catalog;
mod db;
mod environment;
mod help;
mod knowledge;
mod model;
mod settings;
mod ui;

use crate::{
    app::App,
    db::{Database, PlantInput},
    model::{DAY, TaskKind},
};
use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{DisableLineWrap, EnableLineWrap},
};
use std::{
    collections::HashSet,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    version,
    about = "A persistent indoor garden and care journal. No network or sensors required."
)]
struct Args {
    /// Persistent SQLite file (created automatically, independent of working directory)
    #[arg(long, env = "PLANTMON_DB", default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/data/plantmon.sqlite3"))]
    database: PathBuf,
    /// Explore a temporary example garden; never writes to your real database
    #[arg(long, conflicts_with = "check")]
    demo: bool,
    /// Print due maintenance and exit, without opening the TUI
    #[arg(long)]
    check: bool,
    /// Print the bundled searchable botanical catalog and exit
    #[arg(long, value_name = "SEARCH")]
    catalog: Option<String>,
    /// Print source-backed plant knowledge and exit
    #[arg(long, value_name = "SEARCH")]
    guide: Option<String>,
    /// Ring the terminal bell when a task becomes due while monitoring
    #[arg(long)]
    bell: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(query) = &args.catalog {
        for t in catalog::builtin()?.into_iter().filter(|t| t.matches(query)) {
            println!("{}\t{}\t{}\t{}", t.id, t.common, t.family, t.groups);
        }
        return Ok(());
    }
    if let Some(query) = &args.guide {
        print!("{}", knowledge::render(query)?);
        return Ok(());
    }
    if !(args.check || io::stdin().is_terminal() && io::stdout().is_terminal()) {
        bail!(
            "plantmon needs an interactive terminal. Use --check, --catalog '', or --guide '' for plain text."
        );
    }
    let database = if args.demo {
        demo_database()?
    } else {
        Database::open(&args.database)?
    };
    let mut app = App::new(database)?;
    if args.check {
        let mut count = 0;
        for p in &app.plants {
            for s in &p.schedules {
                if let Some(due) = s.due_at().filter(|d| *d <= app.now) {
                    println!(
                        "{} [{}] · {} · {}{}",
                        p.name(),
                        p.taxon.id,
                        s.kind.label(),
                        model::countdown(due, app.now),
                        if s.kind == TaskKind::Water && s.last_completed.is_none() {
                            " (watering history unknown)"
                        } else {
                            ""
                        }
                    );
                    count += 1;
                }
            }
        }
        if count == 0 {
            println!(
                "No care reminders due. {} plants tracked.",
                app.plants.len()
            );
        }
        return Ok(());
    }
    if args.demo {
        app.notify("DEMO · temporary garden, discarded on exit.", false);
    }
    run(&mut app, args.bell)
}

fn run(app: &mut App, bell: bool) -> Result<()> {
    let mut terminal = ratatui::try_init().context("Initialize terminal")?;
    let result = (|| -> Result<()> {
        execute!(io::stdout(), DisableLineWrap, event::EnableBracketedPaste)?;
        let mut tick = Instant::now();
        let mut refresh = Instant::now();
        let mut due = due_tasks(app);
        while !app.quit {
            terminal.draw(|frame| ui::draw(frame, app))?;
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => app.key(key),
                    Event::Paste(value) => app.paste(&value),
                    _ => {}
                }
            }
            if tick.elapsed() >= Duration::from_secs(1) {
                app.tick();
                let current = due_tasks(app);
                if bell && !current.is_subset(&due) {
                    print!("\x07");
                    io::stdout().flush()?;
                }
                due = current;
                tick = Instant::now();
            }
            if refresh.elapsed() >= Duration::from_secs(15) {
                if let Err(error) = app.reload() {
                    app.notify(format!("Reload failed: {error:#}"), true);
                }
                refresh = Instant::now();
            }
        }
        Ok(())
    })();
    // Restore even after a draw, input, or database error. Ratatui also installs a panic hook.
    let cleanup = execute!(io::stdout(), event::DisableBracketedPaste, EnableLineWrap);
    ratatui::restore();
    result.and(cleanup.map_err(Into::into))
}

fn due_tasks(app: &App) -> HashSet<(i64, &'static str)> {
    app.plants
        .iter()
        .flat_map(|p| {
            p.schedules
                .iter()
                .filter(|s| s.due_at().is_some_and(|due| due <= app.now))
                .map(move |s| (p.id, s.kind.key()))
        })
        .collect()
}

fn demo_database() -> Result<Database> {
    let mut db = Database::memory()?;
    let taxa = db.taxa()?;
    let now = model::now();
    for (index, (name, botanical, location)) in [
        (
            "Mona",
            "Monstera deliciosa 'Thai Constellation'",
            "East window",
        ),
        ("Spike", "Opuntia microdasys", "South windowsill"),
        ("Fernanda", "Nephrolepis exaltata", "Bathroom"),
        ("Ruby", "Ficus elastica 'Ruby'", "Living room"),
        ("Pearl", "Curio rowleyanus", "South window"),
        ("Hoy there", "Hoya carnosa", "East window"),
        ("Velvet", "Alocasia reginula 'Black Velvet'", "Plant shelf"),
        ("Moon", "Phalaenopsis Hybrid Group", "Kitchen"),
    ]
    .into_iter()
    .enumerate()
    {
        let t = taxa
            .iter()
            .find(|t| t.id == botanical)
            .with_context(|| format!("Demo taxon {botanical}"))?;
        let id = db.add_plant(
            t,
            &PlantInput {
                nickname: name.into(),
                location: location.into(),
                notes:
                    "A demo specimen. Tune your real plants' intervals to their actual conditions."
                        .into(),
                ..Default::default()
            },
            now - 40 * DAY,
        )?;
        let elapsed = [8, 12, 9, 4, 13, 3, 2, 5][index];
        db.log(
            id,
            TaskKind::Water,
            now - elapsed * DAY,
            "Checked substrate and watered thoroughly",
            now,
        )?;
        db.log(
            id,
            TaskKind::Rotate,
            now - (index as i64 + 2) * DAY,
            "Quarter turn toward the light",
            now,
        )?;
    }
    Ok(db)
}
