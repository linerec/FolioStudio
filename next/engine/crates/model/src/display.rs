//! Display Performance: the object-level setting (Object ▸ Display Performance).
//!
//! InDesign keeps one setting per page item and per placed graphic (IDML `LocalDisplaySetting`); the
//! view's own setting (View ▸ Display Performance) belongs to the window and the application's
//! "Allow Object-Level Display Settings" to the preferences, so neither is part of the document.
//! Measured with InDesign 2025: `artifacts/displayperf-20261023/STATUS.ko.md`.
use serde::{Deserialize, Serialize};

/// IDML `LocalDisplaySetting`; Object ▸ Display Performance ▸ Use View Setting / High Quality /
/// Typical / Fast Display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalDisplay {
    /// `Default`: the item follows the view's setting.
    #[default]
    UseView,
    /// `HighQuality`.
    HighQuality,
    /// `Typical`.
    Typical,
    /// `Optimized`: Fast Display (a graphic is drawn as a grey box).
    Optimized,
}

impl LocalDisplay {
    /// The IDML attribute value.
    pub fn idml(self) -> &'static str {
        match self {
            Self::UseView => "Default",
            Self::HighQuality => "HighQuality",
            Self::Typical => "Typical",
            Self::Optimized => "Optimized",
        }
    }

    /// The setting an IDML value names; `None` for a value this version does not know.
    pub fn from_idml(value: &str) -> Option<Self> {
        Some(match value {
            "Default" => Self::UseView,
            "HighQuality" => Self::HighQuality,
            "Typical" => Self::Typical,
            "Optimized" => Self::Optimized,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::LocalDisplay;

    #[test]
    fn idml_names_round_trip() {
        for d in [LocalDisplay::UseView, LocalDisplay::HighQuality, LocalDisplay::Typical, LocalDisplay::Optimized] {
            assert_eq!(LocalDisplay::from_idml(d.idml()), Some(d));
        }
        assert_eq!(LocalDisplay::from_idml("Fast"), None);
    }
}
