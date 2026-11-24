use ccusage_core::UsageSummary;
use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Family {
    Opus,
    Sonnet,
    Haiku,
    Other,
}

impl Family {
    pub(crate) fn of(model: &str) -> Self {
        if model.contains("opus") {
            Self::Opus
        } else if model.contains("sonnet") {
            Self::Sonnet
        } else if model.contains("haiku") {
            Self::Haiku
        } else {
            Self::Other
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            Self::Opus => Color::Rgb(0xe0, 0x96, 0x5c),
            Self::Sonnet => Color::Rgb(0x6c, 0xa8, 0xe8),
            Self::Haiku => Color::Rgb(0x7c, 0xc4, 0x7c),
            Self::Other => Color::Gray,
        }
    }
}

pub(crate) fn dominant_family(summary: &UsageSummary) -> Family {
    let mut families = summary
        .model_breakdowns
        .iter()
        .map(|breakdown| Family::of(&breakdown.model_name));
    let Some(first) = families.next() else {
        return Family::Other;
    };
    if families.all(|family| family == first) {
        first
    } else {
        Family::Other
    }
}
