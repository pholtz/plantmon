use chrono::{DateTime, Local, Timelike};

use crate::model::Plant;

/// A local-clock estimate only: no location, astronomical sunrise, or weather data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Daylight {
    Day,
    Night,
}

impl Daylight {
    pub fn at(timestamp: i64) -> Self {
        let hour =
            DateTime::from_timestamp(timestamp, 0).map(|date| date.with_timezone(&Local).hour());
        if hour.is_some_and(|hour| (6..18).contains(&hour)) {
            Self::Day
        } else {
            Self::Night
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Day => "☀",
            Self::Night => "☾",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exposure {
    FullSun,
    PartSun,
    Shade,
}

impl Exposure {
    pub fn for_plant(plant: &Plant) -> Self {
        // The location/light field can override the catalog's preferred light.
        Self::explicit(&plant.location.to_lowercase())
            .or_else(|| Self::explicit(&plant.taxon.light.to_lowercase()))
            .unwrap_or(Self::PartSun)
    }

    fn explicit(text: &str) -> Option<Self> {
        // Check partial/shaded sun before direct sun to avoid substring ambiguity.
        if [
            "no direct sun",
            "avoid direct sun",
            "part sun",
            "partial sun",
            "part shade",
            "partial shade",
            "gentle sun",
            "morning sun",
            "dappled",
            "filtered",
            "indirect",
        ]
        .iter()
        .any(|word| text.contains(word))
        {
            if text.starts_with("low") || text.contains("low light") {
                Some(Self::Shade)
            } else {
                Some(Self::PartSun)
            }
        } else if [
            "full sun",
            "direct sun",
            "bright sun",
            "acclimated sun",
            "sunny",
            "; sun",
        ]
        .iter()
        .any(|word| text.contains(word))
        {
            Some(Self::FullSun)
        } else if ["shade", "low light", "low to medium"]
            .iter()
            .any(|word| text.contains(word))
        {
            Some(Self::Shade)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn daylight_uses_local_calendar_time_and_exact_day_night_boundaries() {
        for (hour, minute, expected) in [
            (5, 59, Daylight::Night),
            (6, 0, Daylight::Day),
            (17, 59, Daylight::Day),
            (18, 0, Daylight::Night),
        ] {
            let now = Local
                .with_ymd_and_hms(2026, 1, 15, hour, minute, 0)
                .single()
                .unwrap()
                .timestamp();
            assert_eq!(Daylight::at(now), expected);
        }
    }
}
