use crate::styles::{Cat, ObjectProps};
use folio_model::fitting::FrameFitting;

pub const ALIGNMENTS: [&str; 9] = ["TopLeftAnchor", "TopCenterAnchor", "TopRightAnchor", "LeftCenterAnchor", "CenterAnchor", "RightCenterAnchor", "BottomLeftAnchor", "BottomCenterAnchor", "BottomRightAnchor"];
pub const MODES: [&str; 4] = ["None", "Proportionally", "FillProportionally", "ContentToFrame"];
pub const FIELDS: [(&str, &str); 7] = [("autoFit", "AutoFit"), ("topCrop", "TopCrop"), ("bottomCrop", "BottomCrop"), ("leftCrop", "LeftCrop"), ("rightCrop", "RightCrop"), ("fittingAlignment", "FittingAlignment"), ("fittingOnEmptyFrame", "FittingOnEmptyFrame")];

pub(crate) fn read(o: &ObjectProps<'_, '_>) -> FrameFitting {
    let p = &["FrameFittingOption"];
    FrameFitting {
        auto_fit: o.bool(p, "AutoFit", Cat::Fitting).unwrap_or(false),
        top_crop: o.f64(p, "TopCrop", Cat::Fitting).unwrap_or(0.0),
        bottom_crop: o.f64(p, "BottomCrop", Cat::Fitting).unwrap_or(0.0),
        left_crop: o.f64(p, "LeftCrop", Cat::Fitting).unwrap_or(0.0),
        right_crop: o.f64(p, "RightCrop", Cat::Fitting).unwrap_or(0.0),
        fitting_alignment: o.str(p, "FittingAlignment", Cat::Fitting).unwrap_or("CenterAnchor").into(),
        fitting_on_empty_frame: o.str(p, "FittingOnEmptyFrame", Cat::Fitting).unwrap_or("None").into(),
    }
}
