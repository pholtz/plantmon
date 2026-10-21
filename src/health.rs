use crate::{
    knowledge::{EnvironmentalRequirements, NumericTarget},
    model::{EnvironmentSnapshot, EnvironmentValue, EnvironmentValueSource, SunExposure},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Unknown,
    BelowTarget,
    WithinTarget,
    AboveTarget,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::BelowTarget => "below target",
            Self::WithinTarget => "within target",
            Self::AboveTarget => "above target",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricAssessment {
    pub status: Status,
    pub recommendation: Option<String>,
    pub input_context: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentAssessment {
    pub light: MetricAssessment,
    pub temperature: MetricAssessment,
    pub humidity: MetricAssessment,
    pub requirements_available: bool,
}

/// Returns no assessment at all while the opt-in setting is disabled. Inputs remain
/// persisted and displayable independently of this calculation.
pub fn assess(
    enabled: bool,
    requirements: Option<&EnvironmentalRequirements>,
    exposure: Option<SunExposure>,
    environment: &EnvironmentSnapshot,
    now: i64,
) -> Option<EnvironmentAssessment> {
    if !enabled {
        return None;
    }
    let Some(requirements) = requirements else {
        let unsupported = MetricAssessment {
            status: Status::Unknown,
            recommendation: None,
            input_context: None,
        };
        return Some(EnvironmentAssessment {
            light: unsupported.clone(),
            temperature: unsupported.clone(),
            humidity: unsupported,
            requirements_available: false,
        });
    };

    let light = match exposure {
        Some(value) => {
            let status = if value.level() < requirements.exposure.minimum.level() {
                Status::BelowTarget
            } else if value.level() > requirements.exposure.maximum.level() {
                Status::AboveTarget
            } else {
                Status::WithinTarget
            };
            MetricAssessment {
                status,
                recommendation: Some(match status {
                    Status::BelowTarget => "Estimated below the sourced light range.".into(),
                    Status::WithinTarget => {
                        "Estimated exposure is within the sourced light range.".into()
                    }
                    Status::AboveTarget => "Estimated above the sourced light range.".into(),
                    Status::Unknown => unreachable!(),
                }),
                input_context: Some("plant setting".into()),
            }
        }
        None => unknown("No explicit five-level sun exposure is configured."),
    };
    let temperature = numeric_assessment(
        environment.temperature,
        &requirements.temperature_c,
        "temperature",
        "Cooler than preferred.",
        "Temperature is within the sourced preferred range.",
        "Warmer than preferred.",
        now,
    );
    let humidity = match &requirements.humidity_percent {
        Some(target) => numeric_assessment(
            environment.humidity,
            target,
            "humidity",
            "Drier than the sourced humidity range.",
            "Humidity is within the sourced range.",
            "More humid than the sourced range.",
            now,
        ),
        None => unknown("No numeric sourced humidity target is available."),
    };

    Some(EnvironmentAssessment {
        light,
        temperature,
        humidity,
        requirements_available: true,
    })
}

fn numeric_assessment(
    input: Option<EnvironmentValue>,
    target: &NumericTarget,
    metric: &str,
    below: &str,
    within: &str,
    above: &str,
    now: i64,
) -> MetricAssessment {
    let Some(input) = input else {
        return unknown(&format!("No current {metric} input is configured."));
    };
    let status = if input.value < target.minimum {
        Status::BelowTarget
    } else if target.maximum.is_some_and(|maximum| input.value > maximum) {
        Status::AboveTarget
    } else {
        Status::WithinTarget
    };
    MetricAssessment {
        status,
        recommendation: Some(
            match status {
                Status::BelowTarget => below,
                Status::WithinTarget => within,
                Status::AboveTarget => above,
                Status::Unknown => unreachable!(),
            }
            .into(),
        ),
        input_context: Some(value_context(input, now)),
    }
}

fn unknown(message: &str) -> MetricAssessment {
    MetricAssessment {
        status: Status::Unknown,
        recommendation: Some(message.into()),
        input_context: None,
    }
}

pub fn value_context(input: EnvironmentValue, now: i64) -> String {
    let scope = input.scope.label().to_lowercase();
    let source = match input.source {
        EnvironmentValueSource::Baseline => "baseline".to_string(),
        EnvironmentValueSource::Override => "override".to_string(),
        EnvironmentValueSource::Reading(source) => format!("{} reading", source.key()),
    };
    match input.observed_at {
        Some(observed_at) => format!(
            "{scope} {source} · {} old",
            age(now.saturating_sub(observed_at).max(0))
        ),
        None => format!("{scope} {source}"),
    }
}

pub fn age(seconds: i64) -> String {
    if seconds < 60 {
        "<1m".into()
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h", seconds / 3_600)
    } else {
        format!("{}d", seconds / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        knowledge::{ExposureTarget, NumericTarget},
        model::{EnvironmentScope, EnvironmentSource},
    };

    fn requirements() -> EnvironmentalRequirements {
        EnvironmentalRequirements {
            key: "Example".into(),
            exposure: ExposureTarget {
                minimum: SunExposure::MostlyShade,
                maximum: SunExposure::PartialSun,
                source_ids: vec!["source".into()],
            },
            temperature_c: NumericTarget {
                minimum: 18.0,
                maximum: Some(24.0),
                source_ids: vec!["source".into()],
            },
            humidity_percent: Some(NumericTarget {
                minimum: 60.0,
                maximum: None,
                source_ids: vec!["source".into()],
            }),
            reviewed_on: "2026-10-19".into(),
            content_version: 1,
            fallback_group: None,
        }
    }

    fn reading(value: f64) -> EnvironmentValue {
        EnvironmentValue {
            value,
            scope: EnvironmentScope::Plant,
            source: EnvironmentValueSource::Reading(EnvironmentSource::Manual),
            observed_at: Some(1_000),
        }
    }

    #[test]
    fn assessments_are_opt_in_independent_and_conservative() {
        let snapshot = EnvironmentSnapshot {
            temperature: Some(reading(26.0)),
            humidity: Some(reading(60.0)),
            ..Default::default()
        };
        assert!(assess(false, Some(&requirements()), None, &snapshot, 2_000).is_none());

        let result = assess(
            true,
            Some(&requirements()),
            Some(SunExposure::FullShade),
            &snapshot,
            2_000,
        )
        .unwrap();
        assert_eq!(result.light.status, Status::BelowTarget);
        assert_eq!(result.temperature.status, Status::AboveTarget);
        assert_eq!(result.humidity.status, Status::WithinTarget);
        assert!(result.temperature.input_context.unwrap().contains("manual"));
    }

    #[test]
    fn unsupported_taxa_and_missing_inputs_stay_unknown() {
        let snapshot = EnvironmentSnapshot::default();
        let unsupported = assess(true, None, None, &snapshot, 0).unwrap();
        assert!(!unsupported.requirements_available);
        assert_eq!(unsupported.light.status, Status::Unknown);

        let supported = assess(true, Some(&requirements()), None, &snapshot, 0).unwrap();
        assert_eq!(supported.light.status, Status::Unknown);
        assert_eq!(supported.temperature.status, Status::Unknown);
        assert_eq!(supported.humidity.status, Status::Unknown);
    }
}
