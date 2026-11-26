//! What the File Name text variable reads from the opened document (oracle textvars; the dates come from the package, see
//! `folio_idml`): the file's name with its extension and its folder relative to the project root, `/` between the names. Never an
//! absolute path (principle 1): a document that is in no project (an INDD or IDML opened read-only) has no folder, only its name.
use std::path::Path;

/// Sets `doc.facts.file_name` and `.folder` for the document file `file`.
pub fn attach(doc: &mut folio_model::Document, file: &Path, folio: Option<&crate::project::FolioSession>) {
    doc.facts.file_name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    doc.facts.folder = folio
        .and_then(|f| file.parent()?.strip_prefix(&f.opened.project.root).ok().map(Path::to_path_buf))
        .map(|rel| rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
        .unwrap_or_default();
}

/// Minutes east of UTC of this computer's time zone now (the Output Date is written with the wall clock of the computer that outputs).
#[cfg(windows)]
fn local_offset_minutes() -> i64 {
    #[repr(C)]
    struct SystemTime([u16; 8]);
    #[repr(C)]
    struct ZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    unsafe extern "system" {
        fn GetTimeZoneInformation(info: *mut ZoneInformation) -> u32;
    }
    // SAFETY: the structure is the Win32 TIME_ZONE_INFORMATION and lives for the call.
    let mut z: ZoneInformation = unsafe { std::mem::zeroed() };
    let id = unsafe { GetTimeZoneInformation(&mut z) };
    let bias = z.bias + if id == 2 { z.daylight_bias } else { z.standard_bias };
    -i64::from(bias)
}

#[cfg(unix)]
fn local_offset_minutes() -> i64 {
    #[repr(C)]
    struct Tm {
        fields: [i32; 9],
        gmtoff: i64,
        zone: *const u8,
    }
    unsafe extern "C" {
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    // SAFETY: `Tm` has the layout of C's `struct tm` on glibc and the BSDs/macOS (nine ints, then `tm_gmtoff`, `tm_zone`).
    let mut tm = Tm { fields: [0; 9], gmtoff: 0, zone: std::ptr::null() };
    let ok = unsafe { !localtime_r(&now, &mut tm).is_null() };
    if ok { tm.gmtoff / 60 } else { 0 }
}

#[cfg(not(any(windows, unix)))]
fn local_offset_minutes() -> i64 {
    0
}

/// Now, as the wall clock of this computer with its offset (ISO 8601).
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    folio_text::iso_from_unix(secs, local_offset_minutes())
}

/// Whether some text of the document holds an instance of an Output Date variable.
pub fn uses_output_date(doc: &folio_model::Document) -> bool {
    let dates: Vec<&str> = doc.text_variables.values().filter(|d| d.kind == "OutputDateType").map(|d| d.id.as_str()).collect();
    !dates.is_empty() && doc.stories.values().flat_map(|s| &s.paragraphs).flat_map(|p| &p.runs).any(|r| r.named.as_deref().is_some_and(|n| dates.contains(&n.id.as_str())))
}

/// The document and its layout as an output now shows them: the Output Date is this moment (oracle textvars: it changes with the
/// output, not with a save). The document itself is not touched, so a read-only document can be output too.
pub fn output_now(s: &crate::Session) -> (folio_model::Document, folio_model::layout::DocumentLayout) {
    let mut doc = s.doc.clone();
    doc.facts.output = now_iso();
    let mut layout = folio_text::layout_document(&doc, &s.fonts);
    layout.page_frames = folio_text::layout_master_page_numbers(&doc, &s.fonts, &layout)
        .into_iter()
        .map(|p| folio_model::layout::PageFrameText { page: p.page, frame: p.frame })
        .collect();
    (doc, layout)
}
