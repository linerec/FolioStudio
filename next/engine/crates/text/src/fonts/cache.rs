//! Load faces on demand and cache their metrics and glyph outlines.

use super::*;

impl FontStore {
    /// Drops the shaped-segment cache (it is bounded; this frees it at once).
    pub fn clear_shaping_cache(&self) {
        self.shapes.clear();
    }

    /// Segments in the shaping cache.
    pub fn shaping_cache_len(&self) -> usize {
        self.shapes.len()
    }

    /// Faces found by discovery.
    pub fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    pub(super) fn loaded_face(&self, font: FontId) -> Option<Arc<LoadedFace>> {
        self.loaded.read().ok()?.get(font as usize).cloned()
    }

    /// Type ▸ Create Outlines: the polygon paths InDesign builds from `glyph` (font units, y up) and the face's
    /// units per em; `None` for faces the rule was not measured on (see `outlines.rs`).
    pub fn native_outline(&self, font: FontId, glyph: u16) -> Option<(f64, Vec<Vec<crate::outlines::Anchor>>)> {
        let loaded = self.loaded_face(font)?;
        let face = loaded.ttf()?;
        let paths = crate::outlines::glyph_paths(&face, glyph)?;
        Some((f64::from(face.units_per_em()), paths))
    }

    /// The faces loaded so far.
    pub fn snapshot(&self) -> Faces {
        Faces(self.loaded.read().map(|l| l.clone()).unwrap_or_default())
    }

    /// Metrics of a loaded face.
    pub fn metrics(&self, font: FontId) -> FaceMetrics {
        self.loaded_face(font).map_or(FaceMetrics::SYNTHETIC, |f| f.metrics)
    }

    /// The discovered face behind a font id (`None` for the synthetic face).
    pub fn face_info(&self, font: FontId) -> Option<&FaceInfo> {
        let entry = self.loaded_face(font)?.entry?;
        self.faces.get(entry)
    }

    /// Runs `f` on the parsed face (variation coordinates applied).
    pub fn with_face<R>(&self, font: FontId, f: impl FnOnce(&ttf_parser::Face<'_>) -> R) -> Option<R> {
        let face = self.loaded_face(font)?;
        let parsed = face.ttf()?;
        Some(f(&parsed))
    }

    /// Runs `f` on the shaping face (variation coordinates applied).
    pub(crate) fn with_shaping_face<R>(&self, font: FontId, f: impl FnOnce(&rustybuzz::Face<'_>) -> R) -> Option<R> {
        let face = self.loaded_face(font)?;
        let shaping = face.shaping_face()?;
        Some(f(&shaping))
    }

    /// Whether `font` is the built-in synthetic face.
    pub fn is_synthetic(&self, font: FontId) -> bool {
        self.loaded_face(font).is_none_or(|f| f.data.is_none())
    }

    fn push_loaded(&self, face: LoadedFace) -> FontId {
        let mut loaded = self.loaded.write().unwrap_or_else(|e| e.into_inner());
        loaded.push(Arc::new(face));
        (loaded.len() - 1) as FontId
    }

    pub(super) fn synthetic_face(&self, st: &mut State) -> FontId {
        if let Some(id) = st.synthetic {
            return id;
        }
        let id = self.push_loaded(LoadedFace {
            entry: None,
            data: None,
            index: 0,
            coords: Vec::new(),
            metrics: FaceMetrics::SYNTHETIC,
        });
        st.synthetic = Some(id);
        id
    }

    pub(super) fn load_best(&self, st: &mut State, cands: &[usize], want_cff: Option<bool>) -> Option<FontId> {
        let mut ranked: Vec<usize> = cands.to_vec();
        ranked.sort_unstable();
        ranked.dedup();
        ranked.sort_by_key(|&i| {
            let f = &self.faces[i];
            (want_cff.is_some_and(|c| c != f.cff), f.source, i)
        });
        ranked.into_iter().find_map(|i| self.load_entry(st, i))
    }

    pub(super) fn load_entry(&self, st: &mut State, entry: usize) -> Option<FontId> {
        if let Some(&id) = st.loaded_entry.get(&entry) {
            return Some(id);
        }
        let info = &self.faces[entry];
        let file = File::open(info.path.as_path()).ok()?;
        // SAFETY: font files are opened read-only and not modified while mapped; a font
        // being replaced underneath is the same hazard every font engine accepts.
        let data = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => FontData::Mapped(m),
            Err(_) => FontData::Owned(std::fs::read(info.path.as_path()).ok()?),
        };
        let coords = info.coords.clone();
        let metrics = face_metrics(data.bytes(), info.index, &coords)?;
        let id = self.push_loaded(LoadedFace { entry: Some(entry), data: Some(data), index: info.index, coords, metrics });
        st.loaded_entry.insert(entry, id);
        Some(id)
    }
}

impl GlyphSource for FontStore {
    fn outline(&self, font: FontId, glyph: u16) -> Option<Arc<Vec<OutlineCmd>>> {
        let shard = &self.outlines[(font as usize * 31 + glyph as usize) % OUTLINE_SHARDS];
        if let Ok(cache) = shard.read()
            && let Some(hit) = cache.get(&(font, glyph))
        {
            return hit.clone();
        }
        let built = self.build_outline(font, glyph);
        if let Ok(mut cache) = shard.write() {
            cache.insert((font, glyph), built.clone());
        }
        built
    }
}

impl FontStore {
    fn build_outline(&self, font: FontId, glyph: u16) -> Option<Arc<Vec<OutlineCmd>>> {
        let loaded = self.loaded_face(font)?;
        let face = loaded.ttf()?;
        let scale = 1.0 / f32::from(face.units_per_em());
        let mut b = OutlineCollector { scale, cmds: Vec::new() };
        face.outline_glyph(ttf_parser::GlyphId(glyph), &mut b)?;
        (!b.cmds.is_empty()).then(|| Arc::new(b.cmds))
    }
}

struct OutlineCollector {
    scale: f32,
    cmds: Vec<OutlineCmd>,
}

impl ttf_parser::OutlineBuilder for OutlineCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        let s = self.scale;
        self.cmds.push(OutlineCmd::MoveTo(x * s, y * s));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let s = self.scale;
        self.cmds.push(OutlineCmd::LineTo(x * s, y * s));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let s = self.scale;
        self.cmds.push(OutlineCmd::QuadTo(x1 * s, y1 * s, x * s, y * s));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let s = self.scale;
        self.cmds.push(OutlineCmd::CubicTo(x1 * s, y1 * s, x2 * s, y2 * s, x * s, y * s));
    }
    fn close(&mut self) {
        self.cmds.push(OutlineCmd::Close);
    }
}

