use crate::xml::Element;
use crate::props::Props;
use folio_model::traps::{Assignment, Preset, Settings};

pub(crate) fn read(styles: &[&Element<'_>], spreads: &[&Element<'_>]) -> Settings {
    let presets = styles.iter().flat_map(|r| r.children_named("TrapPreset")).map(|e| {
        let text = |key| e.attr(key).unwrap_or_default().to_owned();
        let boolean = |key| e.attr(key).and_then(|v| match v { "true" => Some(true), "false" => Some(false), _ => None });
        Preset {
            id: text("Self"), name: text("Name"), default_width: e.f64("DefaultTrapWidth"), black_width: e.f64("BlackWidth"),
            join: text("TrapJoin"), end: text("TrapEnd"), objects_to_images: boolean("ObjectsToImages"),
            images_to_images: boolean("ImagesToImages"), internal_images: boolean("InternalImages"), one_bit_images: boolean("OneBitImages"),
            image_placement: text("ImagePlacement"), step_threshold: e.f64("StepThreshold"), black_color_threshold: e.f64("BlackColorThreshold"),
            black_density: e.f64("BlackDensity"), sliding_threshold: e.f64("SlidingTrapThreshold"), color_reduction: e.f64("ColorReduction"),
        }
    }).collect();
    let assignments = spreads.iter().filter_map(|r| r.child("Spread")).flat_map(|s| s.children_named("Page")).filter_map(|p| {
        Some(Assignment { page: p.attr("Self")?.into(), preset: p.attr("AppliedTrapPreset")?.into() })
    }).collect();
    Settings { presets, assignments }
}
