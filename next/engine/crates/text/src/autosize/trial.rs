use folio_model::layout::LaidLine;
use folio_model::{
    Affine, Document, ItemKind, PageItem, Point, Rect, Spread, Story, VerticalJustification,
};

use crate::layout::{FrameRef, stroke_inset, stroke_outset};
use crate::{DocumentFrames, FontStore, StoryLayout, layout_story};

pub(super) const PRECISION: f64 = 0.0001;
pub(super) const WIDTH_PRECISION: f64 = 1.0;
pub(super) const MAX_SIZE: f64 = 14_400.0;
const COLUMN_HEIGHT_PRECISION: f64 = 0.5;
const CONTENT_FLOOR: f64 = 3.0;
const SEARCH_OFFSET: f64 = 0.01;

#[derive(Clone, Copy)]
pub(super) struct Size {
    pub width: f64,
    pub height: f64,
}

pub(super) struct Trial<'a> {
    doc: &'a Document,
    fonts: &'a FontStore,
    story: &'a Story,
    frame: &'a FrameRef<'a>,
    spread: Option<&'a Spread>,
    bounds: Rect,
    point: (f64, f64),
    margins: Size,
    outside: f64,
}

impl<'a> Trial<'a> {
    pub fn new(
        doc: &'a Document,
        fonts: &'a FontStore,
        story: &'a Story,
        frame: &'a FrameRef<'a>,
        spread: Option<&'a Spread>,
        point: (f64, f64),
    ) -> Option<Self> {
        let bounds = rectangle(frame.item)?;
        let stroke = 2.0 * stroke_inset(frame.item);
        let [top, left, bottom, right] = frame.frame.insets;
        let margins = Size {
            width: left + right + stroke,
            height: top + bottom + stroke,
        };
        let outside = 2.0 * stroke_outset(frame.item);
        Some(Self {
            doc,
            fonts,
            story,
            frame,
            spread,
            bounds,
            point,
            margins,
            outside,
        })
    }

    pub fn original(&self) -> Size {
        Size {
            width: self.bounds.width(),
            height: self.bounds.height(),
        }
    }

    pub fn minimum(&self, width: Option<f64>, height: Option<f64>) -> Size {
        Size {
            width: (width.unwrap_or_default() - self.outside).max(PRECISION),
            height: (height.unwrap_or_default() - self.outside).max(PRECISION),
        }
    }

    pub fn outer_ratio(&self) -> f64 {
        let size = self.original();
        (size.width + self.outside) / (size.height + self.outside)
    }

    pub fn proportional_width(&self, height: f64, ratio: f64) -> f64 {
        self.margins.width + (height - self.margins.height) * ratio
    }

    pub fn proportional_height(&self, width: f64, ratio: f64) -> f64 {
        self.margins.height + (width - self.margins.width).max(PRECISION) / ratio
    }

    pub fn width_floor(&self) -> f64 {
        self.margins.width + CONTENT_FLOOR - SEARCH_OFFSET
    }

    pub fn proportional_floor(&self, ratio: f64) -> f64 {
        self.margins.height + ((CONTENT_FLOOR * ratio).max(CONTENT_FLOOR) - SEARCH_OFFSET) / ratio
    }

    pub fn pasteboard_room(&self) -> Option<Size> {
        let matrix = &self.frame.to_spread;
        // Use pasteboard dimensions to seed the search for axis-aligned frames.
        if matrix.b != 0.0 || matrix.c != 0.0 {
            return None;
        }
        let spread = self.spread?;
        let pages = spread
            .pages
            .iter()
            .map(|page| page.transform.apply_rect(&page.bounds))
            .reduce(|a, b| a.union(&b))?;
        let page_width = self.frame.page_rect?.width();
        let pasteboard = self.doc.pasteboard.around(pages, page_width);
        let local = matrix.invert()?.apply_rect(&pasteboard);
        let original = self.original();
        let x = self.bounds.left + original.width * self.point.0;
        let y = self.bounds.top + original.height * self.point.1;
        let outside = self.outside * 0.5;
        let width = anchored_span(x, local.left + outside, local.right - outside, self.point.0);
        let height = anchored_span(y, local.top + outside, local.bottom - outside, self.point.1);
        (width > 0.0 && height > 0.0).then_some(Size { width, height })
    }

    pub fn columns(&self) -> u32 {
        self.frame.frame.columns.max(1)
    }

    pub fn fixed_width(&self) -> Option<f64> {
        self.frame
            .frame
            .column_width
            .filter(|width| width.is_finite() && *width > 0.0)
    }