/// The ideographic em-box bottom from `BASE` (horizontal axis, first script, 'ideo').
fn base_ideo(face: &ttf_parser::Face<'_>) -> Option<i16> {
    let b = face.raw_face().table(ttf_parser::Tag::from_bytes(b"BASE"))?;
    let u16at = |at: usize| -> Option<usize> { Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]) as usize) };
    let axis = u16at(4)?;
    if axis == 0 {
        return None;
    }
    let tags = axis + u16at(axis)?;
    let scripts = axis + u16at(axis + 2)?;
    let ntags = u16at(tags)?;
    let ideo = (0..ntags).position(|i| b.get(tags + 2 + i * 4..tags + 6 + i * 4) == Some(b"ideo"))?;
    let script = scripts + u16at(scripts + 2 + 4)?;
    let values = script + u16at(script)?;
    if values == script {
        return None;
    }
    let coord = values + u16at(values + 4 + ideo * 2)?;
    Some(u16at(coord + 2)? as u16 as i16)
}

fn outline_top(face:&ttf_parser::Face<'_>,glyph:ttf_parser::GlyphId)->Option<f64> {
    struct Top {y:f64,start:f64,top:f64}
    impl ttf_parser::OutlineBuilder for Top {
        fn move_to(&mut self,_:f32,y:f32){self.y=y as f64;self.start=self.y;self.top=self.top.max(self.y);}
        fn line_to(&mut self,_:f32,y:f32){self.y=y as f64;self.top=self.top.max(self.y);}
        fn quad_to(&mut self,_:f32,y1:f32,_:f32,y:f32){
            let (a,b,c)=(self.y,y1 as f64,y as f64);
            let d=a-2.0*b+c;
            if d.abs()>1e-12 {let t=(a-b)/d;if t>0.0&&t<1.0 {let u=1.0-t;self.top=self.top.max(u*u*a+2.0*u*t*b+t*t*c);}}
            self.y=c;self.top=self.top.max(c);
        }
        fn curve_to(&mut self,_:f32,y1:f32,_:f32,y2:f32,_:f32,y:f32){
            let (p,q,r,s)=(self.y,y1 as f64,y2 as f64,y as f64);
            let (a,b,c)=(-p+3.0*q-3.0*r+s,2.0*(p-2.0*q+r),q-p);
            let mut ts=Vec::new();
            if a.abs()<1e-12 {if b.abs()>1e-12 {ts.push(-c/b);}}
            else {let d=b*b-4.0*a*c;if d>=0.0 {ts.push((-b+d.sqrt())/(2.0*a));ts.push((-b-d.sqrt())/(2.0*a));}}
            for t in ts {if t>0.0&&t<1.0 {let u=1.0-t;self.top=self.top.max(u*u*u*p+3.0*u*u*t*q+3.0*u*t*t*r+t*t*t*s);}}
            self.y=s;self.top=self.top.max(s);
        }
        fn close(&mut self){self.y=self.start;}
    }
    let mut top=Top{y:0.0,start:0.0,top:f64::NEG_INFINITY};
    face.outline_glyph(glyph,&mut top)?;
    top.top.is_finite().then_some(top.top)
}

