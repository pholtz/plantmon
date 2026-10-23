use anyhow::{Result, bail};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

pub const DAY: i64 = 86_400;
pub const MAX_INTERVAL: i64 = 3650;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SunExposure {
    FullShade,
    MostlyShade,
    PartialSun,
    MostlySun,
    FullSun,
}

impl SunExposure {
    pub const ALL: [Self; 5] = [
        Self::FullShade,
        Self::MostlyShade,
        Self::PartialSun,
        Self::MostlySun,
        Self::FullSun,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::FullShade => "full_shade",
            Self::MostlyShade => "mostly_shade",
            Self::PartialSun => "partial_sun",
            Self::MostlySun => "mostly_sun",
            Self::FullSun => "full_sun",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FullShade => "Full shade",
            Self::MostlyShade => "Mostly shade",
            Self::PartialSun => "Partial sun",
            Self::MostlySun => "Mostly sun",
            Self::FullSun => "Full sun",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|exposure| exposure.key() == value)
    }

    pub fn level(self) -> u8 {
        Self::ALL
            .iter()
            .position(|exposure| *exposure == self)
            .expect("exposure is in ALL") as u8
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlantEnvironment {
    pub exposure: Option<SunExposure>,
    pub temperature_c: Option<f64>,
    pub humidity_percent: Option<f64>,
}

impl PlantEnvironment {
    pub fn is_empty(self) -> bool {
        self.exposure.is_none() && self.temperature_c.is_none() && self.humidity_percent.is_none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentScope {
    Home,
    Plant,
}

impl EnvironmentScope {
    pub fn key(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Plant => "plant",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Plant => "Plant",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "home" => Some(Self::Home),
            "plant" => Some(Self::Plant),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentSource {
    Manual,
    Import,
    Sensor,
}

impl EnvironmentSource {
    pub fn key(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Import => "import",
            Self::Sensor => "sensor",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "import" => Some(Self::Import),
            "sensor" => Some(Self::Sensor),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentValueSource {
    Baseline,
    Override,
    Reading(EnvironmentSource),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentValue {
    pub value: f64,
    pub scope: EnvironmentScope,
    pub source: EnvironmentValueSource,
    pub observed_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MetricSummary {
    pub count: usize,
    pub minimum: f64,
    pub maximum: f64,
    pub mean: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvironmentSummary {
    pub temperature: Option<MetricSummary>,
    pub humidity: Option<MetricSummary>,
    pub lux: Option<MetricSummary>,
    pub ppfd: Option<MetricSummary>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnvironmentSnapshot {
    pub temperature: Option<EnvironmentValue>,
    pub humidity: Option<EnvironmentValue>,
    pub lux: Option<EnvironmentValue>,
    pub ppfd: Option<EnvironmentValue>,
    pub latest_temperature_reading: Option<EnvironmentValue>,
    pub latest_humidity_reading: Option<EnvironmentValue>,
    pub latest_lux_reading: Option<EnvironmentValue>,
    pub latest_ppfd_reading: Option<EnvironmentValue>,
    pub daily: EnvironmentSummary,
    pub weekly: EnvironmentSummary,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentReadingInput {
    pub plant_id: Option<i64>,
    pub scope: EnvironmentScope,
    pub source: EnvironmentSource,
    pub observed_at: i64,
    pub temperature_c: Option<f64>,
    pub humidity_percent: Option<f64>,
    pub lux: Option<f64>,
    pub ppfd: Option<f64>,
}
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

macro_rules! inspection_enum {
    (
        $name:ident {
            $($variant:ident => ($key:literal, $label:literal)),+ $(,)?
        }
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key),+
                }
            }

            pub fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label),+
                }
            }

            pub fn parse(value: &str) -> Result<Self> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|candidate| candidate.key() == value)
                    .ok_or_else(|| anyhow::anyhow!(
                        "Unknown {}: {value}",
                        stringify!($name)
                    ))
            }
        }
    };
}

inspection_enum! {
    InspectionFindingKind {
        NoAdverseSigns => ("no_adverse_signs", "No adverse signs"),
        NewLeaf => ("new_leaf", "New leaf"),
        NewShoot => ("new_shoot", "New shoot"),
        NewBranch => ("new_branch", "New branch"),
        NewRoot => ("new_root", "New root"),
        OffsetOrPup => ("offset_or_pup", "Offset / pup"),
        Budding => ("budding", "Budding"),
        Flowering => ("flowering", "Flowering"),
        Fruiting => ("fruiting", "Fruiting"),
        Recovery => ("recovery", "Recovery / improvement"),
        LeafYellowing => ("leaf_yellowing", "Leaf yellowing / chlorosis"),
        LeafBrowning => ("leaf_browning", "Leaf browning / necrosis"),
        LeafBlackening => ("leaf_blackening", "Leaf blackening"),
        LeafPalingOrBleaching => ("leaf_paling_or_bleaching", "Leaf paling / bleaching"),
        LeafSpotsOrLesions => ("leaf_spots_or_lesions", "Leaf spots / lesions"),
        LeafTipBurn => ("leaf_tip_burn", "Leaf tip burn"),
        LeafEdgeBurn => ("leaf_edge_burn", "Leaf edge burn"),
        LeafScorch => ("leaf_scorch", "Leaf scorch"),
        LeafWiltingOrDrooping => ("leaf_wilting_or_drooping", "Leaf wilting / drooping"),
        LeafCurlingOrCupping => ("leaf_curling_or_cupping", "Leaf curling / cupping"),
        LeafShrivelingOrWrinkling => ("leaf_shriveling_or_wrinkling", "Leaf shriveling / wrinkling"),
        LeafDrop => ("leaf_drop", "Leaf drop"),
        DistortedGrowth => ("distorted_growth", "Distorted growth"),
        StuckUnfurling => ("stuck_unfurling", "Stuck unfurling"),
        EdemaOrBlistering => ("edema_or_blistering", "Edema / blistering"),
        PhysicalDamage => ("physical_damage", "Physical damage"),
        Dieback => ("dieback", "Dieback"),
        StalledGrowth => ("stalled_growth", "Stalled growth"),
        Etiolation => ("etiolation", "Etiolation / stretching"),
        WeakOrFloppyGrowth => ("weak_or_floppy_growth", "Weak / floppy growth"),
        LeaningOrInstability => ("leaning_or_instability", "Leaning / instability"),
        StemShriveling => ("stem_shriveling", "Stem shriveling"),
        StemSoftening => ("stem_softening", "Stem softening"),
        StemLesionOrDiscoloration => ("stem_lesion_or_discoloration", "Stem lesion / discoloration"),
        StemBreakage => ("stem_breakage", "Stem breakage"),
        CrownSofteningOrCollapse => ("crown_softening_or_collapse", "Crown softening / collapse"),
        RootBoundOrCircling => ("root_bound_or_circling", "Root-bound / circling"),
        RootsExitingPot => ("roots_exiting_pot", "Roots exiting pot"),
        RootDryOrBrittle => ("root_dry_or_brittle", "Dry / brittle roots"),
        RootDarkOrMushy => ("root_dark_or_mushy", "Dark / mushy roots"),
        RootLoss => ("root_loss", "Root loss"),
        RootDamage => ("root_damage", "Root damage"),
        AbnormalRootOdor => ("abnormal_root_odor", "Abnormal root odor"),
        PestObserved => ("pest_observed", "Pest observed"),
        EggsOrLarvae => ("eggs_or_larvae", "Eggs / larvae"),
        Webbing => ("webbing", "Webbing"),
        CottonyResidue => ("cottony_residue", "Cottony residue"),
        ScaleLikeBumps => ("scale_like_bumps", "Scale-like bumps"),
        StipplingOrSilvering => ("stippling_or_silvering", "Stippling / silvering"),
        ChewingOrMining => ("chewing_or_mining", "Chewing / mining"),
        HoneydewOrStickiness => ("honeydew_or_stickiness", "Honeydew / stickiness"),
        Frass => ("frass", "Frass"),
        PowderyGrowth => ("powdery_growth", "Powdery growth"),
        FuzzyMold => ("fuzzy_mold", "Fuzzy mold"),
        WaterSoakedTissue => ("water_soaked_tissue", "Water-soaked tissue"),
        RustLikePustules => ("rust_like_pustules", "Rust-like pustules"),
        Ooze => ("ooze", "Ooze"),
        FoulOdor => ("foul_odor", "Foul odor"),
        SubstrateDry => ("substrate_dry", "Substrate dry"),
        SubstrateSaturated => ("substrate_saturated", "Substrate saturated"),
        HydrophobicSubstrate => ("hydrophobic_substrate", "Hydrophobic substrate"),
        CompactedSubstrate => ("compacted_substrate", "Compacted substrate"),
        MoldOrAlgae => ("mold_or_algae", "Mold / algae"),
        FungalFruitingBody => ("fungal_fruiting_body", "Fungal fruiting body"),
        SaltOrMineralBuildup => ("salt_or_mineral_buildup", "Salt / mineral buildup"),
        StandingWater => ("standing_water", "Standing water"),
        PoorDrainage => ("poor_drainage", "Poor drainage"),
        DamagedContainer => ("damaged_container", "Damaged container"),
        SupportOrStakingIssue => ("support_or_staking_issue", "Support / staking issue"),
        BudDrop => ("bud_drop", "Bud drop"),
        FlowerDrop => ("flower_drop", "Flower drop"),
        BloomDamage => ("bloom_damage", "Bloom damage"),
        FruitDamageOrRot => ("fruit_damage_or_rot", "Fruit damage / rot")
    }
}

impl InspectionFindingKind {
    pub fn category(self) -> &'static str {
        use InspectionFindingKind::*;
        match self {
            NoAdverseSigns | NewLeaf | NewShoot | NewBranch | NewRoot | OffsetOrPup | Budding
            | Flowering | Fruiting | Recovery => "positive",
            LeafYellowing
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
            | DistortedGrowth
            | StuckUnfurling
            | EdemaOrBlistering
            | PhysicalDamage
            | Dieback => "foliage",
            StalledGrowth
            | Etiolation
            | WeakOrFloppyGrowth
            | LeaningOrInstability
            | StemShriveling
            | StemSoftening
            | StemLesionOrDiscoloration
            | StemBreakage
            | CrownSofteningOrCollapse => "growth / stem",
            RootBoundOrCircling | RootsExitingPot | RootDryOrBrittle | RootDarkOrMushy
            | RootLoss | RootDamage | AbnormalRootOdor => "roots",
            PestObserved | EggsOrLarvae | Webbing | CottonyResidue | ScaleLikeBumps
            | StipplingOrSilvering | ChewingOrMining | HoneydewOrStickiness | Frass
            | PowderyGrowth | FuzzyMold | WaterSoakedTissue | RustLikePustules | Ooze
            | FoulOdor => "pest / disease signs",
            SubstrateDry
            | SubstrateSaturated
            | HydrophobicSubstrate
            | CompactedSubstrate
            | MoldOrAlgae
            | FungalFruitingBody
            | SaltOrMineralBuildup
            | StandingWater
            | PoorDrainage
            | DamagedContainer
            | SupportOrStakingIssue => "substrate / container",
            BudDrop | FlowerDrop | BloomDamage | FruitDamageOrRot => "flower / fruit",
        }
    }

    pub fn is_concern(self) -> bool {
        !matches!(
            self,
            Self::NoAdverseSigns
                | Self::NewLeaf
                | Self::NewShoot
                | Self::NewBranch
                | Self::NewRoot
                | Self::OffsetOrPup
                | Self::Budding
                | Self::Flowering
                | Self::Fruiting
                | Self::Recovery
        )
    }
}

inspection_enum! {
    PlantPart {
        WholePlant => ("whole_plant", "Whole plant"),
        Leaf => ("leaf", "Leaf"),
        NewGrowth => ("new_growth", "New growth"),
        Stem => ("stem", "Stem"),
        Crown => ("crown", "Crown"),
        Root => ("root", "Root"),
        Flower => ("flower", "Flower"),
        Fruit => ("fruit", "Fruit"),
        Substrate => ("substrate", "Substrate"),
        Container => ("container", "Container")
    }
}

inspection_enum! {
    InspectionSeverity {
        Mild => ("mild", "Mild"),
        Moderate => ("moderate", "Moderate"),
        Severe => ("severe", "Severe")
    }
}

inspection_enum! {
    InspectionExtent {
        Single => ("single", "Single"),
        Few => ("few", "Few"),
        Localized => ("localized", "Localized"),
        Widespread => ("widespread", "Widespread")
    }
}

inspection_enum! {
    InspectionTrend {
        New => ("new", "New"),
        Improving => ("improving", "Improving"),
        Stable => ("stable", "Stable"),
        Worsening => ("worsening", "Worsening"),
        Resolved => ("resolved", "Resolved")
    }
}

inspection_enum! {
    PestKind {
        Unknown => ("unknown", "Unknown pest"),
        Aphid => ("aphid", "Aphid"),
        Mealybug => ("mealybug", "Mealybug"),
        RootMealybug => ("root_mealybug", "Root mealybug"),
        Scale => ("scale", "Scale"),
        SpiderMite => ("spider_mite", "Spider mite"),
        Thrips => ("thrips", "Thrips"),
        Whitefly => ("whitefly", "Whitefly"),
        FungusGnat => ("fungus_gnat", "Fungus gnat"),
        Caterpillar => ("caterpillar", "Caterpillar"),
        BeetleOrWeevil => ("beetle_or_weevil", "Beetle / weevil"),
        SnailOrSlug => ("snail_or_slug", "Snail / slug"),
        Other => ("other", "Other")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectionFindingInput {
    pub kind: InspectionFindingKind,
    pub part: Option<PlantPart>,
    pub severity: Option<InspectionSeverity>,
    pub extent: Option<InspectionExtent>,
    pub trend: InspectionTrend,
    pub pest: Option<PestKind>,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectionFinding {
    pub id: i64,
    pub kind: InspectionFindingKind,
    pub part: Option<PlantPart>,
    pub severity: Option<InspectionSeverity>,
    pub extent: Option<InspectionExtent>,
    pub trend: InspectionTrend,
    pub pest: Option<PestKind>,
    pub note: String,
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
    pub environment: PlantEnvironment,
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
    pub findings: Vec<InspectionFinding>,
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
