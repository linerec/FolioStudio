//! The INDD reader's saved composition sidecar (`folio-saved-lines`, version 1; written by
//! `third_party/indd-large/src/idml/lines.rs`, which documents the format): InDesign's last
//! composed lines of each story, with offsets in IDML text units (`idml-text-v1`: one per
//! character of `Content`, per `Br`, per element that stands for a character and per
//! `<?ACE n?>`; U+FEFF markers count none). While a story is read (`text`), `UnitTrack`
//! counts the same units, checks the story's unit count and digest against the sidecar and
//! maps the line starts to the story indices layout uses (`SavedLine::start`). A story whose
//! text differs from the saved one gets `SavedComposition::Unusable` and is composed by
//! layout as before; IDML documents have no sidecar and no saved lines.

use crate::warn::Warnings;
use folio_model::{SavedComposition, SavedLine, SavedSegment};
use std::collections::HashMap;
use std::path::Path;

/// The sidecar's format name and the version read.
pub const FORMAT: &str = "folio-saved-lines";
pub const VERSION: u64 = 1;
/// Largest sidecar read (the reader writes at most 32 MiB).
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A line as the sidecar stores it.
#[derive(Clone, Debug)]
pub struct RawLine {
    pub start: usize,
    /// Index into `SidecarStory::frames`; negative when overset.
    pub frame: i64,
    pub column: u16,
    pub baseline: Option<f64>,
    /// x, width and (later segments) start unit or -1.
    pub segments: Vec<(f64, f64, i64)>,
}

/// A story of the sidecar.
#[derive(Clone, Debug)]
pub struct SidecarStory {
    pub units: usize,
    pub digest: u64,
    pub frames: Vec<String>,
    pub lines: Vec<RawLine>,
}

/// The sidecar: stories with lines, and the reader's reason for those without.
#[derive(Clone, Debug, Default)]
pub struct Sidecar {
    pub stories: HashMap<String, SidecarStory>,
    pub skipped: HashMap<String, String>,
}

/// The sidecar named by the reader's `result.json` (`savedLines.file`, next to it), or
/// `None` when it names none. A sidecar that cannot be read, or of another format or
/// version, is left out with a warning (every story is then composed by layout).
pub fn load(result: &serde_json::Value, dir: &Path, w: &mut Warnings) -> Option<Sidecar> {
    let v = load_value(result, dir, w)?;
    match parse(&v) {
        Ok(s) => Some(s),
        Err(e) => {
            w.add(format!("saved lines: {} left out: {e}", dir.display()));
            None
        }
    }
}

