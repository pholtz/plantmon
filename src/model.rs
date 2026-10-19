use anyhow::{Result, bail};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

pub const DAY: i64 = 86_400;
pub const MAX_INTERVAL: i64 = 3650;

#[derive(Clone, Debug)]
pub struct Taxon {
    pub id: String,
    pub genus: String,
    pub species: String,
    pub designation: String,
    pub common: String,
    pub family: String,
    pub groups: String,
    pub habit: String,
    pub water_days: i64,
    pub light: String,
    pub care: String,
}

impl Taxon {
    pub fn botanical(genus: &str, species: &str, designation: &str) -> String {
        [genus.trim(), species.trim(), designation.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {} {}",
            self.id, self.common, self.family, self.groups
        )
        .to_lowercase();
        query
            .to_lowercase()
            .split_whitespace()
            .all(|word| haystack.contains(word))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    Water,
    Rotate,
    Fertilize,
    Soil,
    Inspect,
}

impl TaskKind {
    pub const ALL: [Self; 5] = [
        Self::Water,
        Self::Rotate,
        Self::Fertilize,
        Self::Soil,
        Self::Inspect,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Water => "water",
            Self::Rotate => "rotate",
            Self::Fertilize => "fertilize",
            Self::Soil => "soil",
            Self::Inspect => "inspect",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Water => "Check / add water",
            Self::Rotate => "Rotate pot",
            Self::Fertilize => "Fertilize",
            Self::Soil => "Refresh soil",
            Self::Inspect => "Inspect / clean",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.key() == value)
            .ok_or_else(|| anyhow::anyhow!("Unknown maintenance task: {value}"))
    }
}

#[derive(Clone, Debug)]
pub struct Schedule {
    pub kind: TaskKind,
    pub interval_days: i64,
    pub initial_due_at: i64,
    pub last_completed: Option<i64>,
}

impl Schedule {
    pub fn due_at(&self) -> Option<i64> {
        (self.interval_days > 0).then(|| {
            self.last_completed
                .map(|last| last + self.interval_days * DAY)
                .unwrap_or(self.initial_due_at)
        })
    }

    pub fn water_level(&self, now: i64) -> Option<f64> {
        if self.interval_days == 0 {
            return None;
        }
        self.last_completed.map(|last| {
            (1.0 - (now - last).max(0) as f64 / (self.interval_days * DAY) as f64).clamp(0.0, 1.0)
        })
    }
}

#[derive(Clone, Debug)]
pub struct Plant {
    pub id: i64,
    pub display_order: i64,
    pub taxon: Taxon,
    pub nickname: String,
    pub location: String,
    pub acquired: String,
    pub pot: String,
    pub soil: String,
    pub tags: String,
    pub notes: String,
    pub schedules: Vec<Schedule>,
}

impl Plant {
    pub fn name(&self) -> &str {
        if self.nickname.is_empty() {
            self.taxon
                .common
                .split(';')
                .next()
                .unwrap_or(&self.taxon.common)
                .trim()
        } else {
            &self.nickname
        }
    }

    pub fn water(&self) -> &Schedule {
        self.schedules
            .iter()
            .find(|s| s.kind == TaskKind::Water)
            .expect("all plants have a water schedule")
    }

    pub fn due_count(&self, now: i64) -> usize {
        self.schedules
            .iter()
            .filter(|s| s.due_at().is_some_and(|due| due <= now))
            .count()
    }

    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {} {} {} {} {}",
            self.nickname,
            self.location,
            self.tags,
            self.taxon.id,
            self.taxon.common,
            self.taxon.family,
            self.taxon.groups
        )
        .to_lowercase();
        query
            .to_lowercase()
            .split_whitespace()
            .all(|word| haystack.contains(word))
    }
}

#[derive(Clone, Debug)]
pub struct Maintenance {
    pub id: i64,
    pub kind: TaskKind,
    pub occurred_at: i64,
    pub note: String,
}

pub fn now() -> i64 {
    Utc::now().timestamp()
}

pub fn date(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "invalid date".into())
}

pub fn countdown(due: i64, now: i64) -> String {
    let seconds = due - now;
    let magnitude = seconds.unsigned_abs();
    let span = if magnitude >= DAY as u64 {
        format!(
            "{}d {}h",
            magnitude / DAY as u64,
            magnitude % DAY as u64 / 3600
        )
    } else if magnitude >= 3600 {
        format!("{}h {}m", magnitude / 3600, magnitude % 3600 / 60)
    } else {
        format!("{}m {}s", magnitude / 60, magnitude % 60)
    };
    if seconds <= 0 {
        format!("{span} overdue")
    } else {
        format!("in {span}")
    }
}

/// Relative times are elapsed durations; calendar dates use the terminal's local timezone.
pub fn parse_when(value: &str, now: i64) -> Result<i64> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("now") {
        return Ok(now);
    }
    if value.eq_ignore_ascii_case("yesterday") {
        return Ok(now - DAY);
    }
    if let Some(unit) = value
        .chars()
        .last()
        .filter(|c| matches!(c, 'd' | 'h' | 'm'))
    {
        let amount: i64 = value[..value.len() - 1]
            .parse()
            .map_err(|_| anyhow::anyhow!("Use a date, now, yesterday, 2d, 12h, or 30m"))?;
        let multiplier = match unit {
            'd' => DAY,
            'h' => 3600,
            _ => 60,
        };
        if amount < 0 {
            bail!("Backdating must be a positive duration");
        }
        return now
            .checked_sub(
                amount
                    .checked_mul(multiplier)
                    .ok_or_else(|| anyhow::anyhow!("Duration is too large"))?,
            )
            .filter(|ts| DateTime::from_timestamp(*ts, 0).is_some())
            .ok_or_else(|| anyhow::anyhow!("Date is out of range"));
    }
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M")
        .or_else(|_| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .map_err(|_| anyhow::anyhow!("Use YYYY-MM-DD [HH:MM], now, yesterday, 2d, or 12h"))?;
    let timestamp = Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Ambiguous or nonexistent local time (daylight saving); use a relative duration"
            )
        })?
        .timestamp();
    if timestamp > now {
        bail!("Maintenance cannot be recorded in the future");
    }
    Ok(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_is_unknown_until_logged_and_decays_without_a_running_process() {
        let mut schedule = Schedule {
            kind: TaskKind::Water,
            interval_days: 10,
            initial_due_at: 100,
            last_completed: None,
        };
        assert_eq!(schedule.water_level(100), None);
        assert_eq!(schedule.due_at(), Some(100));
        schedule.last_completed = Some(100);
        assert_eq!(schedule.water_level(100 + 5 * DAY), Some(0.5));
        assert_eq!(schedule.water_level(100 + 20 * DAY), Some(0.0));
        schedule.interval_days = 0;
        assert_eq!(schedule.due_at(), None);
        assert_eq!(schedule.water_level(100), None);
    }

    #[test]
    fn relative_backdates_reject_future_negative_and_overflow() {
        let current = 1_800_000_000;
        assert_eq!(parse_when("2d", current).unwrap(), current - 2 * DAY);
        assert_eq!(parse_when("12h", current).unwrap(), current - 43_200);
        assert!(parse_when("-1d", current).is_err());
        assert!(parse_when("999999999999999999d", current).is_err());
        assert!(parse_when("2099-01-01", current).is_err());
    }
}