    pub fn column_path_width(&self, columns: u32) -> Option<f64> {
        let width = self.fixed_width()?;
        let gap = self.frame.frame.gutter.max(0.0);
        Some(self.margins.width + f64::from(columns) * width + f64::from(columns - 1) * gap)
    }

    pub fn minimum_columns(&self, min_width: f64) -> Option<u32> {
        let width = self.fixed_width()?;
        let gap = self.frame.frame.gutter.max(0.0);
        let count = ((min_width - self.margins.width + gap) / (width + gap))
            .ceil()
            .max(1.0);
        (count <= 1000.0).then_some(count as u32)
    }

    pub fn fit_fixed_width(&self, height: f64, min_width: f64) -> Option<Size> {
        let mut lower = self.minimum_columns(min_width)?;
        let size = |count| Size {
            width: self.column_path_width(count).expect("fixed columns"),
            height,
        };
        let fits = |count| self.fits(size(count), count);
        if fits(lower) {
            return Some(size(lower));
        }
        let mut upper = self.columns().max(lower);
        while !fits(upper) {
            if upper == 1000 {
                return None;
            }
            lower = upper;
            upper = (upper * 2).min(1000);
        }
        while upper - lower > 1 {
            let middle = lower + (upper - lower) / 2;
            if fits(middle) {
                upper = middle;
            } else {
                lower = middle;
            }
        }
        Some(size(upper))
    }

    pub fn fit_height(&self, width: f64, lower: f64, columns: u32) -> Option<Size> {
        let fits = |height| self.fits(Size { width, height }, columns);
        let horizontal_top = !self.story.vertical
            && self.frame.item.text_path.is_none()
            && self.frame.frame.vertical_justification == VerticalJustification::Top;
        let height = if horizontal_top && columns > 1 {
            self.fit_column_height(width, lower, columns)
        } else {
            None
        }
        .or_else(|| smallest(lower, lower, self.original().height, PRECISION, fits))?;
        let mut size = Size { width, height };
        if horizontal_top {
            // R3: the clipping tolerance can admit a baseline just below the inset.
            let layout = self.compose(size, columns)?;
            let bottom_inset = self.frame.frame.insets[2] + stroke_inset(self.frame.item);
            for line in self
                .lines(&layout)
                .iter()
                .filter(|line| line.cell.is_none())
            {
                size.height = size
                    .height
                    .max(line.baseline - self.bounds.top + bottom_inset);
            }
        }
        Some(size)
    }

    fn fit_column_height(&self, width: f64, minimum: f64, columns: u32) -> Option<f64> {
        // Use the unpartitioned line advances to bracket a search at the actual
        // column width. The bracket stays the same when a fitted frame is refit.
        let tall = self.compose(
            Size {
                width,
                height: MAX_SIZE,
            },
            columns,
        )?;
        let lines = self.lines(&tall);
        let [first, second, .., last] = lines else {
            return None;
        };
        let advance = second.baseline - first.baseline;
        if tall.overset
            || advance < 1.0
            || lines
                .iter()
                .any(|line| line.table.is_some() || line.cell.is_some())
            || lines
                .windows(2)
                .any(|pair| (pair[1].baseline - pair[0].baseline - advance).abs() > PRECISION)
        {
            return None;
        }

        // Approximate the first line by a whole-point advance for the upper
        // estimate, and check the actual composition at each candidate height.
        let upper = self.margins.height + last.baseline - first.baseline + advance.floor();
        let bottom_inset = self.frame.frame.insets[2] + stroke_inset(self.frame.item);
        let rows_per_column = lines.len().div_ceil(columns as usize);
        let lower = if rows_per_column > 1 {
            minimum.max(lines[rows_per_column - 2].baseline - self.bounds.top + bottom_inset)
        } else {
            minimum
        };
        let fits = |height| self.fits(Size { width, height }, columns);
        if lower >= upper || !fits(upper) || (lower > minimum && (fits(minimum) || fits(lower))) {
            return None;
        }
        smallest(lower, lower, upper, COLUMN_HEIGHT_PRECISION, fits)
    }