/// The sidecar's JSON as [`load`] finds it (not yet checked against the format).
pub fn load_value(result: &serde_json::Value, dir: &Path, w: &mut Warnings) -> Option<serde_json::Value> {
    let entry = result.get("savedLines").filter(|v| !v.is_null())?;
    let file = entry.get("file").and_then(|f| f.as_str())?;
    if file.contains(['/', '\\']) || file.contains("..") {
        w.add("saved lines: sidecar name is not a plain file name; left out");
        return None;
    }
    let path = dir.join(file);
    let read = (|| -> Result<serde_json::Value, String> {
        let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if len > MAX_BYTES {
            return Err(format!("{len} bytes, more than {MAX_BYTES}"));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    })();
    match read {
        Ok(v) => Some(v),
        Err(e) => {
            w.add(format!("saved lines: {} left out: {e}", path.display()));
            None
        }
    }
}

/// The sidecar's bytes as stored, after the same checks as [`load_value`] (size, plain
/// name); `.folio` documents keep them as their own part.
pub fn sidecar_bytes(result: &serde_json::Value, dir: &Path) -> Option<Vec<u8>> {
    let file = result.get("savedLines").filter(|v| !v.is_null())?.get("file")?.as_str()?;
    if file.contains(['/', '\\']) || file.contains("..") {
        return None;
    }
    let path = dir.join(file);
    (std::fs::metadata(&path).ok()?.len() <= MAX_BYTES).then(|| std::fs::read(&path).ok()).flatten()
}

/// The sidecar from its JSON.
pub fn parse(v: &serde_json::Value) -> Result<Sidecar, String> {
    if v.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err("not a folio-saved-lines sidecar".into());
    }
    if v.get("version").and_then(|f| f.as_u64()) != Some(VERSION) {
        return Err(format!("version {} (reads {VERSION})", v.get("version").map_or("none".into(), |x| x.to_string())));
    }
    if v.get("units").and_then(|f| f.as_str()) != Some("idml-text-v1") {
        return Err("unknown text units".into());
    }
    let mut out = Sidecar::default();
    for s in v.get("stories").and_then(|s| s.as_array()).ok_or("no stories")? {
        let id = s.get("story").and_then(|x| x.as_str()).ok_or("story without id")?.to_owned();
        let units = s.get("units").and_then(|x| x.as_u64()).ok_or("story without units")? as usize;
        let digest = s
            .get("digest")
            .and_then(|x| x.as_str())
            .and_then(|d| u64::from_str_radix(d, 16).ok())
            .ok_or("story without digest")?;
        let frames = s
            .get("frames")
            .and_then(|x| x.as_array())
            .ok_or("story without frames")?
            .iter()
            .map(|f| f.as_str().map(str::to_owned).ok_or("frame not a string"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut lines = Vec::new();
        for l in s.get("lines").and_then(|x| x.as_array()).ok_or("story without lines")? {
            let a = l.as_array().ok_or("line not an array")?;
            let int = |i: usize| a.get(i).and_then(|x| x.as_i64()).ok_or("line field missing");
            let baseline = match a.get(3) {
                Some(serde_json::Value::Null) => None,
                Some(x) => Some(x.as_f64().ok_or("baseline not a number")?),
                None => return Err("line field missing".into()),
            };
            let mut segments = Vec::new();
            for (k, g) in a.get(4).and_then(|x| x.as_array()).ok_or("line without segments")?.iter().enumerate() {
                let g = g.as_array().ok_or("segment not an array")?;
                let num = |i: usize| g.get(i).and_then(|x| x.as_f64()).ok_or("segment field missing");
                let start = if k == 0 { -1 } else { g.get(2).and_then(|x| x.as_i64()).ok_or("segment start missing")? };
                segments.push((num(0)?, num(1)?, start));
            }
            let start = int(0)?;
            let column = int(2)?;
            if start < 0 || !(0..=u16::MAX as i64).contains(&column) {
                return Err("line field out of range".into());
            }
            lines.push(RawLine { start: start as usize, frame: int(1)?, column: column as u16, baseline, segments });
        }
        out.stories.insert(id, SidecarStory { units, digest, frames, lines });
    }
    for s in v.get("skipped").and_then(|s| s.as_array()).into_iter().flatten() {
        if let (Some(id), Some(why)) = (s.get("story").and_then(|x| x.as_str()), s.get("reason").and_then(|x| x.as_str())) {
            out.skipped.insert(id.to_owned(), why.to_owned());
        }
    }
    Ok(out)
}

/// 64-bit FNV-1a, fed one code at a time (the sidecar's `digest`).
fn fnv(mut h: u64, c: char) -> u64 {
    let mut buf = [0u8; 4];
    for b in c.encode_utf8(&mut buf).bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Counts a story's IDML text units while it is read and maps the saved line starts.
pub struct UnitTrack<'s> {
    story: &'s SidecarStory,
    /// Unit offsets to map (line and segment starts), sorted and distinct.
    targets: Vec<usize>,
    next: usize,
    /// Story index of each target.
    mapped: Vec<usize>,
    count: usize,
    hash: u64,
}

impl<'s> UnitTrack<'s> {
    pub fn new(story: &'s SidecarStory) -> Self {
        let mut targets: Vec<usize> = story
            .lines
            .iter()
            .flat_map(|l| std::iter::once(l.start).chain(l.segments.iter().filter(|g| g.2 >= 0).map(|g| g.2 as usize)))
            .collect();
        targets.sort_unstable();
        targets.dedup();
        Self { story, targets, next: 0, mapped: Vec::new(), count: 0, hash: 0xcbf2_9ce4_8422_2325 }
    }

    /// One unit with its digest code; `at` is the story index of its first character (or of
    /// the character after it when it adds none).
    pub fn unit(&mut self, code: char, at: usize) {
        while self.targets.get(self.next) == Some(&self.count) {
            self.mapped.push(at);
            self.next += 1;
        }
        self.count += 1;
        self.hash = fnv(self.hash, code);
    }

    /// The saved composition once the story is read; `end` is the story index after its
    /// last character.
    pub fn finish(mut self, end: usize) -> SavedComposition {
        while self.targets.get(self.next) == Some(&self.count) {
            self.mapped.push(end);
            self.next += 1;
        }
        let s = self.story;
        if self.count != s.units {
            return SavedComposition::Unusable(format!("text has {} units, the saved lines {}", self.count, s.units));
        }
        if self.hash != s.digest {
            return SavedComposition::Unusable("text differs from the saved lines".into());
        }
        if self.mapped.len() != self.targets.len() {
            return SavedComposition::Unusable("a saved line starts after the text".into());
        }
        let at = |u: usize| self.targets.binary_search(&u).ok().map(|i| self.mapped[i]);
        let mut lines = Vec::with_capacity(s.lines.len());
        for l in &s.lines {
            let frame = match l.frame {
                f if f < 0 => None,
                f => match s.frames.get(f as usize) {
                    Some(id) => Some(id.clone()),
                    None => return SavedComposition::Unusable("a saved line names no frame".into()),
                },
            };
            let Some(start) = at(l.start) else {
                return SavedComposition::Unusable("a saved line start is not in the text".into());
            };
            let mut segments = Vec::with_capacity(l.segments.len());
            for (k, g) in l.segments.iter().enumerate() {
                let start = match (k, g.2) {
                    (0, _) => Some(start),
                    (_, u) if u < 0 => None,
                    (_, u) => match at(u as usize) {
                        Some(s) => Some(s),
                        None => return SavedComposition::Unusable("a saved segment start is not in the text".into()),
                    },
                };
                segments.push(SavedSegment { x: g.0, width: g.1, start });
            }
            lines.push(SavedLine { start, frame, column: l.column, baseline: l.baseline, segments });
        }
        SavedComposition::Lines(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(s: &str) -> u64 {
        s.chars().fold(0xcbf2_9ce4_8422_2325, fnv)
    }

    #[test]
    fn parses_and_maps_units() {
        let v = serde_json::json!({
            "format": FORMAT, "version": 1, "units": "idml-text-v1",
            "stories": [{"story": "u9", "units": 6, "digest": format!("{:016x}", digest("ab\rc\u{18}d")),
                "frames": ["u20"], "lines": [[0, 0, 0, 7.5, [[0, 100]]], [3, 0, 1, null, [[0, 40], [60, 40, 5]]], [6, -1, 0, 0, [[0, 100]]]]}],
            "skipped": [{"story": "u10", "reason": "spanning or split columns"}]
        });
        let side = parse(&v).unwrap();
        assert_eq!(side.skipped["u10"], "spanning or split columns");
        let story = &side.stories["u9"];
        let mut t = UnitTrack::new(story);
        // "ab" ¶ "c" variable "12" "d": the variable is one unit and two characters.
        for (code, at) in [('a', 0), ('b', 1), ('\r', 2), ('c', 3), ('\u{18}', 4), ('d', 6)] {
            t.unit(code, at);
        }
        let SavedComposition::Lines(lines) = t.finish(7) else { panic!("unusable") };
        type LineStart<'a> = (usize, Option<&'a str>, u16, Vec<Option<usize>>);
        let starts: Vec<LineStart<'_>> =
            lines.iter().map(|l| (l.start, l.frame.as_deref(), l.column, l.segments.iter().map(|g| g.start).collect())).collect();
        assert_eq!(starts, [(0, Some("u20"), 0, vec![Some(0)]), (3, Some("u20"), 1, vec![Some(3), Some(6)]), (7, None, 0, vec![Some(7)])]);
        assert_eq!((lines[0].baseline, lines[1].baseline, lines[1].segments[1].x), (Some(7.5), None, 60.0));
        // Another text.
        let mut t = UnitTrack::new(story);
        for (code, at) in [('a', 0), ('x', 1), ('\r', 2), ('c', 3), ('\u{18}', 4), ('d', 6)] {
            t.unit(code, at);
        }
        assert!(matches!(t.finish(7), SavedComposition::Unusable(_)));
        let mut t = UnitTrack::new(story);
        t.unit('a', 0);
        assert!(matches!(t.finish(1), SavedComposition::Unusable(_)));
        assert!(parse(&serde_json::json!({"format": FORMAT, "version": 2, "units": "idml-text-v1", "stories": []})).is_err());
    }
}
