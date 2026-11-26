//! The composition InDesign saved with a story: the lines it last composed
//! (Folio fork, folio.6). InDesign shows and prints this composition until
//! the story is edited, so a reader that keeps it shows InDesign's lines even
//! where the fonts it used are not installed.
//!
//! Layout (Folio's own differential analysis of documents made in InDesign
//! 20.5.5.119 for this purpose and of #347, each compared with InDesign's
//! scripted `lines`; records in `artifacts/next-20261018/saved-lines/`; no
//! Adobe binary was read):
//! - chunk 0x223 of a story: after the strand list, the story's frame list
//!   (class 0x228). Its chunk 0x205 is the story and the frame columns
//!   (class 0x227) it flows through, in thread order.
//! - the frame list's strand (chunk 0x261, pages of class 0x23C with chunk
//!   0x262) has run kind 0x24E: one record per composed line, in text order,
//!   over the whole story text (the story's own text, then the text of its
//!   table cells, footnotes, notes and hidden conditions). The record length
//!   counts characters like every strand but text.
//! - a record holds tag 0x24C, u16 2, then sub-records (u32 tag, u32 size,
//!   data): 0x29F geometry and 0x2A3 (not read).
//! - 0x29F: u32 owner (the story for its own lines; a table, footnote or 0
//!   for hidden text), u32 frame column (0 when overset; 1 for table cell and
//!   footnote lines), f64 leading, flag words (u32, each with bit 31 set
//!   continues), u32 field mask, f64 baseline below the column's top before
//!   vertical justification, f64 ascent when mask bit 13 is set, then for
//!   each of the (mask & 0x7F) segments f64 x and f64 width in the frame's
//!   text area; every segment after the first also has a u32 character
//!   count after its width, and every segment a natural width when mask bit
//!   25 is set. Mask bit 16 adds an 11-byte drop cap block after the
//!   baseline, bit 15 a u32 after the ascent. After the segments, mask bit
//!   17 adds the line's vertical justification offset (first); other bits
//!   add fields there that are not read. Mask bit 7 marks a table's line
//!   (other fields). A line split by a text wrap or a drop cap has two or
//!   more segments.
//! - flag word bit 27 marks a line InDesign no longer shows: it follows the
//!   overset lines and repeats an earlier line's geometry; InDesign reports
//!   it as overset (#347 stories 122077, 269757, 275047).
//! - a frame column's chunk 0x15B has its multi-column frame at 4, whose
//!   chunk 0x15B has the text frame at 4; u16 at 2 of the column's chunk
//!   0x2CF is its column index (span and split columns repeat an index).
//!
//! Only little-endian files were compared; big-endian files are read the
//! same way with their byte order.

use super::*;

/// A line of a story's saved composition.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedLine {
    /// UTF-16 offset of the line's first character in the story text.
    pub start: usize,
    /// Its length in UTF-16 units.
    pub len: usize,
    /// The record's owner: the story for its own lines.
    pub owner: u32,
    /// Where the line is shown: `None` when it is overset.
    pub place: Option<LinePlace>,
    /// Baseline below the top of the frame's text area, vertical
    /// justification included; `None` when the record's fields are not all
    /// known.
    pub baseline: Option<f64>,
    /// The line's ascent when the record has it.
    pub ascent: Option<f64>,
    /// The line's segments: one, or more where a text wrap or a drop cap
    /// splits it.
    pub segments: Vec<Segment>,
    /// The line holds a table.
    pub table: bool,
}

/// A segment of a saved line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// x and width in the frame's text area (paragraph indents not
    /// subtracted).
    pub x: f64,
    pub width: f64,
    /// UTF-16 offset of the segment's first character; `None` for a later
    /// segment that holds none. The first segment starts the line.
    pub start: Option<usize>,
}

/// The frame column of a shown line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinePlace {
    /// The frame column object (class 0x227).
    pub column: u32,
    /// The text frame (or other page item) of the column.
    pub frame: u32,
    /// Index of the column in its frame (chunk 0x2CF).
    pub index: u16,
}

