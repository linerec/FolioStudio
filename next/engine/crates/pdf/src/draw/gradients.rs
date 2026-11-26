//! Builds gradient ramps and registers their PDF shadings.

use super::Drawer;
use crate::colors::{Alternate, Col, CsDef};
use crate::registry::{RampStop, ShadingDef, ShadingSpace};
use folio_model::{GradientGeometry, GradientKind, Paint};

impl<'a, 'd> Drawer<'a, 'd> {
    /// The gradient's colours in one shading space (DeviceCMYK, a Separation, Lab, RGB, or a
    /// DeviceN of the inks it uses).
    fn ramp(&mut self, id: &str, item: &str) -> Option<(ShadingSpace, Vec<RampStop>)> {
        if let Some(hit) = self.reg.ramps.get(id) {
            return hit.clone();
        }
        let built = self.build_ramp(id, item);
        self.reg.ramps.insert(id.to_owned(), built.clone());
        built
    }

    fn build_ramp(&mut self, id: &str, item: &str) -> Option<(ShadingSpace, Vec<RampStop>)> {
        let doc = self.doc;
        let g = doc.gradients.get(id)?;
        let mut cols = Vec::new();
        for st in &g.stops {
            let r = self.reg.res.resolve(&Paint::Swatch { id: st.color.clone(), tint: 100.0 });
            cols.push(r.map(|r| r.col).unwrap_or(Col::Cmyk([0.0; 4])));
        }
        if cols.is_empty() {
            return None;
        }
        let curves: Vec<_> = g.stops.iter().map(|s| folio_color::GradientCurve::new(s.midpoint)).collect();
        let locs: Vec<f32> = g.stops.iter().map(|s| (s.location / 100.0).clamp(0.0, 1.0) as f32).collect();
        let make = |vals: Vec<Vec<f32>>| -> Vec<RampStop> {
            vals.into_iter().enumerate().map(|(i, v)| RampStop { location: locs[i], curve: curves[i], values: v }).collect()
        };
        // One space for all stops when possible.
        if cols.iter().all(|c| matches!(c, Col::Cmyk(_))) {
            let v = cols.iter().map(|c| if let Col::Cmyk(x) = c { x.to_vec() } else { vec![] }).collect();
            return Some((ShadingSpace::Cmyk, make(v)));
        }
        let spaces = &self.reg.res.spaces;
        let first_space = cols.iter().find_map(|c| if let Col::Space(i, _) = c { Some(*i) } else { None });
        if let Some(cs) = first_space {
            let paper_ok = matches!(spaces.defs[cs], CsDef::Separation { .. });
            let all_same = cols.iter().all(|c| match c {
                Col::Space(i, _) => *i == cs,
                Col::Cmyk(x) => paper_ok && x.iter().all(|v| *v == 0.0),
                _ => false,
            });
            if all_same && !matches!(spaces.defs[cs], CsDef::DeviceN { .. }) {
                let n = match &spaces.defs[cs] {
                    CsDef::Separation { .. } => 1,
                    _ => 3,
                };
                let v = cols
                    .iter()
                    .map(|c| match c {
                        Col::Space(_, v) => v.clone(),
                        _ => vec![0.0; n],
                    })
                    .collect();
                return Some((ShadingSpace::Space(cs), make(v)));
            }
        }
        // Mixed inks: a DeviceN of every ink the stops use.
        type InkBreakdown = ([f32; 4], Vec<(String, f32, [f32; 4])>);
        let mut breakdown: Vec<InkBreakdown> = Vec::new();
        for c in &cols {
            let mut cmyk = [0.0f32; 4];
            let mut spots: Vec<(String, f32, [f32; 4])> = Vec::new();
            match c {
                Col::Cmyk(x) => cmyk = *x,
                Col::Gray(gv) => cmyk[3] = 1.0 - gv,
                Col::Space(i, v) => match &self.reg.res.spaces.defs[*i] {
                    CsDef::Separation { ink, alt } => spots.push((ink.clone(), v[0], alt.approx_cmyk())),
                    CsDef::DeviceN { inks, cmyk: alts } => {
                        for (k, ink) in inks.iter().enumerate() {
                            match crate::colors::PROCESS_NAMES.iter().position(|p| p == ink) {
                                Some(ch) => cmyk[ch] = v[k],
                                None => spots.push((ink.clone(), v[k], alts[k])),
                            }
                        }
                    }
                    CsDef::Lab => {
                        self.reg.warn.add("colour", "Lab stop in a mixed gradient approximated as CMYK", Some(self.page), Some(item));
                        cmyk = Alternate::Lab([v[0], v[1], v[2]]).approx_cmyk();
                    }
                    _ => {
                        self.reg.warn.add("colour", "RGB stop in a mixed gradient approximated as CMYK", Some(self.page), Some(item));
                        cmyk = Alternate::Rgb([v[0], v[1], v[2]]).approx_cmyk();
                    }
                },
            }
            breakdown.push((cmyk, spots));
        }
        let mut process = [false; 4];
        let mut spot_inks: Vec<(String, [f32; 4])> = Vec::new();
        for (c, sp) in &breakdown {
            for k in 0..4 {
                process[k] |= c[k] > 0.0;
            }
            for (n, _, a) in sp {
                if !spot_inks.iter().any(|(x, _)| x == n) {
                    spot_inks.push((n.clone(), *a));
                }
            }
        }
        if spot_inks.is_empty() {
            let v = breakdown.iter().map(|(c, _)| c.to_vec()).collect();
            return Some((ShadingSpace::Cmyk, make(v)));
        }
        let mut inks = Vec::new();
        let mut alts = Vec::new();
        for k in 0..4 {
            if process[k] {
                inks.push(crate::colors::PROCESS_NAMES[k].to_owned());
                let mut u = [0.0f32; 4];
                u[k] = 1.0;
                alts.push(u);
            }
        }
        for (n, a) in &spot_inks {
            inks.push(n.clone());
            alts.push(*a);
        }
        let values: Vec<Vec<f32>> = breakdown
            .iter()
            .map(|(c, sp)| {
                let mut v: Vec<f32> = (0..4).filter(|k| process[*k]).map(|k| c[k]).collect();
                for (n, _) in &spot_inks {
                    v.push(sp.iter().find(|x| &x.0 == n).map(|x| x.1).unwrap_or(0.0));
                }
                v
            })
            .collect();
        let cs = self.reg.res.spaces.intern(CsDef::DeviceN { inks, cmyk: alts });
        Some((ShadingSpace::Space(cs), make(values)))
    }

    pub(super) fn shading(&mut self, id: &str, geo: &GradientGeometry, item: &str) -> Option<usize> {
        let kind = self.doc.gradients.get(id)?.kind;
        let (space, stops) = self.ramp(id, item)?;
        let (x, y) = (geo.start.x as f32, geo.start.y as f32);
        let len = geo.length.max(0.01) as f32;
        let coords = match kind {
            GradientKind::Linear => {
                // The angle is counter-clockwise on screen; inner y points down.
                let a = geo.angle.to_radians();
                vec![x, y, x + len * a.cos() as f32, y - len * a.sin() as f32]
            }
            GradientKind::Radial => vec![x, y, 0.0, x, y, len],
        };
        if geo.hilite_length.abs() > 1e-6 {
            self.reg.warn.add("colour", "radial gradient highlight not exported", Some(self.page), Some(item));
        }
        self.reg.counts.shadings += 1;
        Some(self.reg.add_shading(ShadingDef { radial: kind == GradientKind::Radial, coords, space, stops }))
    }
}
