//! QR content, module matrices, and vector geometry.
use crate::{PathPoint, Point, Rect, SubPath};

mod content;
mod encode;
mod eps;
mod mask;

pub use encode::encode;
pub use eps::{from_indesign_eps, indesign_eps_color};

pub const QUIET_ZONE: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QrContent {
    PlainText(String),
    Hyperlink(String),
    Email {
        address: String,
        subject: String,
        body: String,
    },
    TextMessage {
        number: String,
        message: String,
    },
    VCard(VCard),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VCard {
    pub first_name: String,
    pub last_name: String,
    pub job_title: String,
    pub cell_phone: String,
    pub phone: String,
    pub email: String,
    pub organisation: String,
    pub street_address: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
    pub website: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrModules {
    pub size: usize,
    pub dark: Vec<bool>,
}

impl QrModules {
    pub fn get(&self, x: usize, y: usize) -> bool {
        self.dark[y * self.size + x]
    }
}

// R10
pub fn module_paths(modules: &QrModules, bounds: &Rect) -> Vec<SubPath> {
    let step = bounds.width() / (modules.size + 2 * QUIET_ZONE) as f64;
    let y_at = |y| bounds.top + (y + QUIET_ZONE) as f64 * step;
    let mut paths = Vec::new();
    for x in 0..modules.size {
        let left = bounds.left + (x + QUIET_ZONE) as f64 * step;
        let right = left + step;
        let mut y = 0;
        while y < modules.size {
            if !modules.get(x, y) {
                y += 1;
                continue;
            }
            let top = y_at(y);
            while y < modules.size && modules.get(x, y) {
                y += 1;
            }
            let bottom = y_at(y);
            let points = [(left, top), (left, bottom), (right, bottom), (right, top)]
                .into_iter()
                .map(|(x, y)| {
                    let anchor = Point::new(x, y);
                    PathPoint {
                        anchor,
                        left: anchor,
                        right: anchor,
                    }
                })
                .collect();
            paths.push(SubPath {
                points,
                open: false,
            });
        }
    }
    paths
}
