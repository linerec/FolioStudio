use folio_color::{Colors, ProfileIndex, ProofSpec, icc::Intent, read_profile};
use folio_model::layout::{DocumentLayout, FontId, GlyphSource, OutlineCmd};
use folio_model::{
    Affine, ColorModel, ColorSpace, Document, ItemKind, Page, PageItem, Paint, PathPoint, Point,
    Rect, Spread, SubPath, Swatch,
};
use folio_render::{GlyphCache, ImageCache, Raster, Scene, render_page};
use serde_json::Value;

struct NoGlyphs;
impl GlyphSource for NoGlyphs {
    fn outline(&self, _: FontId, _: u16) -> Option<std::sync::Arc<Vec<OutlineCmd>>> {
        None
    }
}

fn document(input: &Value) -> Document {
    let mut doc = Document::default();
    let mut spread = Spread {
        pages: vec![Page {
            bounds: Rect {
                right: input["page"][0].as_f64().unwrap(),
                bottom: input["page"][1].as_f64().unwrap(),
                ..Default::default()
            },
            ..Default::default()
        }],
        ..Default::default()
    };
    for patch in input["patches"].as_array().unwrap() {
        let id = patch["name"].as_str().unwrap().to_owned();
        let space = match patch["space"].as_str().unwrap() {
            "CMYK" => ColorSpace::Cmyk,
            "RGB" => ColorSpace::Rgb,
            other => panic!("unexpected color space {other}"),
        };
        doc.swatches.insert(
            id.clone(),
            Swatch {
                id: id.clone(),
                name: id.clone(),
                model: ColorModel::Process,
                space,
                values: patch["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect(),
                base: None,
                tint: None,
                display_rgb: [0; 3],
            },
        );
        let points = [(-12.0, -12.0), (12.0, -12.0), (12.0, 12.0), (-12.0, 12.0)].map(|(x, y)| {
            let anchor = Point::new(x, y);
            PathPoint {
                anchor,
                left: anchor,
                right: anchor,
            }
        });
        spread.items.push(PageItem {
            id: id.clone(),
            kind: ItemKind::Rectangle { content: None },
            transform: Affine::translate(
                patch["at"][0].as_f64().unwrap(),
                patch["at"][1].as_f64().unwrap(),
            ),
            paths: vec![SubPath {
                points: points.into(),
                open: false,
            }],
            fill: Paint::Swatch { id, tint: 100.0 },
            visible: true,
            opacity: 1.0,
            ..Default::default()
        });
    }
    doc.spreads.push(spread);
    doc
}

fn sample(raster: &Raster, point: &Value, side: usize) -> [u8; 3] {
    let center = |axis: usize| point[axis].as_u64().unwrap() as usize;
    let half = side / 2;
    let mut channels = [const { Vec::new() }; 3];
    for y in center(1) - half..center(1) - half + side {
        for x in center(0) - half..center(0) - half + side {
            let start = (y * raster.width as usize + x) * 4;
            let pixel = &raster.rgba[start..start + 4];
            assert_eq!(pixel[3], 255);
            for (values, value) in channels.iter_mut().zip(pixel) {
                values.push(*value);
            }
        }
    }
    channels.map(|mut values| *values.select_nth_unstable(side * side / 2).1)
}

#[cfg(not(has_reference_vectors))]
#[test]
#[ignore = "optional reference vectors are missing"]
fn proof_preview_matches_measured_color_patches() { panic!("optional reference vectors are missing"); }

#[cfg(has_reference_vectors)]
#[test]
fn proof_preview_matches_measured_color_patches() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../../cleanroom/vectors/B7/cr-03.json"
    ))
    .unwrap();
    let common = &vectors["commonInput"];
    let cases = vectors["cases"].as_array().unwrap();
    let side = vectors["sampleSidePixels"].as_u64().unwrap() as usize;
    let installed = ProfileIndex::scan_installed();
    let profile = |name: &Value| {
        installed
            .find(name.as_str().unwrap())
            .map(|path| read_profile(&path).unwrap())
    };
    let Some(document_cmyk) = profile(&common["cmykProfile"]) else {
        eprintln!("SKIPPED: {}", common["cmykProfile"]);
        return;
    };
    let doc = document(common);
    let layout = DocumentLayout::default();
    let glyph_cache = GlyphCache::new();
    let images = ImageCache::new();
    let mut compared = 0;
    let mut matched = 0;
    let mut baseline_matches = 0;
    let mut regressions = Vec::new();
    for case in cases {
        let input = &case["input"];
        let enabled = input["enabled"].as_bool().unwrap();
        let colors = if enabled {
            let Some(device) = profile(&input["profile"]) else {
                eprintln!("SKIPPED: {}", input["profile"]);
                continue;
            };
            let spec = ProofSpec {
                preserve_numbers: input["preserve"].as_bool().unwrap(),
                simulate_black: input["black"].as_bool().unwrap(),
                simulate_paper: input["paper"].as_bool().unwrap(),
                intent: Intent::Relative,
            };
            Colors::proofed(&device, Some(&document_cmyk), None, spec).unwrap()
        } else {
            Colors::from_profiles(Some(&document_cmyk), None).unwrap()
        };
        let scene = Scene {
            doc: &doc,
            colors: &colors,
            layout: &layout,
            glyphs: &NoGlyphs,
            glyph_cache: &glyph_cache,
            images: &images,
        };
        let raster = render_page(&scene, 0, 1.0, None).unwrap();
        let expected = &case["output"]["inDesign"]["samples"];
        let comparison = &case["output"]["comparison"];
        let delta = |id: &str, at: &Value| {
            let actual = sample(&raster, at, side);
            actual
                .into_iter()
                .enumerate()
                .map(|(c, value)| {
                    u64::from(value.abs_diff(expected[id][c].as_u64().unwrap() as u8))
                })
                .max()
                .unwrap()
        };
        let mut max_delta = [0, 0, delta("paper", &common["paperAt"])];
        for patch in common["patches"].as_array().unwrap() {
            let bucket = usize::from(patch["space"] == "RGB");
            max_delta[bucket] =
                max_delta[bucket].max(delta(patch["name"].as_str().unwrap(), &patch["at"]));
        }
        let mut within_tolerance = true;
        for (bucket, actual) in ["CMYK", "RGB", "paper"].into_iter().zip(max_delta) {
            let tolerance = vectors["tolerance"][bucket].as_u64().unwrap();
            let baseline = comparison["maxChannelDelta"][bucket].as_u64().unwrap();
            within_tolerance &= actual <= tolerance;
            if actual > tolerance.max(baseline) || (bucket == "paper" && actual > tolerance) {
                regressions.push(format!(
                    "{} {bucket}: {actual} (tolerance {tolerance}, baseline {baseline})",
                    case["id"]
                ));
            }
        }
        eprintln!("{} CMYK/RGB/paper = {max_delta:?}", case["id"]);
        compared += 1;
        matched += usize::from(within_tolerance);
        baseline_matches += usize::from(comparison["matches"].as_bool().unwrap());
    }
    eprintln!("CR-03: {matched}/{compared}, baseline {baseline_matches}/{compared}");
    assert!(regressions.is_empty(), "{}", regressions.join("\n"));
    assert!(matched >= baseline_matches);
}