/// A story's saved composition.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Composition {
    /// Every line of the story text, in text order.
    pub lines: Vec<SavedLine>,
    /// Why the composition cannot be used for the story's own text (it is
    /// kept for inspection), e.g. columns the reader cannot tell apart.
    pub unusable: Option<String>,
}

const WAX_LINES: u32 = 0x24E;
const LINE: u32 = 0x24C;
const GEOMETRY: u32 = 0x29F;
const STALE: u32 = 0x0800_0000;
const COLUMN_INDEX: u32 = 0x2CF;
/// Bound on the lines of one story: more than InDesign's limit of text in a
/// story could need is malformed input.
const MAX_LINES: usize = 4_000_000;

impl Reader<'_> {
    /// The saved composition of story `uid` whose frame list is
    /// `frame_list` (the u32 after the strand list of chunk 0x223).
    /// `unit(p)` is the UTF-16 offset of character position `p`; `chars` the
    /// number of characters of the story text. `Ok(None)` when the story has
    /// no frame list (a story never placed).
    pub(super) fn composition(
        &self,
        uid: u32,
        frame_list: u32,
        chars: usize,
        unit: &dyn Fn(usize) -> usize,
    ) -> Result<Option<Composition>, String> {
        if frame_list == 0 || self.class(frame_list) != Some(class::FRAME_LIST) {
            return Ok(None);
        }
        let e = |err: Error| err.to_string();
        let frames = self
            .chunk(frame_list, chunk::FRAME_LIST_FRAMES)
            .map_err(e)?
            .ok_or("frame list without chunk 0x205")?;
        let mut c = self.cursor(&frames);
        if c.u32().map_err(e)? != uid {
            return Err("frame list names another story".into());
        }
        let columns = c.u32_list().map_err(e)?;
        let Some(list) = self.chunk(frame_list, chunk::STRAND_DATA).map_err(e)? else {
            return Err("frame list without lines".into());
        };
        let mut c = self.cursor(&list);
        let pages = c.u16().map_err(e)?;
        let mut lines = Vec::new();
        let mut pos = 0usize;
        let mut place_of: HashMap<u32, Option<LinePlace>> = HashMap::new();
        for _ in 0..pages {
            let page_len = c.u32().map_err(e)? as usize;
            let data = c.u32().map_err(e)?;
            let runs = self.required(data, chunk::STRAND_RUNS).map_err(e)?;
            let mut r = self.cursor(&runs);
            if r.u32().map_err(e)? != WAX_LINES {
                return Err("frame list strand of another kind".into());
            }
            r.skip(4).map_err(e)?;
            let count = r.u16().map_err(e)?;
            let page_start = pos;
            for _ in 0..count {
                let size = r.u32().map_err(e)? as usize;
                let rec = r.bytes(size).map_err(e)?;
                let (mut line, counts) = self.saved_line(rec).map_err(e)?;
                let len = line.len;
                line.start = unit(pos);
                line.len = unit(pos + len) - line.start;
                // Segment k > 0 starts after the characters of segments k.. end.
                let mut rest: usize = counts.iter().sum();
                if rest > len {
                    return Err("line segments hold more characters than the line".into());
                }
                for (k, seg) in line.segments.iter_mut().enumerate() {
                    if k == 0 {
                        seg.start = Some(line.start);
                        continue;
                    }
                    let n = counts.get(k - 1).copied().unwrap_or(0);
                    seg.start = (n > 0).then(|| unit(pos + len - rest));
                    rest -= n.min(rest);
                }
                if let Some(column) = line.place.map(|p| p.column) {
                    let place = match place_of.get(&column) {
                        Some(p) => *p,
                        None => {
                            let p = if columns.contains(&column) { self.line_place(column).map_err(e)? } else { None };
                            place_of.insert(column, p);
                            p
                        }
                    };
                    line.place = place;
                    // A column outside the story's frames (table cells and
                    // footnotes name column 1) has no place of its own.
                }
                pos += len;
                lines.push(line);
                if lines.len() > MAX_LINES {
                    return Err("too many lines".into());
                }
            }
            if pos - page_start != page_len {
                return Err(format!("line page holds {} characters, not {page_len}", pos - page_start));
            }
        }
        if pos != chars {
            return Err(format!("lines hold {pos} characters, the story {chars}"));
        }
        // Span and split columns repeat a column index within a frame.
        let mut seen: HashMap<(u32, u16), u32> = HashMap::new();
        let mut unusable = None;
        for p in place_of.values().flatten() {
            if let Some(&other) = seen.get(&(p.frame, p.index))
                && other != p.column
            {
                unusable = Some("spanning or split columns".to_string());
            }
            seen.insert((p.frame, p.index), p.column);
        }
        Ok(Some(Composition { lines, unusable }))
    }

    /// One line record (a run of the frame list strand, after its length),
    /// with the character counts of its segments after the first. `len`
    /// holds the record's character count; `start` and the segments' starts
    /// are set by the caller; `place` names only the column.
    fn saved_line(&self, rec: &[u8]) -> Result<(SavedLine, Vec<usize>), Error> {
        let mut c = self.cursor(rec);
        let len = c.u32()? as usize;
        let mut line = SavedLine {
            start: 0,
            len,
            owner: 0,
            place: None,
            baseline: None,
            ascent: None,
            segments: Vec::new(),
            table: false,
        };
        let mut counts = Vec::new();
        if c.u32()? != LINE {
            return Ok((line, counts));
        }
        let n = c.u16()?;
        for _ in 0..n {
            let tag = c.u32()?;
            let size = c.u32()? as usize;
            let data = c.bytes(size)?;
            if tag != GEOMETRY {
                continue;
            }
            let mut g = self.cursor(data);
            line.owner = g.u32()?;
            let column = g.u32()?;
            g.f64()?;
            let first = g.u32()?;
            let mut word = first;
            while word & 0x8000_0000 != 0 {
                word = g.u32()?;
            }
            let mask = g.u32()?;
            line.table = mask & 0x80 != 0;
            if column > 1 && first & STALE == 0 {
                line.place = Some(LinePlace { column, frame: 0, index: 0 });
            }
            // The geometry is read where its fields are known; a record
            // with other fields keeps none (the line itself is kept).
            if !line.table && mask & 0x200 != 0 && self.line_geometry(&mut g, mask, &mut line, &mut counts).is_err() {
                line.baseline = None;
                line.ascent = None;
                line.segments.clear();
                counts.clear();
            }
        }
        Ok((line, counts))
    }

    /// The fields of a 0x29F block after its mask (see the module notes):
    /// baseline, an 11-byte drop cap block (mask bit 16), the ascent (bit
    /// 13), a u32 (bit 15), then each of the (mask & 0x7F) segments: x,
    /// width, for every segment but the first a u32 character count, and a
    /// natural width (bit 25); then the vertical justification offset (bit
    /// 17), read only when no other field of an unknown place is present.
    fn line_geometry(&self, g: &mut Cursor<'_>, mask: u32, line: &mut SavedLine, counts: &mut Vec<usize>) -> Result<(), Error> {
        let baseline = g.f64()?;
        if mask & 0x1_0000 != 0 {
            g.skip(11)?;
        }
        if mask & 0x2000 != 0 {
            line.ascent = Some(g.f64()?);
        }
        if mask & 0x8000 != 0 {
            g.u32()?;
        }
        for k in 0..mask & 0x7F {
            let x = g.f64()?;
            let width = g.f64()?;
            if k > 0 {
                counts.push(g.u32()? as usize);
            }
            if mask & 0x0200_0000 != 0 {
                g.f64()?;
            }
            line.segments.push(Segment { x, width, start: None });
        }
        line.baseline = match mask & 0x2_0000 {
            0 => Some(baseline),
            _ if mask & 0x00F4_0000 == 0 => Some(baseline + g.f64()?),
            _ => None,
        };
        Ok(())
    }

    /// Frame and column index of a frame column of the story's frame list.
    fn line_place(&self, column: u32) -> Result<Option<LinePlace>, Error> {
        if self.class(column) != Some(class::FRAME_COLUMN) {
            return Ok(None);
        }
        let parent = |uid: u32| -> Result<Option<u32>, Error> {
            Ok(self
                .chunk(uid, chunk::ITEM_HIERARCHY)?
                .and_then(|h| self.enc().u32_at(&h, 4))
                .and_then(uid_or_none))
        };
        let Some(mcf) = parent(column)? else { return Ok(None) };
        let Some(frame) = parent(mcf)? else { return Ok(None) };
        let index = self
            .chunk(column, COLUMN_INDEX)?
            .and_then(|d| self.enc().u16_at(&d, 2))
            .unwrap_or(0);
        Ok(Some(LinePlace { column, frame, index }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::synthetic;

    /// A 0x29F geometry block.
    fn geometry(owner: u32, column: u32, flags: &[u32], mask: u32, values: &[f64]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&owner.to_le_bytes());
        d.extend_from_slice(&column.to_le_bytes());
        d.extend_from_slice(&12.0f64.to_le_bytes());
        for f in flags {
            d.extend_from_slice(&f.to_le_bytes());
        }
        d.extend_from_slice(&mask.to_le_bytes());
        for v in values {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d
    }

    /// A line record: length, tag 0x24C, the geometry and an empty 0x2A3.
    fn record(len: u32, geometry: &[u8]) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&len.to_le_bytes());
        r.extend_from_slice(&LINE.to_le_bytes());
        r.extend_from_slice(&2u16.to_le_bytes());
        r.extend_from_slice(&GEOMETRY.to_le_bytes());
        r.extend_from_slice(&(geometry.len() as u32).to_le_bytes());
        r.extend_from_slice(geometry);
        r.extend_from_slice(&0x2A3u32.to_le_bytes());
        r.extend_from_slice(&4u32.to_le_bytes());
        r.extend_from_slice(&0u32.to_le_bytes());
        r
    }

    fn runs(records: &[Vec<u8>]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&WAX_LINES.to_le_bytes());
        d.extend_from_slice(&20u32.to_le_bytes());
        d.extend_from_slice(&(records.len() as u16).to_le_bytes());
        for r in records {
            d.extend_from_slice(&(r.len() as u32).to_le_bytes());
            d.extend_from_slice(r);
        }
        d
    }

    fn hierarchy(parent: u32) -> Vec<u8> {
        let mut h = 0xCFu32.to_le_bytes().to_vec();
        h.extend_from_slice(&parent.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h
    }

    /// Story 10 with frame list 20 over columns 31 (index 0) and 32 (index
    /// 1) of text frame 40 (multi-column frame 35).
    fn objects(records: &[Vec<u8>], pages: &[(u32, usize)]) -> Vec<(u32, u32, Vec<u8>)> {
        let mut frames = 10u32.to_le_bytes().to_vec();
        frames.extend_from_slice(&2u32.to_le_bytes());
        frames.extend_from_slice(&31u32.to_le_bytes());
        frames.extend_from_slice(&32u32.to_le_bytes());
        let mut list = (pages.len() as u16).to_le_bytes().to_vec();
        let mut objects: Vec<(u32, u32, Vec<u8>)> = Vec::new();
        let mut at = 0;
        for &(uid, n) in pages {
            let chars: u32 = records[at..at + n].iter().map(|r| u32::from_le_bytes(r[0..4].try_into().unwrap())).sum();
            list.extend_from_slice(&chars.to_le_bytes());
            list.extend_from_slice(&uid.to_le_bytes());
            objects.push((uid, 0x23C, synthetic::chunks(&[(chunk::STRAND_RUNS, runs(&records[at..at + n]))])));
            at += n;
        }
        objects.push((20, class::FRAME_LIST, synthetic::chunks(&[(chunk::FRAME_LIST_FRAMES, frames), (chunk::STRAND_DATA, list)])));
        for (uid, index) in [(31u32, 0u16), (32, 1)] {
            let mut col = vec![1, 0];
            col.extend_from_slice(&index.to_le_bytes());
            objects.push((uid, class::FRAME_COLUMN, synthetic::chunks(&[(chunk::ITEM_HIERARCHY, hierarchy(35)), (COLUMN_INDEX, col)])));
        }
        objects.push((35, class::MULTI_COLUMN_FRAME, synthetic::chunks(&[(chunk::ITEM_HIERARCHY, hierarchy(40))])));
        objects
    }

    #[test]
    fn lines_columns_overset_and_stale() {
        let recs = vec![
            record(5, &geometry(10, 31, &[0x8100_1060, 0x8000_0000, 8], 0x0208_2201, &[7.5, 7.5, 0.0, 100.0, 90.0])),
            record(4, &geometry(10, 32, &[0x1060], 0x0208_0201, &[19.5, 112.0, 100.0, 60.0])),
            // Vertical justification: the baseline moves down 2.5.
            record(1, &geometry(10, 32, &[0x1060], 0x020A_0201, &[19.5, 112.0, 100.0, 60.0, 2.5])),
            record(3, &geometry(0, 0, &[0x60], 0x0208_0201, &[31.5, 0.0, 100.0, 50.0])),
            record(2, &geometry(10, 31, &[0x0900_1440], 0x0208_2201, &[7.5, 7.5, 0.0, 100.0, 90.0])),
        ];
        // A line split in two by a wrap: 6 characters, 4 of them right of it.
        // Fields: baseline, ascent, x, width, natural width, then x, width,
        // character count and natural width of the second segment.
        let mut split = geometry(10, 31, &[0x1060], 0x0208_2202, &[43.5, 7.5, 0.0, 40.0, 30.0, 60.0, 40.0]);
        split.extend_from_slice(&4u32.to_le_bytes());
        split.extend_from_slice(&20.0f64.to_le_bytes());
        let mut recs = recs;
        recs.insert(1, record(6, &split));
        let objs = objects(&recs, &[(50, 3), (51, 3)]);
        let bytes = synthetic::image(&objs);
        let db = synthetic::database(&bytes, &objs);
        let r = Reader::new(&db);
        let mut c = r.composition(10, 20, 21, &|p| p).unwrap().unwrap();
        let starts: Vec<(usize, usize)> = c.lines.iter().map(|l| (l.start, l.len)).collect();
        assert_eq!(starts, [(0, 5), (5, 6), (11, 4), (15, 1), (16, 3), (19, 2)]);
        assert_eq!(c.lines[3].baseline, Some(22.0));
        c.lines.remove(3);
        assert_eq!(c.lines[0].place, Some(LinePlace { column: 31, frame: 40, index: 0 }));
        assert_eq!(c.lines[2].place, Some(LinePlace { column: 32, frame: 40, index: 1 }));
        let seg = |x: f64, width: f64, start: Option<usize>| Segment { x, width, start };
        assert_eq!((c.lines[0].baseline, c.lines[0].ascent, c.lines[0].segments.clone()), (Some(7.5), Some(7.5), vec![seg(0.0, 100.0, Some(0))]));
        assert_eq!((c.lines[2].baseline, c.lines[2].ascent, c.lines[2].segments.clone()), (Some(19.5), None, vec![seg(112.0, 100.0, Some(11))]));
        // The split line: its second segment starts 4 characters before its end.
        assert_eq!(c.lines[1].segments, [seg(0.0, 40.0, Some(5)), seg(60.0, 40.0, Some(7))]);
        // Overset, and the stale line after it.
        assert_eq!((c.lines[3].place, c.lines[4].place), (None, None));
        assert_eq!(c.unusable, None);
        // The lines must hold the story's characters.
        assert!(r.composition(10, 20, 22, &|p| p).is_err());
        // Another story's frame list.
        assert!(r.composition(11, 20, 21, &|p| p).is_err());
        // No frame list.
        assert_eq!(r.composition(10, 0, 21, &|p| p), Ok(None));
    }
}
