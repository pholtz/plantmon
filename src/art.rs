use crate::model::Taxon;

pub const HABITS: &[&str] = &[
    "monstera",
    "broadleaf",
    "vine",
    "rosette",
    "cactus",
    "palm",
    "fern",
    "orchid",
    "grass",
];

// Small, terminal-native silhouettes. Unknown taxa inherit their growth habit;
// unknown habits deliberately fall back to a generic leafy plant.
pub fn silhouette(taxon: &Taxon, frame: u64) -> &'static str {
    match taxon.habit.as_str() {
        "monstera" => {
            if frame % 8 < 4 {
                "    ▄██▄  ▄██▄\n   ██ ░█╲╱█░ ██\n    ▀█▄ ╳ ▄█▀\n     ▄█╱│╲█▄\n        │\n     ╭─────╮\n      ╲███╱\n       ▔▔▔"
            } else {
                "   ▄██▄   ▄██▄\n  ██ ░█╲ ╱█░ ██\n   ▀█▄  ╳  ▄█▀\n     ▄█╱│╲█▄\n        │\n     ╭─────╮\n      ╲███╱\n       ▔▔▔"
            }
        }
        "vine" => {
            "     ▄▄   ▄▄\n    ◖██╲ ╱██◗\n    ▄▄  │ ▄▄\n   ◖██╲ │╱██◗\n      ╭─┴──╮\n      ╲███╱ ╲▄\n       ▔▔▔  ◖█◗\n             ╲▄"
        }
        "cactus" => {
            "        ╭─╮\n    ╭╮  │┆│\n    │╰──┤┆│ ╭╮\n    ╰───┤┆├─╯│\n        │┆├──╯\n     ╭──┴─┴─╮\n      ╲████╱\n       ▔▔▔▔"
        }
        "rosette" => {
            "        ◢◣\n     ◢◣ ╱╲ ◢◣\n    ◥██╲│╱██◤\n     ◥█╲│╱█◤\n    ◥██╲│╱██◤\n     ╭─────╮\n      ╲███╱\n       ▔▔▔"
        }
        "palm" => {
            "    ╲ ╲ │ ╱ ╱\n   ──╲╲ │╱╱──\n    ╱╱╲╲│╱╲╲\n   ╱    │   ╲\n        │\n     ╭──┴──╮\n      ╲███╱\n       ▔▔▔"
        }
        "fern" => {
            "     ╲  │  ╱\n    ╲╲╲ │ ╱╱╱\n   ╲╲╲╲╲│╱╱╱╱╱\n    ╲╲╲╲│╱╱╱╱\n      ╲╲│╱╱\n     ╭──┴──╮\n      ╲███╱\n       ▔▔▔"
        }
        "orchid" => {
            "        ❀──❀\n       ╱    ╲❀\n       │\n    ◖██│██◗\n      ╲│╱\n     ╭─────╮\n      ╲███╱\n       ▔▔▔"
        }
        "grass" => {
            "     ╱│ │ │╲\n    │││ │ │││\n    │││╱│╲│││\n    │││ │ │││\n     ╲│ │ │╱\n     ╭─────╮\n      ╲███╱\n       ▔▔▔"
        }
        _ => {
            "        ▄▄\n       ◖██◗\n    ▄▄  │  ▄▄\n   ◖██╲ │ ╱██◗\n       ╲│╱\n     ╭─────╮\n      ╲███╱\n       ▔▔▔"
        }
    }
}

/// Add decorations only in a padded margin; never replace foliage or pot glyphs.
pub fn portrait(
    plant: &crate::model::Plant,
    frame: u64,
    daylight: crate::environment::Daylight,
    enabled: bool,
) -> String {
    use crate::environment::{Daylight, Exposure};
    let drawing = silhouette(&plant.taxon, frame / 2);
    if !enabled {
        return drawing.to_string();
    }
    // The silhouettes above contain only single-cell glyphs.
    let width = drawing
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        + 6;
    let mut rows: Vec<Vec<char>> = drawing
        .lines()
        .map(|line| {
            let mut row: Vec<char> = format!("   {line}").chars().collect();
            row.resize(width, ' ');
            row
        })
        .collect();
    if rows.is_empty() {
        return drawing.to_string();
    }
    let exposure = Exposure::for_plant(plant);
    let phase = frame.wrapping_add((plant.id as u64).wrapping_mul(7)) % 12;
    if daylight == Daylight::Night || exposure == Exposure::Shade {
        let y = (phase as usize / 6).min(rows.len() - 1);
        rows[y][width - 2] = '☾';
    } else {
        let active = if exposure == Exposure::FullSun { 9 } else { 3 };
        if phase < active {
            let positions = [(0, 1), (width - 2, 0), (width - 1, 4), (1, 6)];
            let (x, y) = positions[phase as usize % positions.len()];
            let y = y.min(rows.len() - 1);
            rows[y][x] = if phase.is_multiple_of(2) {
                '✧'
            } else {
                '✦'
            };
            if exposure == Exposure::FullSun {
                let y = (y + 3) % rows.len();
                rows[y][if x < 3 { width - 2 } else { 1 }] = '·';
            }
        }
    }
    rows.into_iter()
        .map(|row| row.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{Daylight, Exposure};

    #[test]
    fn decorations_follow_exposure_and_daylight_without_covering_plants() -> anyhow::Result<()> {
        let mut plant = crate::demo_database()?.plants()?.remove(0);
        let mut counts = vec![];
        for (location, expected) in [
            ("Full sun", Exposure::FullSun),
            ("Part sun", Exposure::PartSun),
            ("Shade", Exposure::Shade),
        ] {
            plant.location = location.into();
            assert_eq!(Exposure::for_plant(&plant), expected);
            let mut sparkles = 0;
            for frame in 0..48 {
                let decorated = portrait(&plant, frame, Daylight::Day, true);
                if decorated.contains(['✧', '✦']) {
                    sparkles += 1;
                }
                if expected == Exposure::Shade {
                    assert!(decorated.contains('☾'));
                }
                for (raw, padded) in silhouette(&plant.taxon, frame / 2)
                    .lines()
                    .zip(decorated.lines())
                {
                    assert_eq!(
                        padded
                            .chars()
                            .skip(3)
                            .take(raw.chars().count())
                            .collect::<String>(),
                        raw
                    );
                }
                let night = portrait(&plant, frame, Daylight::Night, true);
                assert!(night.contains('☾'));
                assert!(!night.contains(['✧', '✦']));
                assert_eq!(
                    portrait(&plant, frame, Daylight::Day, false),
                    silhouette(&plant.taxon, frame / 2)
                );
            }
            counts.push(sparkles);
        }
        assert!(counts[0] > counts[1] && counts[1] > counts[2]);
        plant.location = "Kitchen".into();
        plant.taxon.light = "Full sun".into();
        assert_eq!(Exposure::for_plant(&plant), Exposure::FullSun);
        plant.location = "Avoid direct sun".into();
        assert_eq!(Exposure::for_plant(&plant), Exposure::PartSun);
        Ok(())
    }
}
