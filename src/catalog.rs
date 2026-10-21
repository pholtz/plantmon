use crate::model::{MAX_INTERVAL, Taxon};
use anyhow::{Context, Result, ensure};
use std::collections::HashSet;

pub const CATALOG_TITLE: &str = "Catalog";
#[allow(dead_code)]
pub const FIELD_GUIDE_TITLE: &str = "Field Guide";

pub fn builtin() -> Result<Vec<Taxon>> {
    let mut seen = HashSet::new();
    include_str!("../data/catalog.tsv")
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('#') && !line.is_empty())
        .map(|(index, line)| {
            let c: Vec<_> = line.split('\t').collect();
            ensure!(
                c.len() == 10,
                "Catalog row {} needs 10 tab-separated columns",
                index + 1
            );
            let id = Taxon::botanical(c[0], c[1], c[2]);
            ensure!(seen.insert(id.clone()), "Duplicate catalog taxon: {id}");
            ensure!(
                !c[0].is_empty() && !c[3].is_empty() && !c[4].is_empty(),
                "Incomplete taxon: {id}"
            );
            ensure!(
                crate::art::HABITS.contains(&c[6]),
                "Unknown habit: {}",
                c[6]
            );
            let water_days: i64 = c[7].parse().context("Catalog water interval")?;
            ensure!(
                (1..=MAX_INTERVAL).contains(&water_days),
                "Invalid interval for {id}"
            );
            Ok(Taxon {
                id,
                genus: c[0].into(),
                species: c[1].into(),
                designation: c[2].into(),
                common: c[3].into(),
                family: c[4].into(),
                groups: c[5].into(),
                habit: c[6].into(),
                water_days,
                light: c[8].into(),
                care: c[9].into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_valid_and_searches_names_cultivars_and_groups() {
        let taxa = builtin().unwrap();
        assert!(taxa.len() >= 150);
        for query in [
            "Thai Constellation",
            "swiss cheese",
            "aroids",
            "cacti",
            "succulents",
            "ficus",
            "hoyas",
            "hybrid",
            "Aphelandra squarrosa",
            "calathea jungle velvet",
            "Calathea warszewiczii",
        ] {
            assert!(taxa.iter().any(|t| t.matches(query)), "Missing {query}");
        }
        assert!(taxa.iter().any(|t| t.genus == "Monstera"
            && t.species == "deliciosa"
            && t.designation.contains("Thai")));
        let zebra = taxa
            .iter()
            .find(|taxon| taxon.id == "Aphelandra squarrosa")
            .expect("Missing zebra plant");
        assert!(zebra.matches("zebra plant"));
        assert!(zebra.matches("saffron spike zebra"));
        assert_eq!(zebra.family, "Acanthaceae");
        let jungle_velvet = taxa
            .iter()
            .find(|taxon| taxon.id == "Goeppertia warszewiczii")
            .expect("Missing jungle velvet calathea");
        assert!(jungle_velvet.matches("velvet calathea"));
        assert_eq!(jungle_velvet.family, "Marantaceae");
    }
}
