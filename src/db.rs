use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    catalog,
    model::{DAY, MAX_INTERVAL, Maintenance, Plant, Schedule, TaskKind, Taxon},
    settings::{CardSpacing, Settings, Theme},
};

pub struct Database {
    conn: Connection,
}

#[derive(Default)]
pub struct PlantInput {
    pub nickname: String,
    pub location: String,
    pub acquired: String,
    pub pot: String,
    pub soil: String,
    pub tags: String,
    pub notes: String,
}

impl PlantInput {
    fn validate(&self) -> Result<()> {
        if !self.acquired.is_empty() {
            NaiveDate::parse_from_str(&self.acquired, "%Y-%m-%d")
                .context("Acquired date must be YYYY-MM-DD or blank")?;
        }
        Ok(())
    }
}

impl Database {
    pub fn memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).with_context(|| format!("Create {}", parent.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("Open {}", path.display()))?;
        Self::initialize(conn)
    }

    fn initialize(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 3 {
            bail!("This database needs a newer plantmon (schema {version})");
        }
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let tx = conn.transaction()?;
        tx.execute_batch("
            CREATE TABLE IF NOT EXISTS taxa (
                id TEXT PRIMARY KEY, genus TEXT NOT NULL, species TEXT NOT NULL,
                designation TEXT NOT NULL, common TEXT NOT NULL, family TEXT NOT NULL,
                groups_text TEXT NOT NULL, habit TEXT NOT NULL, water_days INTEGER NOT NULL CHECK(water_days BETWEEN 1 AND 3650),
                light TEXT NOT NULL, care TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS plants (
                id INTEGER PRIMARY KEY, taxon_id TEXT NOT NULL REFERENCES taxa(id),
                nickname TEXT NOT NULL, location TEXT NOT NULL, acquired TEXT NOT NULL,
                pot TEXT NOT NULL, soil TEXT NOT NULL, tags TEXT NOT NULL, notes TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS schedules (
                plant_id INTEGER NOT NULL REFERENCES plants(id) ON DELETE CASCADE,
                kind TEXT NOT NULL CHECK(kind IN ('water','rotate','fertilize','soil','inspect')),
                interval_days INTEGER NOT NULL CHECK(interval_days BETWEEN 0 AND 3650), initial_due_at INTEGER NOT NULL,
                PRIMARY KEY (plant_id, kind)
            );
            CREATE TABLE IF NOT EXISTS maintenance (
                id INTEGER PRIMARY KEY, plant_id INTEGER NOT NULL REFERENCES plants(id) ON DELETE CASCADE,
                kind TEXT NOT NULL CHECK(kind IN ('water','rotate','fertilize','soil','inspect')),
                occurred_at INTEGER NOT NULL, recorded_at INTEGER NOT NULL, note TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS maintenance_lookup ON maintenance(plant_id, kind, occurred_at DESC);
            CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        ")?;
        let has_display_order = {
            let mut columns = tx.prepare("PRAGMA table_info(plants)")?;
            let names = columns
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|name| name == "display_order")
        };
        if !has_display_order {
            tx.execute_batch(
                "ALTER TABLE plants ADD COLUMN display_order INTEGER NOT NULL DEFAULT 0;
                 UPDATE plants SET display_order = id;",
            )?;
        }
        tx.execute_batch("PRAGMA user_version = 3;")?;
        for taxon in catalog::builtin()? {
            tx.execute("INSERT INTO taxa VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO NOTHING",
                params![taxon.id, taxon.genus, taxon.species, taxon.designation, taxon.common,
                    taxon.family, taxon.groups, taxon.habit, taxon.water_days, taxon.light, taxon.care])?;
        }
        tx.commit()?;
        Ok(Self { conn })
    }

    pub fn settings(&self) -> Result<Settings> {
        let get = |key: &str| -> Result<Option<String>> {
            Ok(self
                .conn
                .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                    row.get(0)
                })
                .optional()?)
        };
        Ok(Settings {
            theme: get("theme")?
                .and_then(|value| Theme::parse(&value))
                .unwrap_or_default(),
            display_artifacts: get("display_artifacts")?.is_some_and(|value| value == "true"),
            card_spacing: get("card_spacing")?
                .and_then(|value| CardSpacing::parse(&value))
                .unwrap_or_default(),
            manual_order: get("manual_order")?.is_some_and(|value| value == "true"),
        })
    }

    pub fn save_settings(&mut self, settings: Settings) -> Result<()> {
        let tx = self.conn.transaction()?;
        for (key, value) in [
            ("theme", settings.theme.key()),
            (
                "display_artifacts",
                if settings.display_artifacts {
                    "true"
                } else {
                    "false"
                },
            ),
            ("card_spacing", settings.card_spacing.key()),
            (
                "manual_order",
                if settings.manual_order {
                    "true"
                } else {
                    "false"
                },
            ),
        ] {
            tx.execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn taxa(&self) -> Result<Vec<Taxon>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM taxa ORDER BY genus, species, designation")?;
        Ok(stmt
            .query_map([], taxon_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_taxon(&self, t: &Taxon) -> Result<()> {
        if t.genus.trim().is_empty() || t.common.trim().is_empty() {
            bail!("Genus and common name are required");
        }
        if !(1..=MAX_INTERVAL).contains(&t.water_days) {
            bail!("Water interval must be 1–3650 days");
        }
        self.conn
            .execute(
                "INSERT INTO taxa VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    t.id,
                    t.genus,
                    t.species,
                    t.designation,
                    t.common,
                    t.family,
                    t.groups,
                    t.habit,
                    t.water_days,
                    t.light,
                    t.care
                ],
            )
            .context("Save taxon (this botanical name may already exist)")?;
        Ok(())
    }

    pub fn add_plant(&mut self, taxon: &Taxon, input: &PlantInput, now: i64) -> Result<i64> {
        input.validate()?;
        let tx = self.conn.transaction()?;
        let display_order = tx.query_row(
            "SELECT COALESCE(MAX(display_order), -1) + 1 FROM plants",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        tx.execute("INSERT INTO plants (taxon_id,nickname,location,acquired,pot,soil,tags,notes,created_at,display_order)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![taxon.id, input.nickname, input.location, input.acquired,
                input.pot, input.soil, input.tags, input.notes, now, display_order])?;
        let id = tx.last_insert_rowid();
        for (kind, interval) in [
            (TaskKind::Water, taxon.water_days),
            (TaskKind::Rotate, 14),
            (
                TaskKind::Fertilize,
                if taxon.groups.to_lowercase().contains("carnivorous") {
                    0
                } else {
                    30
                },
            ),
            (TaskKind::Soil, 365),
            (TaskKind::Inspect, 7),
        ] {
            // A newly added plant has unknown hydration, not an invented watering event.
            let due = if kind == TaskKind::Water {
                now
            } else {
                now + interval * DAY
            };
            tx.execute(
                "INSERT INTO schedules VALUES (?1,?2,?3,?4)",
                params![id, kind.key(), interval, due],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn edit_plant(&self, id: i64, input: &PlantInput) -> Result<()> {
        input.validate()?;
        self.conn.execute("UPDATE plants SET nickname=?2,location=?3,acquired=?4,pot=?5,soil=?6,tags=?7,notes=?8 WHERE id=?1",
            params![id, input.nickname, input.location, input.acquired, input.pot, input.soil, input.tags, input.notes])?;
        Ok(())
    }

    pub fn plants(&self) -> Result<Vec<Plant>> {
        let taxa = self.taxa()?;
        let mut stmt = self.conn.prepare(
            "SELECT id,taxon_id,nickname,location,acquired,pot,soil,tags,notes,display_order
            FROM plants ORDER BY display_order,id",
        )?;
        let mut plants = stmt
            .query_map([], |r| {
                let taxon_id: String = r.get(1)?;
                let taxon = taxa
                    .iter()
                    .find(|t| t.id == taxon_id)
                    .cloned()
                    .ok_or(rusqlite::Error::InvalidQuery)?;
                Ok(Plant {
                    id: r.get(0)?,
                    display_order: r.get(9)?,
                    taxon,
                    nickname: r.get(2)?,
                    location: r.get(3)?,
                    acquired: r.get(4)?,
                    pot: r.get(5)?,
                    soil: r.get(6)?,
                    tags: r.get(7)?,
                    notes: r.get(8)?,
                    schedules: vec![],
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut schedule_stmt = self.conn.prepare("SELECT s.kind,s.interval_days,s.initial_due_at,
            (SELECT MAX(m.occurred_at) FROM maintenance m WHERE m.plant_id=s.plant_id AND m.kind=s.kind)
            FROM schedules s WHERE s.plant_id=?1 ORDER BY CASE s.kind
            WHEN 'water' THEN 0 WHEN 'rotate' THEN 1 WHEN 'fertilize' THEN 2 WHEN 'soil' THEN 3 ELSE 4 END")?;
        for plant in &mut plants {
            plant.schedules = schedule_stmt
                .query_map([plant.id], |r| {
                    let key: String = r.get(0)?;
                    Ok(Schedule {
                        kind: TaskKind::parse(&key).map_err(|_| rusqlite::Error::InvalidQuery)?,
                        interval_days: r.get(1)?,
                        initial_due_at: r.get(2)?,
                        last_completed: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            if plant.schedules.len() != TaskKind::ALL.len() {
                bail!("Plant {} has incomplete schedules", plant.id);
            }
        }
        Ok(plants)
    }

    pub fn log(
        &self,
        plant: i64,
        kind: TaskKind,
        occurred: i64,
        note: &str,
        now: i64,
    ) -> Result<()> {
        if occurred > now {
            bail!("Maintenance cannot be recorded in the future");
        }
        self.conn.execute("INSERT INTO maintenance (plant_id,kind,occurred_at,recorded_at,note) VALUES (?1,?2,?3,?4,?5)",
            params![plant, kind.key(), occurred, now, note])?;
        Ok(())
    }

    pub fn history(&self, plant: i64) -> Result<Vec<Maintenance>> {
        let mut stmt = self.conn.prepare("SELECT id,kind,occurred_at,note FROM maintenance WHERE plant_id=?1 ORDER BY occurred_at DESC,id DESC")?;
        Ok(stmt
            .query_map([plant], |r| {
                let key: String = r.get(1)?;
                Ok(Maintenance {
                    id: r.get(0)?,
                    kind: TaskKind::parse(&key).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    occurred_at: r.get(2)?,
                    note: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_intervals(&mut self, plant: i64, intervals: &[i64], now: i64) -> Result<()> {
        if intervals.len() != 5 || intervals.iter().any(|n| !(0..=MAX_INTERVAL).contains(n)) {
            bail!("Use five intervals, each 0–3650 days (0 disables)");
        }
        let tx = self.conn.transaction()?;
        for (kind, days) in TaskKind::ALL.into_iter().zip(intervals) {
            // Reset the initial anchor only when changing the interval; completed tasks
            // always remain anchored to the actual last completion.
            tx.execute("UPDATE schedules SET initial_due_at=CASE WHEN interval_days != ?3 THEN ?4 ELSE initial_due_at END,
                interval_days=?3 WHERE plant_id=?1 AND kind=?2",
                params![plant, kind.key(), days, now + days * DAY])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_plant(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM plants WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn reorder_plants(&mut self, plant_ids: &[i64]) -> Result<()> {
        let unique_count = plant_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        let tx = self.conn.transaction()?;
        let plant_count = tx.query_row("SELECT COUNT(*) FROM plants", [], |row| {
            row.get::<_, i64>(0)
        })?;
        if plant_ids.len() as i64 != plant_count || unique_count != plant_ids.len() {
            bail!("Plant collection changed; reopen the reorder list and try again");
        }
        for (display_order, plant_id) in plant_ids.iter().enumerate() {
            let changed = tx.execute(
                "UPDATE plants SET display_order=?2 WHERE id=?1",
                params![plant_id, display_order as i64],
            )?;
            if changed != 1 {
                bail!("Plant collection changed; reopen the reorder list and try again");
            }
        }
        tx.execute(
            "INSERT INTO settings (key,value) VALUES ('manual_order','true')
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_event(&self, plant: i64, event: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM maintenance WHERE id=?1 AND plant_id=?2",
            params![event, plant],
        )?;
        Ok(())
    }
}

fn taxon_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Taxon> {
    Ok(Taxon {
        id: r.get(0)?,
        genus: r.get(1)?,
        species: r.get(2)?,
        designation: r.get(3)?,
        common: r.get(4)?,
        family: r.get(5)?,
        groups: r.get(6)?,
        habit: r.get(7)?,
        water_days: r.get(8)?,
        light: r.get(9)?,
        care: r.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backdating_does_not_move_timers_backwards_and_undo_recalculates() -> Result<()> {
        let mut db = Database::initialize(Connection::open_in_memory()?)?;
        let taxon = db.taxa()?.remove(0);
        let id = db.add_plant(&taxon, &PlantInput::default(), 100 * DAY)?;
        db.log(id, TaskKind::Water, 99 * DAY, "recent", 100 * DAY)?;
        db.log(
            id,
            TaskKind::Water,
            90 * DAY,
            "forgotten older event",
            100 * DAY,
        )?;
        assert_eq!(db.plants()?[0].water().last_completed, Some(99 * DAY));
        let recent = db.history(id)?[0].id;
        db.remove_event(id, recent)?;
        assert_eq!(db.plants()?[0].water().last_completed, Some(90 * DAY));
        assert!(
            db.log(id, TaskKind::Water, 101 * DAY, "future", 100 * DAY)
                .is_err()
        );
        db.set_intervals(id, &[0, 14, 30, 365, 7], 100 * DAY)?;
        assert_eq!(db.plants()?[0].water().due_at(), None);
        db.remove_plant(id)?;
        assert!(db.history(id)?.is_empty());
        Ok(())
    }

    #[test]
    fn reopening_preserves_collection_history_and_custom_taxa() -> Result<()> {
        let path = std::env::temp_dir().join(format!(
            "plantmon-test-{}-{}.sqlite3",
            std::process::id(),
            crate::model::now()
        ));
        {
            let mut db = Database::open(&path)?;
            let mut taxon = db.taxa()?.remove(0);
            taxon.id = "Testus example 'Personal'".into();
            taxon.genus = "Testus".into();
            taxon.species = "example".into();
            taxon.designation = "'Personal'".into();
            db.add_taxon(&taxon)?;
            let id = db.add_plant(
                &taxon,
                &PlantInput {
                    nickname: "Fernanda".into(),
                    ..Default::default()
                },
                100 * DAY,
            )?;
            db.log(id, TaskKind::Water, 95 * DAY, "before shutdown", 100 * DAY)?;
            db.save_settings(Settings {
                theme: Theme::Default,
                display_artifacts: true,
                card_spacing: CardSpacing::Compact,
                manual_order: false,
            })?;
        }
        {
            let db = Database::open(&path)?;
            let plants = db.plants()?;
            assert_eq!(plants.len(), 1);
            assert_eq!(plants[0].name(), "Fernanda");
            assert_eq!(plants[0].taxon.designation, "'Personal'");
            assert_eq!(plants[0].water().last_completed, Some(95 * DAY));
            assert_eq!(db.history(plants[0].id)?[0].note, "before shutdown");
            assert_eq!(
                db.settings()?,
                Settings {
                    theme: Theme::Default,
                    display_artifacts: true,
                    card_spacing: CardSpacing::Compact,
                    manual_order: false,
                }
            );
        }
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn old_versions_upgrade_additively_and_future_schemas_are_refused() -> Result<()> {
        let mut old = Database::memory()?;
        let taxon = old.taxa()?.remove(0);
        let id = old.add_plant(&taxon, &PlantInput::default(), 100 * DAY)?;
        old.log(id, TaskKind::Water, 99 * DAY, "before migration", 100 * DAY)?;
        old.conn.execute_batch(
            "DROP TABLE settings;
                 ALTER TABLE plants DROP COLUMN display_order;
                 PRAGMA user_version = 1;",
        )?;
        let db = Database::initialize(old.conn)?;
        assert_eq!(db.settings()?, Settings::default());
        assert_eq!(db.plants()?[0].water().last_completed, Some(99 * DAY));
        assert_eq!(db.plants()?[0].display_order, id);
        assert_eq!(db.history(id)?[0].note, "before migration");
        assert_eq!(
            db.conn
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?,
            3
        );
        db.conn.execute_batch("PRAGMA user_version = 99;")?;
        assert!(Database::initialize(db.conn).is_err());
        Ok(())
    }

    #[test]
    fn manual_order_is_validated_and_persisted() -> Result<()> {
        let mut db = Database::memory()?;
        let taxon = db.taxa()?.remove(0);
        let first = db.add_plant(
            &taxon,
            &PlantInput {
                nickname: "First".into(),
                ..Default::default()
            },
            100 * DAY,
        )?;
        let second = db.add_plant(
            &taxon,
            &PlantInput {
                nickname: "Second".into(),
                ..Default::default()
            },
            100 * DAY,
        )?;
        assert!(db.reorder_plants(&[first]).is_err());
        db.reorder_plants(&[second, first])?;
        assert_eq!(
            db.plants()?
                .iter()
                .map(|plant| plant.id)
                .collect::<Vec<_>>(),
            vec![second, first]
        );
        assert!(db.settings()?.manual_order);
        Ok(())
    }
}
