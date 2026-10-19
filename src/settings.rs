use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    Default,
    #[default]
    Greenhouse,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Self; 4] = [Self::Default, Self::Greenhouse, Self::Light, Self::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Greenhouse => "greenhouse",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Greenhouse => "Greenhouse",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.key() == value)
    }

    pub fn cycle(self, direction: i32) -> Self {
        let index = Self::ALL.iter().position(|theme| *theme == self).unwrap() as i32;
        Self::ALL[(index + direction).rem_euclid(Self::ALL.len() as i32) as usize]
    }

    pub fn palette(self) -> Palette {
        let rgb = Color::Rgb;
        match self {
            Self::Default => Palette {
                bg: Color::Reset,
                panel: Color::Reset,
                accent: Color::Green,
                on_accent: Color::Black,
                text: Color::Reset,
                muted: Color::DarkGray,
                border: Color::DarkGray,
                warning: Color::Yellow,
                error: Color::Red,
                water: Color::Cyan,
                count: Color::Green,
            },
            Self::Greenhouse => Palette {
                bg: rgb(17, 25, 23),
                panel: rgb(23, 35, 30),
                accent: rgb(139, 196, 143),
                on_accent: rgb(17, 25, 23),
                text: rgb(226, 228, 207),
                muted: rgb(134, 155, 141),
                border: rgb(54, 77, 61),
                warning: rgb(235, 187, 106),
                error: rgb(238, 138, 128),
                water: rgb(91, 156, 163),
                count: rgb(117, 156, 126),
            },
            Self::Light => Palette {
                bg: rgb(245, 247, 243),
                panel: rgb(255, 255, 252),
                accent: rgb(36, 105, 65),
                on_accent: rgb(245, 247, 243),
                text: rgb(35, 45, 39),
                muted: rgb(91, 107, 97),
                border: rgb(184, 201, 186),
                warning: rgb(143, 89, 12),
                error: rgb(166, 49, 49),
                water: rgb(35, 113, 132),
                count: rgb(82, 124, 91),
            },
            Self::Dark => Palette {
                bg: rgb(18, 21, 28),
                panel: rgb(26, 31, 41),
                accent: rgb(145, 198, 173),
                on_accent: rgb(18, 21, 28),
                text: rgb(228, 232, 241),
                muted: rgb(148, 161, 181),
                border: rgb(62, 75, 97),
                warning: rgb(238, 194, 116),
                error: rgb(245, 145, 156),
                water: rgb(116, 180, 218),
                count: rgb(116, 156, 134),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardSpacing {
    #[default]
    Relaxed,
    Compact,
}

impl CardSpacing {
    pub const ALL: [Self; 2] = [Self::Relaxed, Self::Compact];

    pub fn key(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Compact => "compact",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Relaxed => "Relaxed",
            Self::Compact => "Compact",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|spacing| spacing.key() == value)
    }

    pub fn cycle(self, direction: i32) -> Self {
        let index = Self::ALL
            .iter()
            .position(|spacing| *spacing == self)
            .unwrap() as i32;
        Self::ALL[(index + direction).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub panel: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub warning: Color,
    pub error: Color,
    pub water: Color,
    pub count: Color,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: Theme,
    pub display_artifacts: bool,
    pub card_spacing: CardSpacing,
    pub manual_order: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_default_inherits_colors_without_changing_the_initial_theme() {
        assert_eq!(Theme::default(), Theme::Greenhouse);
        assert_eq!(Theme::parse("default"), Some(Theme::Default));
        assert_eq!(CardSpacing::default(), CardSpacing::Relaxed);
        assert_eq!(CardSpacing::parse("compact"), Some(CardSpacing::Compact));
        assert_eq!(CardSpacing::Relaxed.cycle(1), CardSpacing::Compact);
        let colors = Theme::Default.palette();
        assert_eq!(colors.bg, Color::Reset);
        assert_eq!(colors.panel, Color::Reset);
        assert_eq!(colors.text, Color::Reset);
        for color in [
            colors.accent,
            colors.on_accent,
            colors.muted,
            colors.border,
            colors.warning,
            colors.error,
            colors.water,
            colors.count,
        ] {
            assert!(!matches!(color, Color::Rgb(..) | Color::Indexed(_)));
        }
        for theme in Theme::ALL {
            assert_eq!(Theme::parse(theme.key()), Some(theme));
            assert_eq!(theme.cycle(1).cycle(-1), theme);
        }
    }
}