fn face_metrics(data: &[u8], index: u32, coords: &[([u8; 4], f32)]) -> Option<FaceMetrics> {
    let mut face = ttf_parser::Face::parse(data, index).ok()?;
    for (tag, v) in coords {
        face.set_variation(ttf_parser::Tag::from_bytes(tag), *v);
    }
    let upem = f64::from(face.units_per_em().max(1));
    let em = |v: i16| f64::from(v) / upem;
    let top_of = |c: char| {
        face.glyph_index(c).and_then(|g| face.glyph_bounding_box(g)).map(|b| f64::from(b.y_max) / upem)
    };
    let cap_height = face.capital_height().filter(|&v| v > 0).map(em).or_else(|| top_of('H')).unwrap_or(0.7);
    let x_height = face.x_height().filter(|&v| v > 0).map(em).or_else(|| top_of('x')).unwrap_or(0.5);
    let space_advance =
        face.glyph_index(' ').and_then(|g| face.glyph_hor_advance(g)).map_or(0.25, |a| f64::from(a) / upem);
    let typo_ascender = face.typographic_ascender().map_or(em(face.ascender()), em);
    let typo_descender = face.typographic_descender().map_or(em(face.descender()), em);
    let hangul = face.glyph_index('\u{AC00}').is_some();
    let cjk = hangul || face.glyph_index('\u{4E00}').is_some() || face.glyph_index('\u{3042}').is_some();
    let ascent = if hangul {
        // CID-keyed: the glyph of CID 69 (Adobe-Korea1 / Adobe-Japan1 'd').
        let n = face.number_of_glyphs();
        let by_cid = face.tables().cff.and_then(|cff| {
            let gid = (69..n).chain(0..69.min(n)).find(|&g| cff.glyph_cid(ttf_parser::GlyphId(g)) == Some(69))?;
            face.glyph_bounding_box(ttf_parser::GlyphId(gid)).map(|b| f64::from(b.y_max) / upem)
        });
        by_cid.or_else(|| top_of('d')).unwrap_or(typo_ascender)
    } else if cjk {
        // text-multilingual: Japanese/Chinese Ascent also uses the actual top
        // of 'd', not OS/2 sTypoAscender. Keep fractional curve extrema.
        face.glyph_index('d').and_then(|g| outline_top(&face,g)).map(|v|v/upem).unwrap_or(typo_ascender)
    } else {
        typo_ascender
    };
    // A Roman font's em box is centred on half its cap height: InDesign 2025 put the "Em Box"
    // first baseline and the em box anchored-object references of Arial at 0.858 em
    // ((1 + 0.716) / 2), Georgia 0.846 and Times New Roman 0.831 (content-drops specimens).
    // A CJK font without a BASE table: its line metrics when they span exactly an em (Gulim,
    // Batang, SimSun: 879 / 145 of 1024, 220 / 36 of 256), otherwise an em centred on the
    // ideograph 永 (Malgun Gothic 1798 of 2048, Microsoft YaHei 1756: content-drops em box
    // specimen, first baseline "Em Box Height", and the kenten above and below the text).
    let hhea = face.tables().hhea;
    let line_em = i32::from(hhea.ascender) - i32::from(hhea.descender) == i32::from(face.units_per_em());
    let yong = || {
        let b = face.glyph_index('\u{6C38}').and_then(|g| face.glyph_bounding_box(g))?;
        Some((f64::from(b.y_min) + f64::from(b.y_max)) / 2.0 / upem + 0.5)
    };
    let embox_top = match base_ideo(&face) {
        Some(ideo) => 1.0 + em(ideo),
        None if !cjk => (1.0 + cap_height) / 2.0,
        None if line_em => em(hhea.ascender),
        None => match yong() {
            Some(top) => top,
            None if typo_ascender - typo_descender > 0.0 => typo_ascender / (typo_ascender - typo_descender),
            None => 0.88,
        },
    };
    let underline = face.underline_metrics().filter(|m| m.thickness > 0);
    let strikeout = face.strikeout_metrics().filter(|m| m.thickness > 0);
    Some(FaceMetrics {
        underline_position: underline.map_or(-0.1, |m| em(m.position)),
        underline_thickness: underline.map_or(0.05, |m| em(m.thickness)),
        strikeout_position: strikeout.map_or(x_height / 2.0, |m| em(m.position)),
        strikeout_thickness: strikeout.map_or(0.05, |m| em(m.thickness)),
        units_per_em: upem,
        ascender: em(face.ascender()),
        descender: em(face.descender()),
        line_gap: em(face.line_gap()),
        typo_ascender,
        typo_descender,
        ascent,
        cap_height,
        x_height,
        embox_top,
        space_advance,
        bbox_top: em(face.global_bounding_box().y_max),
        bbox_bottom: -em(face.global_bounding_box().y_min),
    })
}
