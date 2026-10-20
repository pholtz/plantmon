use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::catalog;

const SCHEMA_VERSION: u32 = 1;
const SOURCES: &str = include_str!("../data/knowledge/sources.toml");
const TAXA: &str = include_str!("../data/knowledge/taxa.toml");
const FAMILIES: &str = include_str!("../data/knowledge/families.toml");
const CARE_GROUPS: &str = include_str!("../data/knowledge/care_groups.toml");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDocument {
    schema_version: u32,
    source: Vec<Source>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileDocument {
    schema_version: u32,
    profile: Vec<Profile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    title: String,
    organization: String,
    url: String,
    kind: SourceKind,
    accessed_on: String,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SourceKind {
    Taxonomy,
    BotanicalReference,
    UniversityExtension,
    HorticulturalSociety,
}

impl SourceKind {
    fn label(self) -> &'static str {
        match self {
            Self::Taxonomy => "taxonomy",
            Self::BotanicalReference => "botanical reference",
            Self::UniversityExtension => "university extension",
            Self::HorticulturalSociety => "horticultural society",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    scope: Scope,
    key: String,
    title: String,
    summary: String,
    caveat: Option<String>,
    reviewed_on: String,
    content_version: u32,
    fact: Vec<Fact>,
}

#[derive(Clone, Copy, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Scope {
    Taxon,
    Family,
    CareGroup,
}

impl Scope {
    fn label(self) -> &'static str {
        match self {
            Self::Taxon => "Taxon",
            Self::Family => "Family",
            Self::CareGroup => "Care group",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fact {
    id: String,
    section: Section,
    topic: String,
    title: String,
    text: String,
    source_ids: Vec<String>,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Section {
    Care,
    Background,
    History,
    Peculiarity,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Self::Care => "CARE",
            Self::Background => "BACKGROUND",
            Self::History => "HISTORY",
            Self::Peculiarity => "PECULIARITIES",
        }
    }
}

struct KnowledgeBase {
    sources: Vec<Source>,
    profiles: Vec<Profile>,
}

pub fn render(query: &str) -> Result<String> {
    let knowledge = bundled()?;
    let matches = knowledge.search(query);
    let mut output = String::new();

    for (profile_index, profile) in matches.iter().enumerate() {
        if profile_index > 0 {
            output.push('\n');
        }
        writeln!(
            output,
            "{} · {}",
            profile.title.to_uppercase(),
            profile.scope.label()
        )?;
        writeln!(output, "{}", profile.summary)?;
        if let Some(caveat) = &profile.caveat {
            writeln!(output, "Scope note: {caveat}")?;
        }

        let mut current_section = None;
        for fact in &profile.fact {
            if current_section != Some(fact.section) {
                current_section = Some(fact.section);
                writeln!(output, "\n{}", fact.section.label())?;
            }
            writeln!(output, "- {}: {}", fact.title, fact.text)?;
        }

        writeln!(
            output,
            "\nReviewed: {} · content version {}",
            profile.reviewed_on, profile.content_version
        )?;
        writeln!(output, "SOURCES")?;
        let mut printed = HashSet::new();
        for source_id in profile
            .fact
            .iter()
            .flat_map(|fact| fact.source_ids.iter())
            .filter(|source_id| printed.insert(source_id.as_str()))
        {
            let source = knowledge
                .sources
                .iter()
                .find(|source| source.id == *source_id)
                .expect("validated source reference");
            writeln!(
                output,
                "- {} · {} ({}, accessed {})\n  {}",
                source.organization,
                source.title,
                source.kind.label(),
                source.accessed_on,
                source.url
            )?;
        }
    }

    Ok(output)
}

fn bundled() -> Result<KnowledgeBase> {
    let sources: SourceDocument =
        toml::from_str(SOURCES).context("Parse data/knowledge/sources.toml")?;
    ensure!(
        sources.schema_version == SCHEMA_VERSION,
        "Unsupported knowledge source schema {}",
        sources.schema_version
    );

    let documents = [
        (
            Scope::Taxon,
            "data/knowledge/taxa.toml",
            parse_profiles(TAXA, "data/knowledge/taxa.toml")?,
        ),
        (
            Scope::Family,
            "data/knowledge/families.toml",
            parse_profiles(FAMILIES, "data/knowledge/families.toml")?,
        ),
        (
            Scope::CareGroup,
            "data/knowledge/care_groups.toml",
            parse_profiles(CARE_GROUPS, "data/knowledge/care_groups.toml")?,
        ),
    ];
    let mut profiles = Vec::new();
    for (expected_scope, path, document) in documents {
        ensure!(
            document.schema_version == SCHEMA_VERSION,
            "Unsupported knowledge schema {} in {path}",
            document.schema_version
        );
        for profile in document.profile {
            ensure!(
                profile.scope == expected_scope,
                "{} has a profile with the wrong scope",
                profile.key
            );
            profiles.push(profile);
        }
    }

    let knowledge = KnowledgeBase {
        sources: sources.source,
        profiles,
    };
    knowledge.validate()?;
    Ok(knowledge)
}

fn parse_profiles(content: &str, path: &str) -> Result<ProfileDocument> {
    toml::from_str(content).with_context(|| format!("Parse {path}"))
}

impl KnowledgeBase {
    fn validate(&self) -> Result<()> {
        let mut source_ids = HashSet::new();
        for source in &self.sources {
            ensure!(
                !source.id.trim().is_empty() && source_ids.insert(source.id.as_str()),
                "Duplicate or empty knowledge source id: {}",
                source.id
            );
            ensure!(
                !source.title.trim().is_empty() && !source.organization.trim().is_empty(),
                "Incomplete knowledge source: {}",
                source.id
            );
            ensure!(
                source.url.starts_with("https://"),
                "Knowledge source must use HTTPS: {}",
                source.id
            );
            parse_date(&source.accessed_on, &source.id)?;
        }

        let taxa = catalog::builtin()?;
        let known_taxa = taxa
            .iter()
            .map(|taxon| taxon.id.as_str())
            .collect::<HashSet<_>>();
        let known_families = taxa
            .iter()
            .map(|taxon| taxon.family.as_str())
            .collect::<HashSet<_>>();
        let known_groups = taxa
            .iter()
            .flat_map(|taxon| taxon.groups.split(',').map(str::trim))
            .collect::<HashSet<_>>();
        let subjects = [
            (Scope::Taxon, known_taxa),
            (Scope::Family, known_families),
            (Scope::CareGroup, known_groups),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();

        let mut profile_keys = HashSet::new();
        let mut fact_ids = HashSet::new();
        for profile in &self.profiles {
            ensure!(
                profile_keys.insert((profile.scope, profile.key.as_str())),
                "Duplicate knowledge profile: {}",
                profile.key
            );
            ensure!(
                subjects[&profile.scope].contains(profile.key.as_str()),
                "Knowledge profile is not present in the catalog: {}",
                profile.key
            );
            ensure!(
                !profile.title.trim().is_empty()
                    && !profile.summary.trim().is_empty()
                    && !profile.fact.is_empty()
                    && profile.content_version > 0,
                "Incomplete knowledge profile: {}",
                profile.key
            );
            parse_date(&profile.reviewed_on, &profile.key)?;

            for fact in &profile.fact {
                ensure!(
                    !fact.id.trim().is_empty() && fact_ids.insert(fact.id.as_str()),
                    "Duplicate or empty knowledge fact id: {}",
                    fact.id
                );
                ensure!(
                    !fact.topic.trim().is_empty()
                        && !fact.title.trim().is_empty()
                        && !fact.text.trim().is_empty()
                        && !fact.source_ids.is_empty(),
                    "Incomplete knowledge fact: {}",
                    fact.id
                );
                for source_id in &fact.source_ids {
                    ensure!(
                        source_ids.contains(source_id.as_str()),
                        "Unknown source {source_id} in fact {}",
                        fact.id
                    );
                }
            }
        }
        Ok(())
    }

    fn search(&self, query: &str) -> Vec<&Profile> {
        let words = query.to_lowercase();
        let words = words.split_whitespace().collect::<Vec<_>>();
        self.profiles
            .iter()
            .filter(|profile| {
                let haystack = format!(
                    "{} {} {} {}",
                    profile.key,
                    profile.title,
                    profile.summary,
                    profile
                        .fact
                        .iter()
                        .map(|fact| format!("{} {} {}", fact.topic, fact.title, fact.text))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_lowercase();
                words.iter().all(|word| haystack.contains(word))
            })
            .collect()
    }
}

fn parse_date(value: &str, owner: &str) -> Result<()> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("Invalid review/access date for {owner}: {value}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_knowledge_is_valid_and_covers_the_pilot_collection() {
        let knowledge = bundled().unwrap();
        for (scope, key) in [
            (Scope::Taxon, "Chlorophytum comosum"),
            (Scope::Taxon, "Pachira aquatica"),
            (Scope::Taxon, "Aphelandra squarrosa"),
            (Scope::Taxon, "Goeppertia insignis"),
            (Scope::Taxon, "Goeppertia warszewiczii"),
            (Scope::Family, "Asparagaceae"),
            (Scope::Family, "Malvaceae"),
            (Scope::Family, "Acanthaceae"),
            (Scope::Family, "Marantaceae"),
            (Scope::CareGroup, "strappy foliage"),
            (Scope::CareGroup, "trees"),
            (Scope::CareGroup, "tropical foliage"),
            (Scope::CareGroup, "flowering"),
            (Scope::CareGroup, "humidity loving"),
            (Scope::CareGroup, "prayer plants"),
        ] {
            assert!(
                knowledge
                    .profiles
                    .iter()
                    .any(|profile| profile.scope == scope && profile.key == key),
                "Missing knowledge profile for {key}"
            );
        }

        let guide = render("jungle velvet").unwrap();
        assert!(guide.contains("GOEPPERTIA WARSZEWICZII"));
        assert!(guide.contains("SOURCES"));
        assert!(guide.contains("https://"));
    }
}
