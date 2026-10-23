use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    catalog,
    model::{
        DAY, EnvironmentReadingInput, EnvironmentScope, EnvironmentSnapshot, EnvironmentSource,
        EnvironmentSummary, EnvironmentValue, EnvironmentValueSource, InspectionExtent,
        InspectionFinding, InspectionFindingInput, InspectionFindingKind, InspectionSeverity,
        InspectionTrend, MAX_INTERVAL, Maintenance, MetricSummary, PestKind, Plant,
        PlantEnvironment, PlantPart, Schedule, SunExposure, TaskKind, Taxon,
    },
    settings::{CardSpacing, HomeEnvironment, Settings, TemperatureUnit, Theme},
};

const READING_FRESH_SECONDS: i64 = DAY;
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

fn validate_home_environment(environment: HomeEnvironment) -> Result<()> {
    if let Some(latitude) = environment.latitude
        && (!latitude.is_finite() || !(-90.0..=90.0).contains(&latitude))
    {
        bail!("Approximate latitude must be between -90 and 90");
    }
    if let Some(temperature) = environment.baseline_temperature_c {
        if !temperature.is_finite() || !(-100.0..=100.0).contains(&temperature) {
            bail!("Baseline temperature is outside the supported range");
        }
        if environment.temperature_unit.is_none() {
            bail!("Choose a preferred temperature unit when setting a temperature");
        }
    }
    if let Some(humidity) = environment.baseline_humidity_percent
        && (!humidity.is_finite() || !(0.0..=100.0).contains(&humidity))
    {
        bail!("Baseline humidity must be between 0 and 100%");
    }
    Ok(())
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
        if version > 5 {
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
            CREATE TABLE IF NOT EXISTS inspection_findings (
                id INTEGER PRIMARY KEY,
                maintenance_id INTEGER NOT NULL REFERENCES maintenance(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                plant_part TEXT,
                severity TEXT,
                extent TEXT,
                trend TEXT NOT NULL,
                pest_kind TEXT,
                note TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS inspection_findings_lookup
                ON inspection_findings(maintenance_id, id);
            CREATE TABLE IF NOT EXISTS plant_environment (
                plant_id INTEGER PRIMARY KEY REFERENCES plants(id) ON DELETE CASCADE,
                sun_exposure TEXT CHECK(sun_exposure IS NULL OR sun_exposure IN ('full_shade','mostly_shade','partial_sun','mostly_sun','full_sun')),
                temperature_c REAL CHECK(temperature_c IS NULL OR temperature_c BETWEEN -100.0 AND 100.0),
                humidity_percent REAL CHECK(humidity_percent IS NULL OR humidity_percent BETWEEN 0.0 AND 100.0)
            );
            CREATE TABLE IF NOT EXISTS environment_readings (
                id INTEGER PRIMARY KEY,
                plant_id INTEGER REFERENCES plants(id) ON DELETE CASCADE,
                scope TEXT NOT NULL CHECK(scope IN ('home','plant')),
                source TEXT NOT NULL CHECK(source IN ('manual','import','sensor')),
                observed_at INTEGER NOT NULL,
                recorded_at INTEGER NOT NULL,
                temperature_c REAL CHECK(temperature_c IS NULL OR temperature_c BETWEEN -100.0 AND 100.0),
                humidity_percent REAL CHECK(humidity_percent IS NULL OR humidity_percent BETWEEN 0.0 AND 100.0),
                lux REAL CHECK(lux IS NULL OR lux >= 0.0),
                ppfd REAL CHECK(ppfd IS NULL OR ppfd >= 0.0),
                CHECK((scope = 'home' AND plant_id IS NULL) OR (scope = 'plant' AND plant_id IS NOT NULL)),
                CHECK(temperature_c IS NOT NULL OR humidity_percent IS NOT NULL OR lux IS NOT NULL OR ppfd IS NOT NULL)
            );
            CREATE INDEX IF NOT EXISTS environment_readings_lookup
                ON environment_readings(scope, plant_id, observed_at DESC, id DESC);
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
        tx.execute_batch("PRAGMA user_version = 5;")?;
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
        let get_number = |key: &str| -> Result<Option<f64>> {
            get(key)?
                .map(|value| {
                    value
                        .parse::<f64>()
                        .with_context(|| format!("Invalid numeric setting {key}"))
                })
                .transpose()
        };
        let settings = Settings {
            theme: get("theme")?
                .and_then(|value| Theme::parse(&value))
                .unwrap_or_default(),
            display_artifacts: get("display_artifacts")?.is_some_and(|value| value == "true"),
            card_spacing: get("card_spacing")?
                .and_then(|value| CardSpacing::parse(&value))
                .unwrap_or_default(),
            manual_order: get("manual_order")?.is_some_and(|value| value == "true"),
            environment_assessments_enabled: get("environment_assessments_enabled")?
                .is_some_and(|value| value == "true"),
            home_environment: HomeEnvironment {
                latitude: get_number("environment_latitude")?,
                temperature_unit: get("environment_temperature_unit")?
                    .and_then(|value| TemperatureUnit::parse(&value)),
                baseline_temperature_c: get_number("environment_baseline_temperature_c")?,
                baseline_humidity_percent: get_number("environment_baseline_humidity_percent")?,
            },
        };
        validate_home_environment(settings.home_environment)?;
        Ok(settings)
    }

    pub fn save_settings(&mut self, settings: Settings) -> Result<()> {
        validate_home_environment(settings.home_environment)?;
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
            (
                "environment_assessments_enabled",
                if settings.environment_assessments_enabled {
                    "true"
                } else {
                    "false"
                },
            ),
        ] {
            tx.execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        }
        let environment = settings.home_environment;
        for (key, value) in [
            (
                "environment_latitude",
                environment.latitude.map(|value| value.to_string()),
            ),
            (
                "environment_temperature_unit",
                environment
                    .temperature_unit
                    .map(|unit| unit.key().to_string()),
            ),
            (
                "environment_baseline_temperature_c",
                environment
                    .baseline_temperature_c
                    .map(|value| value.to_string()),
            ),
            (
                "environment_baseline_humidity_percent",
                environment
                    .baseline_humidity_percent
                    .map(|value| value.to_string()),
            ),
        ] {
            if let Some(value) = value {
                tx.execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
            } else {
                tx.execute("DELETE FROM settings WHERE key=?1", [key])?;
            }
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
            "SELECT p.id,p.taxon_id,p.nickname,p.location,p.acquired,p.pot,p.soil,p.tags,p.notes,p.display_order,
                    e.sun_exposure,e.temperature_c,e.humidity_percent
             FROM plants p LEFT JOIN plant_environment e ON e.plant_id=p.id
             ORDER BY p.display_order,p.id",
        )?;
        let mut plants = stmt
            .query_map([], |r| {
                let taxon_id: String = r.get(1)?;
                let taxon = taxa
                    .iter()
                    .find(|t| t.id == taxon_id)
                    .cloned()
                    .ok_or(rusqlite::Error::InvalidQuery)?;
                let exposure = r
                    .get::<_, Option<String>>(10)?
                    .map(|value| SunExposure::parse(&value).ok_or(rusqlite::Error::InvalidQuery))
                    .transpose()?;
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
                    environment: PlantEnvironment {
                        exposure,
                        temperature_c: r.get(11)?,
                        humidity_percent: r.get(12)?,
                    },
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

    pub fn log_inspection(
        &mut self,
        plant: i64,
        occurred: i64,
        note: &str,
        findings: &[InspectionFindingInput],
        now: i64,
    ) -> Result<i64> {
        if occurred > now {
            bail!("Inspection cannot be recorded in the future");
        }
        if findings.is_empty() {
            bail!("Add at least one inspection finding");
        }
        if findings
            .iter()
            .any(|finding| finding.kind == InspectionFindingKind::NoAdverseSigns)
            && findings.iter().any(|finding| finding.kind.is_concern())
        {
            bail!("No adverse signs cannot be combined with a concern");
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO maintenance (plant_id,kind,occurred_at,recorded_at,note)
             VALUES (?1,'inspect',?2,?3,?4)",
            params![plant, occurred, now, note],
        )?;
        let maintenance = tx.last_insert_rowid();
        for finding in findings {
            tx.execute(
                "INSERT INTO inspection_findings
                    (maintenance_id,kind,plant_part,severity,extent,trend,pest_kind,note)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    maintenance,
                    finding.kind.key(),
                    finding.part.map(PlantPart::key),
                    finding.severity.map(InspectionSeverity::key),
                    finding.extent.map(InspectionExtent::key),
                    finding.trend.key(),
                    finding.pest.map(PestKind::key),
                    finding.note,
                ],
            )?;
        }
        tx.commit()?;
        Ok(maintenance)
    }

    pub fn history(&self, plant: i64) -> Result<Vec<Maintenance>> {
        let mut stmt = self.conn.prepare("SELECT id,kind,occurred_at,note FROM maintenance WHERE plant_id=?1 ORDER BY occurred_at DESC,id DESC")?;
        let mut history = stmt
            .query_map([plant], |r| {
                let key: String = r.get(1)?;
                Ok(Maintenance {
                    id: r.get(0)?,
                    kind: TaskKind::parse(&key).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    occurred_at: r.get(2)?,
                    note: r.get(3)?,
                    findings: vec![],
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut findings = self.conn.prepare(
            "SELECT id,kind,plant_part,severity,extent,trend,pest_kind,note
             FROM inspection_findings WHERE maintenance_id=?1 ORDER BY id",
        )?;
        for event in &mut history {
            event.findings = findings
                .query_map([event.id], |row| {
                    let kind: String = row.get(1)?;
                    let part: Option<String> = row.get(2)?;
                    let severity: Option<String> = row.get(3)?;
                    let extent: Option<String> = row.get(4)?;
                    let trend: String = row.get(5)?;
                    let pest: Option<String> = row.get(6)?;
                    Ok(InspectionFinding {
                        id: row.get(0)?,
                        kind: InspectionFindingKind::parse(&kind)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        part: part
                            .map(|value| PlantPart::parse(&value))
                            .transpose()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        severity: severity
                            .map(|value| InspectionSeverity::parse(&value))
                            .transpose()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        extent: extent
                            .map(|value| InspectionExtent::parse(&value))
                            .transpose()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        trend: InspectionTrend::parse(&trend)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        pest: pest
                            .map(|value| PestKind::parse(&value))
                            .transpose()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        note: row.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok(history)
    }

    pub fn set_plant_environment(&self, plant: i64, environment: PlantEnvironment) -> Result<()> {
        validate_plant_environment(environment)?;
        if environment.is_empty() {
            self.conn
                .execute("DELETE FROM plant_environment WHERE plant_id=?1", [plant])?;
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO plant_environment (plant_id,sun_exposure,temperature_c,humidity_percent)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(plant_id) DO UPDATE SET
                sun_exposure=excluded.sun_exposure,
                temperature_c=excluded.temperature_c,
                humidity_percent=excluded.humidity_percent",
            params![
                plant,
                environment.exposure.map(SunExposure::key),
                environment.temperature_c,
                environment.humidity_percent
            ],
        )?;
        Ok(())
    }

    pub fn log_environment(&self, input: EnvironmentReadingInput, now: i64) -> Result<i64> {
        validate_environment_reading(input, now)?;
        self.conn.execute(
            "INSERT INTO environment_readings
                (plant_id,scope,source,observed_at,recorded_at,temperature_c,humidity_percent,lux,ppfd)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                input.plant_id,
                input.scope.key(),
                input.source.key(),
                input.observed_at,
                now,
                input.temperature_c,
                input.humidity_percent,
                input.lux,
                input.ppfd
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn environment_summary(
        &self,
        scope: EnvironmentScope,
        plant: Option<i64>,
        now: i64,
        days: u8,
    ) -> Result<EnvironmentSummary> {
        if !(1..=7).contains(&days) {
            bail!("Environment summary must cover 1–7 days");
        }
        validate_scope(scope, plant)?;
        let since = now.saturating_sub(i64::from(days) * DAY);
        self.conn.query_row(
            "SELECT
                COUNT(temperature_c), MIN(temperature_c), MAX(temperature_c), AVG(temperature_c),
                COUNT(humidity_percent), MIN(humidity_percent), MAX(humidity_percent), AVG(humidity_percent),
                COUNT(lux), MIN(lux), MAX(lux), AVG(lux),
                COUNT(ppfd), MIN(ppfd), MAX(ppfd), AVG(ppfd)
             FROM environment_readings
             WHERE scope=?1 AND plant_id IS ?2 AND observed_at BETWEEN ?3 AND ?4",
            params![scope.key(), plant, since, now],
            |row| {
                Ok(EnvironmentSummary {
                    temperature: summary_metric(row, 0)?,
                    humidity: summary_metric(row, 4)?,
                    lux: summary_metric(row, 8)?,
                    ppfd: summary_metric(row, 12)?,
                })
            },
        ).map_err(Into::into)
    }

    pub fn environment_snapshot(&self, plant: i64, now: i64) -> Result<EnvironmentSnapshot> {
        let settings = self.settings()?;
        let plant_environment = self
            .conn
            .query_row(
                "SELECT sun_exposure,temperature_c,humidity_percent
                 FROM plant_environment WHERE plant_id=?1",
                [plant],
                |row| {
                    let exposure = row
                        .get::<_, Option<String>>(0)?
                        .map(|value| {
                            SunExposure::parse(&value).ok_or(rusqlite::Error::InvalidQuery)
                        })
                        .transpose()?;
                    Ok(PlantEnvironment {
                        exposure,
                        temperature_c: row.get(1)?,
                        humidity_percent: row.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default();

        let plant_temperature = self.latest_environment_value(
            EnvironmentScope::Plant,
            Some(plant),
            "temperature_c",
            now,
        )?;
        let home_temperature =
            self.latest_environment_value(EnvironmentScope::Home, None, "temperature_c", now)?;
        let plant_humidity = self.latest_environment_value(
            EnvironmentScope::Plant,
            Some(plant),
            "humidity_percent",
            now,
        )?;
        let home_humidity =
            self.latest_environment_value(EnvironmentScope::Home, None, "humidity_percent", now)?;
        let plant_lux =
            self.latest_environment_value(EnvironmentScope::Plant, Some(plant), "lux", now)?;
        let home_lux = self.latest_environment_value(EnvironmentScope::Home, None, "lux", now)?;
        let plant_ppfd =
            self.latest_environment_value(EnvironmentScope::Plant, Some(plant), "ppfd", now)?;
        let home_ppfd = self.latest_environment_value(EnvironmentScope::Home, None, "ppfd", now)?;

        let fresh = |value: Option<EnvironmentValue>| {
            value.filter(|value| {
                value
                    .observed_at
                    .is_some_and(|observed| now.saturating_sub(observed) <= READING_FRESH_SECONDS)
            })
        };
        let plant_temperature_fresh = fresh(plant_temperature);
        let home_temperature_fresh = fresh(home_temperature);
        let plant_humidity_fresh = fresh(plant_humidity);
        let home_humidity_fresh = fresh(home_humidity);
        let plant_lux_fresh = fresh(plant_lux);
        let home_lux_fresh = fresh(home_lux);
        let plant_ppfd_fresh = fresh(plant_ppfd);
        let home_ppfd_fresh = fresh(home_ppfd);

        let plant_daily = self.environment_summary(EnvironmentScope::Plant, Some(plant), now, 1)?;
        let home_daily = self.environment_summary(EnvironmentScope::Home, None, now, 1)?;
        let plant_weekly =
            self.environment_summary(EnvironmentScope::Plant, Some(plant), now, 7)?;
        let home_weekly = self.environment_summary(EnvironmentScope::Home, None, now, 7)?;

        Ok(EnvironmentSnapshot {
            temperature: plant_temperature_fresh
                .or_else(|| {
                    plant_environment
                        .temperature_c
                        .map(|value| EnvironmentValue {
                            value,
                            scope: EnvironmentScope::Plant,
                            source: EnvironmentValueSource::Override,
                            observed_at: None,
                        })
                })
                .or(home_temperature_fresh)
                .or_else(|| {
                    settings
                        .home_environment
                        .baseline_temperature_c
                        .map(|value| EnvironmentValue {
                            value,
                            scope: EnvironmentScope::Home,
                            source: EnvironmentValueSource::Baseline,
                            observed_at: None,
                        })
                }),
            humidity: plant_humidity_fresh
                .or_else(|| {
                    plant_environment
                        .humidity_percent
                        .map(|value| EnvironmentValue {
                            value,
                            scope: EnvironmentScope::Plant,
                            source: EnvironmentValueSource::Override,
                            observed_at: None,
                        })
                })
                .or(home_humidity_fresh)
                .or_else(|| {
                    settings
                        .home_environment
                        .baseline_humidity_percent
                        .map(|value| EnvironmentValue {
                            value,
                            scope: EnvironmentScope::Home,
                            source: EnvironmentValueSource::Baseline,
                            observed_at: None,
                        })
                }),
            lux: plant_lux_fresh.or(home_lux_fresh),
            ppfd: plant_ppfd_fresh.or(home_ppfd_fresh),
            latest_temperature_reading: plant_temperature.or(home_temperature),
            latest_humidity_reading: plant_humidity.or(home_humidity),
            latest_lux_reading: plant_lux.or(home_lux),
            latest_ppfd_reading: plant_ppfd.or(home_ppfd),
            daily: merge_summaries(plant_daily, home_daily),
            weekly: merge_summaries(plant_weekly, home_weekly),
        })
    }

    fn latest_environment_value(
        &self,
        scope: EnvironmentScope,
        plant: Option<i64>,
        column: &str,
        now: i64,
    ) -> Result<Option<EnvironmentValue>> {
        validate_scope(scope, plant)?;
        if !matches!(
            column,
            "temperature_c" | "humidity_percent" | "lux" | "ppfd"
        ) {
            bail!("Unsupported environment metric");
        }
        let sql = format!(
            "SELECT {column},source,observed_at FROM environment_readings
             WHERE scope=?1 AND plant_id IS ?2 AND observed_at<=?3 AND {column} IS NOT NULL
             ORDER BY observed_at DESC,id DESC LIMIT 1"
        );
        self.conn
            .query_row(&sql, params![scope.key(), plant, now], |row| {
                let source: String = row.get(1)?;
                let source =
                    EnvironmentSource::parse(&source).ok_or(rusqlite::Error::InvalidQuery)?;
                Ok(EnvironmentValue {
                    value: row.get(0)?,
                    scope,
                    source: EnvironmentValueSource::Reading(source),
                    observed_at: Some(row.get(2)?),
                })
            })
            .optional()
            .map_err(Into::into)
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
fn validate_plant_environment(environment: PlantEnvironment) -> Result<()> {
    if let Some(temperature) = environment.temperature_c
        && (!temperature.is_finite() || !(-100.0..=100.0).contains(&temperature))
    {
        bail!("Plant temperature override is outside the supported range");
    }
    if let Some(humidity) = environment.humidity_percent
        && (!humidity.is_finite() || !(0.0..=100.0).contains(&humidity))
    {
        bail!("Plant humidity override must be between 0 and 100%");
    }
    Ok(())
}

fn validate_scope(scope: EnvironmentScope, plant: Option<i64>) -> Result<()> {
    if matches!(scope, EnvironmentScope::Home) != plant.is_none() {
        bail!("Home readings cannot have a plant; plant readings require one");
    }
    Ok(())
}

fn validate_environment_reading(input: EnvironmentReadingInput, now: i64) -> Result<()> {
    validate_scope(input.scope, input.plant_id)?;
    if input.observed_at > now {
        bail!("Environment readings cannot be recorded in the future");
    }
    if input.temperature_c.is_none()
        && input.humidity_percent.is_none()
        && input.lux.is_none()
        && input.ppfd.is_none()
    {
        bail!("Enter at least one environment metric");
    }
    validate_plant_environment(PlantEnvironment {
        exposure: None,
        temperature_c: input.temperature_c,
        humidity_percent: input.humidity_percent,
    })?;
    for (label, value) in [("Lux", input.lux), ("PPFD", input.ppfd)] {
        if let Some(value) = value
            && (!value.is_finite() || value < 0.0)
        {
            bail!("{label} must be a non-negative number");
        }
    }
    Ok(())
}

fn summary_metric(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<Option<MetricSummary>> {
    let count = row.get::<_, i64>(offset)?;
    if count == 0 {
        return Ok(None);
    }
    Ok(Some(MetricSummary {
        count: count as usize,
        minimum: row.get(offset + 1)?,
        maximum: row.get(offset + 2)?,
        mean: row.get(offset + 3)?,
    }))
}

fn merge_summaries(plant: EnvironmentSummary, home: EnvironmentSummary) -> EnvironmentSummary {
    EnvironmentSummary {
        temperature: plant.temperature.or(home.temperature),
        humidity: plant.humidity.or(home.humidity),
        lux: plant.lux.or(home.lux),
        ppfd: plant.ppfd.or(home.ppfd),
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
    fn inspections_store_queryable_findings_and_reject_contradictions() -> Result<()> {
        let mut db = Database::memory()?;
        let taxon = db.taxa()?.remove(0);
        let plant = db.add_plant(&taxon, &PlantInput::default(), 100 * DAY)?;
        let findings = vec![
            InspectionFindingInput {
                kind: InspectionFindingKind::NewLeaf,
                part: Some(PlantPart::NewGrowth),
                severity: None,
                extent: Some(InspectionExtent::Single),
                trend: InspectionTrend::New,
                pest: None,
                note: "unfurling".into(),
            },
            InspectionFindingInput {
                kind: InspectionFindingKind::LeafBrowning,
                part: Some(PlantPart::Leaf),
                severity: Some(InspectionSeverity::Mild),
                extent: Some(InspectionExtent::Localized),
                trend: InspectionTrend::Stable,
                pest: None,
                note: "oldest leaf".into(),
            },
        ];
        let event =
            db.log_inspection(plant, 99 * DAY, "window-side check", &findings, 100 * DAY)?;
        let history = db.history(plant)?;
        assert_eq!(history[0].id, event);
        assert_eq!(history[0].kind, TaskKind::Inspect);
        assert_eq!(history[0].findings.len(), 2);
        assert_eq!(history[0].findings[0].kind, InspectionFindingKind::NewLeaf);
        assert_eq!(
            history[0].findings[1].severity,
            Some(InspectionSeverity::Mild)
        );
        assert_eq!(history[0].findings[1].note, "oldest leaf");

        let contradictory = [
            default_inspection_finding_for_test(InspectionFindingKind::NoAdverseSigns),
            default_inspection_finding_for_test(InspectionFindingKind::LeafDrop),
        ];
        assert!(
            db.log_inspection(plant, 100 * DAY, "", &contradictory, 100 * DAY)
                .is_err()
        );
        db.remove_event(plant, event)?;
        assert!(db.history(plant)?.is_empty());
        Ok(())
    }

    fn default_inspection_finding_for_test(kind: InspectionFindingKind) -> InspectionFindingInput {
        InspectionFindingInput {
            kind,
            part: None,
            severity: None,
            extent: None,
            trend: InspectionTrend::New,
            pest: None,
            note: String::new(),
        }
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
                ..Default::default()
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
                    ..Default::default()
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
                 DROP TABLE inspection_findings;
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
            5
        );
        db.conn.execute_batch("PRAGMA user_version = 99;")?;
        assert!(Database::initialize(db.conn).is_err());
        Ok(())
    }
    #[test]
    fn schema_three_migrates_environment_tables_additively_and_optional_fields_clear() -> Result<()>
    {
        let mut old = Database::memory()?;
        let taxon = old.taxa()?.remove(0);
        let id = old.add_plant(&taxon, &PlantInput::default(), 100 * DAY)?;
        old.log(
            id,
            TaskKind::Water,
            99 * DAY,
            "before schema four",
            100 * DAY,
        )?;
        old.conn.execute_batch(
            "DROP TABLE environment_readings;
             DROP TABLE plant_environment;
             PRAGMA user_version = 3;",
        )?;

        let mut db = Database::initialize(old.conn)?;
        assert_eq!(db.plants()?[0].water().last_completed, Some(99 * DAY));
        assert_eq!(
            db.conn
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?,
            5
        );

        let mut settings = db.settings()?;
        settings.environment_assessments_enabled = true;
        settings.home_environment = HomeEnvironment {
            latitude: Some(34.1),
            temperature_unit: Some(TemperatureUnit::Fahrenheit),
            baseline_temperature_c: Some(22.0),
            baseline_humidity_percent: Some(55.0),
        };
        db.save_settings(settings)?;
        assert_eq!(db.settings()?, settings);
        settings.environment_assessments_enabled = false;
        settings.home_environment = HomeEnvironment::default();
        db.save_settings(settings)?;
        assert_eq!(db.settings()?.home_environment, HomeEnvironment::default());

        db.set_plant_environment(
            id,
            PlantEnvironment {
                exposure: Some(SunExposure::PartialSun),
                temperature_c: Some(21.0),
                humidity_percent: Some(60.0),
            },
        )?;
        assert_eq!(
            db.plants()?[0].environment.exposure,
            Some(SunExposure::PartialSun)
        );
        db.set_plant_environment(id, PlantEnvironment::default())?;
        assert!(db.plants()?[0].environment.is_empty());
        Ok(())
    }

    #[test]
    fn readings_enforce_scope_freshness_precedence_summaries_and_deletion() -> Result<()> {
        let mut db = Database::memory()?;
        let taxon = db.taxa()?.remove(0);
        let id = db.add_plant(&taxon, &PlantInput::default(), 10 * DAY)?;
        let mut settings = db.settings()?;
        settings.home_environment = HomeEnvironment {
            temperature_unit: Some(TemperatureUnit::Celsius),
            baseline_temperature_c: Some(19.0),
            baseline_humidity_percent: Some(45.0),
            ..Default::default()
        };
        db.save_settings(settings)?;
        db.set_plant_environment(
            id,
            PlantEnvironment {
                temperature_c: Some(21.0),
                ..Default::default()
            },
        )?;
        let now = 20 * DAY;
        db.log_environment(
            EnvironmentReadingInput {
                plant_id: None,
                scope: EnvironmentScope::Home,
                source: EnvironmentSource::Import,
                observed_at: now - 60,
                temperature_c: Some(23.0),
                humidity_percent: None,
                lux: Some(500.0),
                ppfd: None,
            },
            now,
        )?;
        db.log_environment(
            EnvironmentReadingInput {
                plant_id: Some(id),
                scope: EnvironmentScope::Plant,
                source: EnvironmentSource::Manual,
                observed_at: now - DAY - 1,
                temperature_c: Some(27.0),
                humidity_percent: None,
                lux: None,
                ppfd: Some(80.0),
            },
            now,
        )?;
        for (age, humidity, lux) in [(120, 60.0, 700.0), (30, 62.0, 900.0)] {
            db.log_environment(
                EnvironmentReadingInput {
                    plant_id: Some(id),
                    scope: EnvironmentScope::Plant,
                    source: EnvironmentSource::Manual,
                    observed_at: now - age,
                    temperature_c: None,
                    humidity_percent: Some(humidity),
                    lux: Some(lux),
                    ppfd: None,
                },
                now,
            )?;
        }

        let snapshot = db.environment_snapshot(id, now)?;
        assert_eq!(snapshot.temperature.map(|value| value.value), Some(21.0));
        assert_eq!(
            snapshot.latest_temperature_reading.map(|value| value.value),
            Some(27.0)
        );
        assert_eq!(snapshot.humidity.map(|value| value.value), Some(62.0));
        assert_eq!(snapshot.lux.map(|value| value.value), Some(900.0));
        assert!(snapshot.ppfd.is_none());
        assert_eq!(
            snapshot.latest_ppfd_reading.map(|value| value.value),
            Some(80.0)
        );
        assert_eq!(snapshot.daily.humidity.unwrap().count, 2);
        assert_eq!(snapshot.daily.lux.unwrap().count, 2);
        assert!(
            db.environment_summary(EnvironmentScope::Plant, Some(id), now, 8)
                .is_err()
        );
        assert!(
            db.log_environment(
                EnvironmentReadingInput {
                    plant_id: Some(id),
                    scope: EnvironmentScope::Plant,
                    source: EnvironmentSource::Manual,
                    observed_at: now,
                    temperature_c: None,
                    humidity_percent: None,
                    lux: None,
                    ppfd: None,
                },
                now,
            )
            .is_err()
        );

        db.remove_plant(id)?;
        let plant_readings: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM environment_readings WHERE scope='plant'",
            [],
            |row| row.get(0),
        )?;
        let home_readings: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM environment_readings WHERE scope='home'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(plant_readings, 0);
        assert_eq!(home_readings, 1);
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
