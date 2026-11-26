//! Conditional text: conditions (class 0x20701), condition sets (class
//! 0x20734) and the text of hidden conditions (class 0x20704).
//!
//! Folio fork facts (`FOLIO_FORK.txt`, revision folio.5), from documents
//! Folio made in InDesign 20.5.5 with their IDML (four conditions, one set,
//! six hidden texts) and the Feature Atlas (one condition, one hidden
//! text). Not upstream documentation.

use super::*;

pub mod chunk {
    /// Preferences: u32 count, condition UIDs in IDML order.
    pub const CONDITIONS: u32 = 0x20702;
    /// Condition: u16 visible, u32 indicator colour, u32 name length and
    /// segments, u32 indicator method, u32 underline appearance.
    pub const CONDITION: u32 = 0x20701;
    /// Preferences: u32 count, condition set UIDs.
    pub const CONDITION_SETS: u32 = 0x20737;
    /// Condition set: u32 count, then (u32 condition, u16 visible) pairs,
    /// then u32 name length and segments.
    pub const CONDITION_SET: u32 = 0x20736;
}

/// A condition as InDesign stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub uid: u32,
    pub name: String,
    /// `Visible`: text with only hidden conditions is in `HiddenText`.
    pub visible: bool,
    /// `IndicatorColor` code (`UIColors`).
    pub color: u32,
    /// `IndicatorMethod`: 0 `UseUnderline`, 1 `UseHighlight`.
    pub method: u32,
    /// `UnderlineIndicatorAppearance`: 0 `Wavy`, 1 `Solid`, 2 `Dashed`.
    pub appearance: u32,
}

impl Condition {
    /// IDML `Self`: `Condition/` and the name.
    pub fn reference(&self) -> String {
        format!("Condition/{}", self.name)
    }

    /// The reference as `AppliedConditions` lists it (spaces as `%20`).
    pub fn applied_reference(&self) -> String {
        self.reference().replace('%', "%25").replace(' ', "%20")
    }
}

/// A condition set: each condition's visibility.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionSet {
    pub uid: u32,
    pub name: String,
    pub pairs: Vec<(u32, bool)>,
}

/// u32 length, then text segments.
fn counted(c: &mut Cursor) -> Result<String, Error> {
    match c.u32()? as usize {
        0 => Ok(String::new()),
        n => c.segments(n),
    }
}

impl Reader<'_> {
    /// The document's conditions and condition sets, in the preferences'
    /// order; one that cannot be read is left out with a warning.
    pub(super) fn conditions(&self) -> (Vec<Condition>, Vec<ConditionSet>) {
        let Some(&(prefs, _)) = self
            .db
            .classes()
            .iter()
            .find(|(_, c)| *c == class::PREFERENCES)
        else {
            return (Vec::new(), Vec::new());
        };
        let list = |id| {
            self.uid_list(prefs, id).unwrap_or_else(|e| {
                self.warn(format!("conditions left out: {e}"));
                Vec::new()
            })
        };
        let mut conditions = Vec::new();
        for uid in list(chunk::CONDITIONS) {
            let read = || -> Result<Condition, Error> {
                if self.class(uid) != Some(class::CONDITION) {
                    return Err(Error::Corrupt("not a condition".into()));
                }
                let d = self.required(uid, chunk::CONDITION)?;
                let mut c = self.cursor(&d);
                let visible = c.u16()? != 0;
                let color = c.u32()?;
                let name = counted(&mut c)?;
                Ok(Condition {
                    uid,
                    name,
                    visible,
                    color,
                    method: c.u32()?,
                    appearance: c.u32()?,
                })
            };
            match read() {
                Ok(v) => conditions.push(v),
                Err(e) => self.warn(format!("condition {uid} left out: {e}")),
            }
        }
        let mut sets = Vec::new();
        for uid in list(chunk::CONDITION_SETS) {
            let read = || -> Result<ConditionSet, Error> {
                let d = self.required(uid, chunk::CONDITION_SET)?;
                let mut c = self.cursor(&d);
                let n = c.u32()? as usize;
                if n > c.remaining() / 6 {
                    return Err(Error::Corrupt("condition set list too long".into()));
                }
                let mut pairs = Vec::with_capacity(n);
                for _ in 0..n {
                    pairs.push((c.u32()?, c.u16()? != 0));
                }
                Ok(ConditionSet { uid, name: counted(&mut c)?, pairs })
            };
            match read() {
                Ok(v) => sets.push(v),
                Err(e) => self.warn(format!("condition set {uid} left out: {e}")),
            }
        }
        (conditions, sets)
    }
}

/// IDML `IndicatorColor` of a `UIColors` code, for the codes the samples
/// show.
pub fn ui_color(code: u32) -> Option<&'static str> {
    Some(match code {
        19 => "LightBlue",
        26 => "Red",
        27 => "Green",
        28 => "Blue",
        29 => "Yellow",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_applied_references() {
        let c = Condition {
            uid: 1,
            name: "Atlas hidden condition".into(),
            visible: false,
            color: 19,
            method: 0,
            appearance: 0,
        };
        assert_eq!(c.reference(), "Condition/Atlas hidden condition");
        assert_eq!(c.applied_reference(), "Condition/Atlas%20hidden%20condition");
    }
}
