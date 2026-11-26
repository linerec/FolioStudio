use std::sync::{Arc, OnceLock};
use folio_color::{icc::Intent, profiles::ProfileIndex};
use folio_model::image_color::{ImageColor, DOCUMENT_PROFILE};
use pdf_writer::types::RenderingIntent;

pub(crate) fn pdf_intent(name: &str) -> RenderingIntent {
    match Intent::from_name(name) {
        Intent::Perceptual => RenderingIntent::Perceptual,
        Intent::Relative => RenderingIntent::RelativeColorimetric,
        Intent::Saturation => RenderingIntent::Saturation,
        Intent::Absolute => RenderingIntent::AbsoluteColorimetric,
    }
}

#[derive(Default)]
pub(crate) struct Resolved {
    profile: Option<Option<Arc<Vec<u8>>>>,
    /// The CMYK profile assigned to the image (`None`: the image's numbers are in the document CMYK profile).
    /// A CMYK profile embedded in the file is not used: InDesign's default colour settings preserve CMYK numbers.
    pub cmyk: Option<Arc<Vec<u8>>>,
    pub intent: Intent,
    pub warning: Option<String>,
}

pub(crate) fn resolve(value: Option<&ImageColor>, document: Option<Arc<Vec<u8>>>) -> Result<Resolved, String> {
    let Some(value) = value else { return Ok(Resolved::default()) };
    let intent = Intent::from_name(&value.intent);
    let mut cmyk = None;
    let profile = match value.profile.as_str() {
        "None" | "" => None,
        DOCUMENT_PROFILE => Some(document),
        name => {
            static INDEX: OnceLock<ProfileIndex> = OnceLock::new();
            let index = INDEX.get_or_init(ProfileIndex::scan_installed);
            let path = index.find(name).ok_or_else(|| format!("image profile is not installed: {name}"))?;
            let bytes = std::fs::read(path).map_err(|e| format!("image profile {name}: {e}"))?;
            if bytes.get(16..20) == Some(b"CMYK") {
                // a CMYK image converts from this profile to the output profile (RGB images are unaffected)
                cmyk = Some(Arc::new(bytes));
                None
            } else if bytes.get(16..20) != Some(b"RGB ") {
                None
            } else {
                Some(Some(Arc::new(bytes)))
            }
        }
    };
    Ok(Resolved { profile, cmyk, intent, warning: None })
}

pub(crate) fn for_source(source: &crate::graphics::ImageSource, value: Option<&ImageColor>, document: Option<Arc<Vec<u8>>>) -> Resolved {
    use crate::graphics::ImageSource;
    if matches!(source, ImageSource::Preview(_) | ImageSource::Proxy(_)) {
        return Resolved::default();
    }
    resolve(value, document).unwrap_or_else(|warning| Resolved {
        warning: Some(warning), ..Resolved::default()
    })
}

impl Resolved {
    pub fn apply(&self, space: &mut crate::images::ImgSpace) {
        if let (crate::images::ImgSpace::Rgb(icc), Some(profile)) = (space, &self.profile) {
            *icc = profile.clone();
        }
    }
}
