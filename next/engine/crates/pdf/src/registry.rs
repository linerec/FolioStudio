//! Document-wide objects that content streams refer to: graphics states, forms
//! (transparency groups, placed PDF pages), shadings and patterns. Fonts, colour spaces and
//! images have their own registries.

use crate::colors::{Col, Resolver};
use crate::content::{GsSource, ResUse};
use crate::fonts::FontRegistry;
use crate::graphics::Graphics;
use crate::options::{Counts, Warning};
use folio_model::{Affine, BlendMode, Rect};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GsDef {
    /// OP / op with OPM 1.
    Overprint { fill: bool, stroke: bool },
    /// Constant alpha (thousandths) and blend mode of a transparency group.
    Blend { alpha: u32, mode: BlendMode },
    /// A luminosity soft mask (form index) with alpha and blend mode (effects).
    Mask { form: usize, alpha: u32, mode: BlendMode },
}

impl std::hash::Hash for GsDef {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        match self {
            GsDef::Overprint { fill, stroke } => (0u8, *fill, *stroke).hash(h),
            GsDef::Blend { alpha, mode } => (1u8, *alpha, *mode as u8).hash(h),
            GsDef::Mask { form, alpha, mode } => (2u8, *form as u32, *alpha, *mode as u8).hash(h),
        }
    }
}

pub struct GroupDef {
    pub isolated: bool,
    pub knockout: bool,
}

pub enum FormDef {
    /// A transparency group drawn by us.
    Group { content: Vec<u8>, used: ResUse, bbox: Rect, group: GroupDef },
    /// A page of a placed PDF (index into `Graphics::imported`).
    Imported(usize),
    /// A soft-mask group (luminosity, DeviceGray) drawing effect rasters.
    Mask { content: Vec<u8>, used: ResUse, bbox: Rect },
    /// A flattened region's rasters (index into the export's flattening jobs), written once
    /// they are rendered.
    Flat(usize),
}

/// One colour of a gradient ramp.
#[derive(Clone, Debug)]
pub struct RampStop {
    pub location: f32,
    /// Midpoint exponent towards this stop from the previous one.
    pub curve: folio_color::GradientCurve,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug)]
pub enum ShadingSpace {
    Cmyk,
    Space(usize),
}

#[derive(Clone, Debug)]
pub struct ShadingDef {
    pub radial: bool,
    /// Axial: x0 y0 x1 y1; radial: x0 y0 r0 x1 y1 r1.
    pub coords: Vec<f32>,
    pub space: ShadingSpace,
    pub stops: Vec<RampStop>,
}

#[derive(Clone, Debug)]
pub struct PatternDef {
    pub shading: usize,
    pub matrix: Affine,
}

/// Aggregated warnings: (kind, message) → entry.
#[derive(Default)]
pub struct Warn {
    pub map: BTreeMap<(String, String), Warning>,
}

impl Warn {
    pub fn add(&mut self, kind: &str, message: impl Into<String>, page: Option<usize>, item: Option<&str>) {
        let message = message.into();
        let e = self.map.entry((kind.to_owned(), message.clone())).or_insert_with(|| Warning {
            kind: kind.to_owned(),
            message,
            ..Default::default()
        });
        e.count += 1;
        if let Some(p) = page
            && e.pages.len() < 12
            && !e.pages.contains(&p)
        {
            e.pages.push(p);
        }
        if let Some(i) = item
            && e.items.len() < 12
            && !e.items.iter().any(|x| x == i)
        {
            e.items.push(i.to_owned());
        }
    }
}

pub struct Registry<'d> {
    pub res: Resolver<'d>,
    pub fonts: FontRegistry,
    pub gs: Vec<GsDef>,
    gs_index: HashMap<GsDef, usize>,
    pub forms: Vec<FormDef>,
    pub shadings: Vec<ShadingDef>,
    pub patterns: Vec<PatternDef>,
    pub graphics: Graphics<'d>,
    pub warn: Warn,
    pub counts: Counts,
    /// Gradient id → interned ramp (space + stops), shared by its uses.
    pub ramps: HashMap<String, Option<(ShadingSpace, Vec<RampStop>)>>,
    /// Text frames drawn at least once (coverage of the layout).
    pub drawn_frames: std::collections::HashSet<String>,
    /// Effect alpha rasters (soft mask images).
    pub rasters: Vec<crate::effects::Alpha>,
    /// Export Layers: the layers whose content is written (all of them with Acrobat layers).
    pub export_layers: crate::ExportLayers,
    /// Acrobat layers: document layer id → index of its optional content group (`None`: no Acrobat layers).
    pub ocg_of_layer: Option<HashMap<String, usize>>,
}

impl<'d> Registry<'d> {
    pub fn new(res: Resolver<'d>, fonts: FontRegistry, graphics: Graphics<'d>) -> Self {
        Self {
            res,
            fonts,
            gs: Vec::new(),
            gs_index: HashMap::new(),
            forms: Vec::new(),
            shadings: Vec::new(),
            patterns: Vec::new(),
            graphics,
            warn: Warn::default(),
            counts: Counts::default(),
            ramps: HashMap::new(),
            drawn_frames: Default::default(),
            rasters: Vec::new(),
            export_layers: crate::ExportLayers::VisiblePrintable,
            ocg_of_layer: None,
        }
    }

    pub fn gs(&mut self, d: GsDef) -> usize {
        if let Some(&i) = self.gs_index.get(&d) {
            return i;
        }
        self.gs.push(d.clone());
        self.gs_index.insert(d, self.gs.len() - 1);
        self.gs.len() - 1
    }

    pub fn add_form(&mut self, f: FormDef) -> usize {
        self.forms.push(f);
        self.forms.len() - 1
    }

    pub fn add_shading(&mut self, s: ShadingDef) -> usize {
        self.shadings.push(s);
        self.shadings.len() - 1
    }

    pub fn add_pattern(&mut self, p: PatternDef) -> usize {
        self.patterns.push(p);
        self.patterns.len() - 1
    }

    pub fn registration(&mut self, tint: f32) -> Col {
        self.res.registration(tint)
    }
}

impl GsSource for Registry<'_> {
    fn overprint_gs(&mut self, fill: bool, stroke: bool) -> usize {
        self.gs(GsDef::Overprint { fill, stroke })
    }
}
