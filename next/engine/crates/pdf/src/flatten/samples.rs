use std::io::Write;

use flate2::{write::ZlibEncoder, Compression};
use folio_render::sep::AreaPlanes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TileEncoding {
    Flate,
    Png,
    Dct,
}

/// Scanlines reused by a tile worker. Most tiles never need a second full set of samples.
#[derive(Default)]
pub(crate) struct SampleBuffer {
    row: Vec<u8>,
    previous: Vec<u8>,
    sub: Vec<u8>,
    up: Vec<u8>,
}

impl SampleBuffer {
    pub(crate) fn encode(&mut self, planes: &AreaPlanes, plates: &[usize], level: u32, jpeg: bool) -> (TileEncoding, Vec<u8>) {
        let width = planes.width as usize;
        let height = planes.height as usize;
        let channels = plates.len();
        let row_bytes = width * channels;
        let raw_bytes = row_bytes * height;
        let source: Vec<&[u8]> = plates.iter().map(|&plate| planes.plane(plate)).collect();
        self.row.resize(row_bytes, 0);
        let compression = Compression::new(level.min(9));
        let mut plain = ZlibEncoder::new(Vec::new(), compression);
        let mut png = (channels <= 4 && width >= 16 && height >= 16).then(|| {
            self.previous.resize(row_bytes, 0);
            self.previous.fill(0);
            self.sub.resize(row_bytes, 0);
            self.up.resize(row_bytes, 0);
            ZlibEncoder::new(Vec::new(), compression)
        });
        for y in 0..height {
            interleave(&source, width, y, &mut self.row);
            plain.write_all(&self.row).expect("compress tile row");
            if let Some(png) = &mut png {
                self.write_prediction(png, channels);
            }
        }
        let plain = plain.finish().expect("finish tile compression");
        if jpeg && plain.len() > raw_bytes / 10 && (16..=65_500).contains(&width) && (16..=65_500).contains(&height) {
            let mut raw = vec![0; raw_bytes];
            for (y, row) in raw.chunks_exact_mut(row_bytes).enumerate() {
                interleave(&source, width, y, row);
            }
            let mut data = Vec::new();
            let mut encoder = jpeg_encoder::Encoder::new(&mut data, 95);
            encoder.set_optimized_huffman_tables(true);
            if encoder.encode(&raw, width as u16, height as u16, jpeg_encoder::ColorType::CmykAsYcck).is_ok() && data.len() < plain.len() / 2 {
                return (TileEncoding::Dct, data);
            }
        }
        if let Some(png) = png {
            let png = png.finish().expect("finish tile prediction");
            // Allow for the extra DecodeParms dictionary in the PDF.
            if png.len() + 80 < plain.len() {
                return (TileEncoding::Png, png);
            }
        }
        (TileEncoding::Flate, plain)
    }

    fn write_prediction(&mut self, out: &mut impl Write, channels: usize) {
        let mut costs = [0, 0, 0];
        let magnitude = |v: u8| (v as i8).unsigned_abs() as usize;
        let left = std::iter::repeat_n(0, channels).chain(self.row.iter().copied());
        for (i, (&value, left)) in self.row.iter().zip(left).enumerate() {
            let sub = value.wrapping_sub(left);
            let up = value.wrapping_sub(self.previous[i]);
            self.sub[i] = sub;
            self.up[i] = up;
            costs[0] += magnitude(value);
            costs[1] += magnitude(sub);
            costs[2] += magnitude(up);
        }
        let filter = costs.iter().enumerate().min_by_key(|(_, cost)| *cost).expect("tile prediction costs").0;
        out.write_all(&[filter as u8]).expect("compress tile filter");
        out.write_all([&self.row, &self.sub, &self.up][filter]).expect("compress predicted tile row");
        std::mem::swap(&mut self.row, &mut self.previous);
    }
}

fn interleave(source: &[&[u8]], width: usize, y: usize, row: &mut [u8]) {
    let start = y * width;
    for (ink, plane) in source.iter().enumerate() {
        for (sample, &value) in row[ink..].iter_mut().step_by(source.len()).zip(&plane[start..start + width]) {
            *sample = value;
        }
    }
}
