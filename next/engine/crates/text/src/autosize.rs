mod trial;

use std::collections::HashSet;

use folio_model::{AutoSizeKind, Document, ItemKind};

use crate::{DocumentFrames, FontStore, layout_story};
use trial::{Size, Trial};

#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    pub frame: String,
    pub width: f64,
    pub height: f64,
    pub point: (f64, f64),
}

pub fn fit_frames(doc: &Document, fonts: &FontStore, stories: &[&str]) -> Vec<Fit> {
    let frames = DocumentFrames::new(doc);
    let mut seen = HashSet::new();
    stories
        .iter()
        .filter_map(|&id| {
            let story = doc.stories.get(id)?;
            if story.paragraphs.len() <= 1
                && story
                    .paragraphs
                    .iter()
                    .flat_map(|p| &p.runs)
                    .all(|r| r.text.is_empty())
            {
                return None;
            }
            let last = frames.frame(frames.chain(id).last()?)?;
            if !seen.insert(last.item.id.as_str())
                || !matches!(last.item.kind, ItemKind::TextFrame(_))
            {
                return None;
            }
            let options = last.frame.auto_size.as_ref()?;
            let spread = frames
                .spread_of(&last.item.id)
                .and_then(|index| doc.spreads.iter().chain(&doc.master_spreads).nth(index));
            let trial = Trial::new(doc, fonts, story, last, spread, options.point)?;
            let layout = layout_story(doc, &frames, id, fonts)?;
            if !layout.overset && trial.lines(&layout).is_empty() {
                return None;
            }
            let fit = find_size(&trial, options)?;
            let original = trial.original();
            if (fit.width - original.width).abs() < trial::PRECISION
                && (fit.height - original.height).abs() < trial::PRECISION
            {
                return None;
            }
            Some(Fit {
                frame: last.item.id.clone(),
                width: fit.width,
                height: fit.height,
                point: options.point,
            })
        })
        .collect()
}

fn find_size(trial: &Trial<'_>, options: &folio_model::AutoSize) -> Option<Size> {
    use AutoSizeKind::*;

    let original = trial.original();
    let columns = trial.columns();
    let min = trial.minimum(options.min_width, options.min_height);
    let unbroken = trial.fixed_width().is_none() && options.no_line_breaks && columns == 1;
    let width = |height, start| {
        if trial.fixed_width().is_some() {
            return trial.fit_fixed_width(height, min.width);
        }
        let lower = if unbroken {
            min.width.max(trial.unbroken_width()?)
        } else {
            min.width
        };
        let precision = if unbroken {
            trial::PRECISION
        } else {
            trial::WIDTH_PRECISION
        };
        let width = trial::smallest(lower, trial.width_floor(), start, precision, |width| {
            trial.fits(Size { width, height }, columns)
        })?;
        Some(Size { width, height })
    };
    match options.kind {
        HeightOnly => trial.fit_height(original.width, min.height, columns),
        WidthOnly => width(original.height, original.width),
        HeightAndWidth => {
            let (width, columns) = if trial.fixed_width().is_some() {
                let columns = columns.max(trial.minimum_columns(min.width)?);
                (trial.column_path_width(columns)?, columns)
            } else {
                let fitted = match trial.pasteboard_room() {
                    Some(room) => width(room.height.max(original.height), room.width)
                        .or_else(|| width(trial::MAX_SIZE, room.width)),
                    None => width(trial::MAX_SIZE, original.width),
                }?;
                (fitted.width, columns)
            };
            trial.fit_height(width, min.height, columns)
        }
        HeightAndWidthProportionally => {
            let ratio = trial.outer_ratio();
            let lower_width = if unbroken {
                min.width.max(trial.unbroken_width()?)
            } else {
                min.width
            };
            let lower = min
                .height
                .max(trial.proportional_height(lower_width, ratio));
            let size = |height| Size {
                width: trial.proportional_width(height, ratio),
                height,
            };
            let floor = trial.proportional_floor(ratio);
            let precision = trial::WIDTH_PRECISION / ratio.max(1.0);
            let height = trial::smallest(lower, floor, original.height, precision, |height| {
                trial.fits(size(height), columns)
            })?;
            Some(size(height))
        }
    }
}