    pub fn lines<'l>(&self, layout: &'l StoryLayout) -> &'l [LaidLine] {
        layout
            .frames
            .iter()
            .find(|f| f.frame == self.frame.item.id)
            .map_or(&[], |f| &f.lines)
    }

    pub fn fits(&self, size: Size, columns: u32) -> bool {
        self.compose(size, columns)
            .is_some_and(|layout| !layout.overset)
    }

    pub fn unbroken_width(&self) -> Option<f64> {
        let size = Size {
            width: MAX_SIZE,
            height: MAX_SIZE,
        };
        let layout = self.compose(size, 1)?;
        if layout.overset {
            return None;
        }
        let mut chars = Vec::new();
        for paragraph in &self.story.paragraphs {
            chars.extend(paragraph.runs.iter().flat_map(|run| run.text.chars()));
            chars.push('\r');
        }
        let width = self
            .lines(&layout)
            .iter()
            .map(|line| {
                let mut end = line.end.min(chars.len());
                while end > line.start && matches!(chars[end - 1], ' ' | '\r' | '\n') {
                    end -= 1;
                }
                let mut edges: Option<(f64, f64)> = None;
                for cluster in line.text_clusters.iter().filter(|c| c.start < end) {
                    let (l, r) = (cluster.x0.min(cluster.x1), cluster.x0.max(cluster.x1));
                    edges = Some(edges.map_or((l, r), |(left, right)| (left.min(l), right.max(r))));
                }
                let indents = size.width - self.margins.width - (line.right - line.left);
                edges.map_or(0.0, |(left, right)| (right - left + indents).max(0.0))
            })
            .fold(0.0, f64::max);
        Some(self.margins.width + width)
    }

    fn compose(&self, size: Size, columns: u32) -> Option<StoryLayout> {
        let mut item = self.frame.item.clone();
        let original = self.original();
        for p in &mut item.paths[0].points {
            let x = if p.anchor.x == self.bounds.left {
                self.bounds.left
            } else {
                self.bounds.left + size.width
            };
            let y = if p.anchor.y == self.bounds.top {
                self.bounds.top
            } else {
                self.bounds.top + size.height
            };
            *p = folio_model::PathPoint {
                anchor: Point::new(x, y),
                left: Point::new(x, y),
                right: Point::new(x, y),
            };
        }
        item.transform = item.transform.then_inner(&Affine::translate(
            self.point.0 * (original.width - size.width),
            self.point.1 * (original.height - size.height),
        ));
        if let ItemKind::TextFrame(frame) = &mut item.kind {
            frame.columns = columns;
        }
        layout_story(
            self.doc,
            &DocumentFrames::with_item(self.doc, &item),
            &self.story.id,
            self.fonts,
        )
    }
}

fn rectangle(item: &PageItem) -> Option<Rect> {
    let [path] = item.paths.as_slice() else {
        return None;
    };
    let bounds = path.bounds()?;
    if path.open || path.points.len() != 4 || bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return None;
    }
    for (i, p) in path.points.iter().enumerate() {
        let q = path.points[(i + 1) % 4].anchor;
        let Point { x, y } = p.anchor;
        if p.anchor != p.left
            || p.anchor != p.right
            || !(x == bounds.left || x == bounds.right)
            || !(y == bounds.top || y == bounds.bottom)
            || (x == q.x) == (y == q.y)
            || p.anchor == path.points[(i + 2) % 4].anchor
        {
            return None;
        }
    }
    Some(bounds)
}

fn anchored_span(anchor: f64, start: f64, end: f64, fraction: f64) -> f64 {
    if anchor < start || anchor > end {
        return 0.0;
    }
    let before = if fraction > 0.0 {
        (anchor - start) / fraction
    } else {
        f64::INFINITY
    };
    let after = if fraction < 1.0 {
        (end - anchor) / (1.0 - fraction)
    } else {
        f64::INFINITY
    };
    before.min(after)
}

pub(super) fn smallest(
    lower: f64,
    floor: f64,
    start: f64,
    precision: f64,
    mut fits: impl FnMut(f64) -> bool,
) -> Option<f64> {
    if !lower.is_finite()
        || !floor.is_finite()
        || !start.is_finite()
        || !precision.is_finite()
        || precision <= 0.0
    {
        return None;
    }
    if fits(lower) {
        return Some(lower);
    }
    let mut low = lower.max(floor);
    let limit = MAX_SIZE.max(low).max(start);
    if low > lower && fits(low) {
        return Some(low);
    }
    let mut high = start.max(low + 1.0).min(limit);
    while !fits(high) {
        if high >= limit {
            return None;
        }
        low = high;
        high = (high * 2.0).min(limit);
    }
    while high - low > precision {
        let mid = low + (high - low) * 0.5;
        if mid == low || mid == high {
            break;
        }
        if fits(mid) {
            high = mid;
        } else {
            low = mid;
        }
    }
    Some(high)
}
