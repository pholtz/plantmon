use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};

use crate::model::Plant;

/// A local-clock daylight estimate. Latitude changes only the seasonal astronomical
/// day length; longitude, weather, horizons, and window transmission are unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Daylight {
    Day,
    Night,
}

impl Daylight {
    /// The location-free fallback deliberately preserves the original 6am–6pm behavior.
    pub fn at(timestamp: i64) -> Self {
        Self::at_with_latitude(timestamp, None)
    }

    pub fn at_with_latitude(timestamp: i64, latitude: Option<f64>) -> Self {
        let Some(local) =
            DateTime::from_timestamp(timestamp, 0).map(|date| date.with_timezone(&Local))
        else {
            return Self::Night;
        };
        let Some(latitude) = latitude else {
            return if (6..18).contains(&local.hour()) {
                Self::Day
            } else {
                Self::Night
            };
        };
        let Some(hours) = daylight_hours(latitude, local.date_naive()) else {
            return Self::at(timestamp);
        };
        if hours >= 24.0 {
            return Self::Day;
        }
        if hours <= 0.0 {
            return Self::Night;
        }
        // Without longitude, center the astronomical duration on local noon. This is
        // intentionally an approximation rather than a claimed sunrise time.
        let clock =
            local.hour() as f64 + local.minute() as f64 / 60.0 + local.second() as f64 / 3600.0;
        let half_day = hours / 2.0;
        if clock >= 12.0 - half_day && clock < 12.0 + half_day {
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

/// Approximate astronomical day length for a signed latitude and local calendar date.
/// The result is bounded to 0–24 hours and explicitly handles polar day and night.
pub fn daylight_hours(latitude: f64, date: NaiveDate) -> Option<f64> {
    if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
        return None;
    }
    let day = date.ordinal() as f64;
    let declination = (23.44_f64.to_radians()
        * (std::f64::consts::TAU * (284.0 + day) / 365.0).sin())
    .clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
    let cosine_hour_angle = -latitude.to_radians().tan() * declination.tan();
    let hours = if cosine_hour_angle >= 1.0 {
        0.0
    } else if cosine_hour_angle <= -1.0 {
        24.0
    } else {
        24.0 * cosine_hour_angle.acos() / std::f64::consts::PI
    };
    Some(hours.clamp(0.0, 24.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exposure {
    FullSun,
    PartSun,
    Shade,
}

impl Exposure {
    pub fn for_plant(plant: &Plant) -> Self {
        // This parser drives artwork only. Health assessments use only the explicit,
        // typed five-level environment setting and never catalog or location prose.
        Self::explicit(&plant.location.to_lowercase())
            .or_else(|| Self::explicit(&plant.taxon.light.to_lowercase()))
            .unwrap_or(Self::PartSun)
    }

    fn explicit(text: &str) -> Option<Self> {
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
    fn location_free_daylight_keeps_exact_local_boundaries() {
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

    #[test]
    fn astronomical_day_length_is_bounded_at_equator_and_poles() {
        let equinox = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let summer = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
        let winter = NaiveDate::from_ymd_opt(2026, 12, 21).unwrap();
        assert!((daylight_hours(0.0, equinox).unwrap() - 12.0).abs() < 0.1);
        assert_eq!(daylight_hours(90.0, summer), Some(24.0));
        assert_eq!(daylight_hours(90.0, winter), Some(0.0));
        assert!(daylight_hours(90.1, equinox).is_none());
        assert!(daylight_hours(f64::NAN, equinox).is_none());
    }
}
