use super::{BAND_COUNT, Bands, LAST_BAND, UNITS_PER_EM};
use ttf_parser::{Face, OutlineBuilder};

#[derive(Clone, Copy)]
pub(super) struct VerticalGuides {
    pub baseline: i32,
    pub lowercase: i32,
    pub capital: i32,
    pub zone: i32,
}

impl VerticalGuides {
    // R5
    pub fn from_face(face: &Face<'_>) -> Self {
        let to_grid = 1000.0 / f64::from(face.units_per_em());
        let bounds = |ch| face.glyph_bounding_box(face.glyph_index(ch)?);
        let upper = |ch| {
            bounds(ch)
                .map_or(666, |b| (f64::from(b.y_max) * to_grid).ceil() as i32)
                .clamp(0, 708)
        };
        let h = upper('H');
        let x = upper('x');
        let d = bounds('p')
            .map_or(250, |b| (-f64::from(b.y_min) * to_grid).ceil() as i32)
            .clamp(0, 1000 - h);
        Self {
            baseline: (d / 10).min(LAST_BAND),
            lowercase: ((d + x) / 10).min(LAST_BAND),
            capital: ((d + h) / 10).min(LAST_BAND),
            zone: h / 100 + 2,
        }
    }
}

pub(super) struct SampledGlyph {
    pub left: Bands,
    pub right: Bands,
    pub bottom: i32,
    pub top: i32,
    pub width: f64,
    pub left_bearing: f64,
    pub right_bearing: f64,
}

pub(super) struct Outline {
    // Horizontal coordinates are in em and vertical coordinates in 0.001 em.
    segments: Vec<Segment>,
    advance: f64,
}

impl Outline {
    pub fn read(face: &Face<'_>, ch: char) -> Option<Self> {
        let id = face.glyph_index(ch)?;
        let to_em = 1.0 / f64::from(face.units_per_em());
        let mut pen = Pen {
            to_em,
            ..Pen::default()
        };
        face.outline_glyph(id, &mut pen)?;
        Some(Self {
            segments: pen.segments,
            advance: f64::from(face.glyph_hor_advance(id).unwrap_or(0)) * to_em,
        })
    }

    // R5
    pub fn sample(&self, baseline: i32) -> Option<SampledGlyph> {
        let mut overall = Intersections::default();
        for segment in &self.segments {
            let (first, end) = segment.rows(0.0);
            if first < end {
                // X is linear between the first and last grid crossings on a segment.
                overall.add(segment.x_at(f64::from(first)));
                overall.add(segment.x_at(f64::from(end - 1)));
            }
        }
        if overall.count == 0 {
            return None;
        }

        const ROWS_PER_BAND: usize = 10;
        const ROW_COUNT: usize = BAND_COUNT * ROWS_PER_BAND;
        let start = -baseline * ROWS_PER_BAND as i32;
        let mut rows = [Intersections::default(); ROW_COUNT];
        for segment in &self.segments {
            let (first, end) = segment.rows(0.5);
            for row in first.max(start)..end.min(start + ROW_COUNT as i32) {
                rows[(row - start) as usize].add(segment.x_at(f64::from(row) + 0.5));
            }
        }

        let width = overall.max - overall.min;
        let units = |distance: f64| (distance * UNITS_PER_EM).round() as i32;
        let mut glyph = SampledGlyph {
            left: [units(width); BAND_COUNT],
            right: [units(width); BAND_COUNT],
            bottom: 0,
            top: 0,
            width,
            left_bearing: overall.min + 0.001,
            right_bearing: self.advance - overall.max + 0.001,
        };
        let mut occupied = false;
        for (band, rows) in rows.chunks_exact(ROWS_PER_BAND).enumerate() {
            let mut bounds = Intersections::default();
            for row in rows.iter().filter(|row| row.count >= 2) {
                bounds.add(row.min);
                bounds.add(row.max);
            }
            if bounds.count > 0 {
                glyph.left[band] = units(bounds.min - overall.min);
                glyph.right[band] = units(overall.max - bounds.max);
                if !occupied {
                    glyph.bottom = band as i32;
                    occupied = true;
                }
                glyph.top = band as i32;
            }
        }
        Some(glyph)
    }
}

#[derive(Clone, Copy)]
struct Intersections {
    min: f64,
    max: f64,
    count: usize,
}

impl Default for Intersections {
    fn default() -> Self {
        Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            count: 0,
        }
    }
}

impl Intersections {
    fn add(&mut self, x: f64) {
        self.min = self.min.min(x);
        self.max = self.max.max(x);
        self.count += 1;
    }
}

#[derive(Clone, Copy, Default)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

struct Segment {
    low: Point,
    high: Point,
}

impl Segment {
    fn rows(&self, offset: f64) -> (i32, i32) {
        (
            (self.low.y - offset).ceil() as i32,
            (self.high.y - offset).ceil() as i32,
        )
    }

    fn x_at(&self, y: f64) -> f64 {
        let ratio = (y - self.low.y) / (self.high.y - self.low.y);
        self.low.x + (self.high.x - self.low.x) * ratio
    }
}

#[derive(Default)]
struct Pen {
    to_em: f64,
    start: Point,
    current: Point,
    segments: Vec<Segment>,
}

impl Pen {
    fn point(&self, x: f32, y: f32) -> Point {
        Point {
            x: f64::from(x) * self.to_em,
            y: f64::from(y) * self.to_em * 1000.0,
        }
    }

    fn line(&mut self, end: Point) {
        let start = std::mem::replace(&mut self.current, end);
        if start.y < end.y {
            self.segments.push(Segment {
                low: start,
                high: end,
            });
        } else if end.y < start.y {
            self.segments.push(Segment {
                low: end,
                high: start,
            });
        }
    }
}

impl OutlineBuilder for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.current = self.point(x, y);
        self.start = self.current;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.line(self.point(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let start = self.current;
        let control = self.point(x1, y1);
        let end = self.point(x, y);
        for i in 1..12 {
            let t = f64::from(i) / 12.0;
            self.line(start.lerp(control, t).lerp(control.lerp(end, t), t));
        }
        self.line(end);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let start = self.current;
        let first = self.point(x1, y1);
        let second = self.point(x2, y2);
        let end = self.point(x, y);
        for i in 1..16 {
            let t = f64::from(i) / 16.0;
            let a = start.lerp(first, t);
            let b = first.lerp(second, t);
            let c = second.lerp(end, t);
            self.line(a.lerp(b, t).lerp(b.lerp(c, t), t));
        }
        self.line(end);
    }

    fn close(&mut self) {
        self.line(self.start);
    }
}
