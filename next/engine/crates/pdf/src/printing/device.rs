//! Sending a print job to a printer. The job is the print PDF Folio already writes for "save as PDF" (`printingFile`, `bookletFile`): the sheets
//! are decided there (page range, spreads, blank pages, marks, booklet). This module only hands those sheets to a device and says what happened.
//!
//! One `Device` per platform, chosen at compile time (`system()`): CUPS on macOS and Linux (`lp` takes the PDF as it is), the Windows spooler
//! (GDI: the sheets are drawn as bitmaps, which every installed driver accepts), and "unsupported" on iOS and Android, which cannot start a
//! helper process. `MockDevice` is the stand-in for tests and screen checks. A request is ephemeral: nothing here is saved in a document or a project.
//! Errors are structured (`ErrorKind` plus text) so a caller that is not a screen can act on them.
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub mod cups;
mod mock;
#[cfg(windows)]
mod windows;
pub use mock::MockDevice;

use super::settings;

/// A paper size a printer offers: the driver's name and its size in points when it is known.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paper { pub name: String, pub size: Option<[f64; 2]> }

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
    /// The driver speaks PostScript (a PostScript printer, or a PDF-capable queue that says so).
    pub postscript: bool,
    /// A virtual PDF printer (its name has "PDF"): the only kind a request may give an output file for.
    pub virtual_pdf: bool,
    pub papers: Vec<Paper>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation { Portrait, Landscape, ReversePortrait, ReverseLandscape }
impl Orientation {
    pub fn parse(id: &str) -> Option<Self> {
        Some(match id { "portrait" => Self::Portrait, "landscape" => Self::Landscape, "reversePortrait" => Self::ReversePortrait, "reverseLandscape" => Self::ReverseLandscape, _ => return None })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Scale { Fit, Percent { width: f64, height: f64 } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorMode { Color, Gray }

/// Where the sheet sits on the paper when it is smaller than the printable area (InDesign's "Page Position").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Position { UpperLeft, CenterHorizontally, CenterVertically, Centered }

/// One sheet of the print PDF: the document pages on it (empty for a booklet face's blank), its label and its size in points.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetRef { pub page_ids: Vec<String>, pub label: String, pub size: [f64; 2] }

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRequest {
    pub printer: String,
    pub job_name: String,
    pub copies: u32,
    pub collate: bool,
    pub reverse_order: bool,
    pub orientation: Orientation,
    pub paper: Option<String>,
    pub scale: Scale,
    pub position: Position,
    pub color: ColorMode,
    /// The sheets the print PDF holds, in order: what the printer is asked to print.
    pub sheets: Vec<SheetRef>,
    /// The print PDF (a temporary file the caller removes afterwards).
    #[serde(skip)]
    pub file: PathBuf,
    /// Only for a virtual PDF printer: the file it writes, so no save box opens.
    pub output_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind { PrinterNotFound, InvalidRequest, InvalidRange, Unsupported, JobNotFound, DeviceFailed, ExportFailed }

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintError { pub kind: ErrorKind, pub message: String }
impl PrintError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self { Self { kind, message: message.into() } }
}
impl std::fmt::Display for PrintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.message) }
}

/// What a device answers to a job it took: the id to cancel it by, and where it is.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTicket { pub id: String, pub printer: String, pub state: &'static str, pub warnings: Vec<String> }

pub trait Device: Send + Sync {
    /// `windows`, `cups`, `mock` or `unsupported`.
    fn kind(&self) -> &'static str;
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError>;
    /// Hands the sheets over. The caller has checked the request (`send`); the device only reports what the system said.
    fn submit(&self, request: &DeviceRequest) -> Result<JobTicket, PrintError>;
    fn cancel(&self, printer: &str, job: &str) -> Result<(), PrintError>;
}

pub const MAX_COPIES: u32 = 9999;
const MAX_DPI: f64 = 300.0;

/// Where and how large a sheet is drawn on the printable area, in device pixels, and the resolution to draw it at.
pub fn placement(sheet: [f64; 2], scale: Scale, position: Position, dpi: [f64; 2], area: [f64; 2]) -> ([f64; 2], [f64; 2], f64) {
    let natural = [sheet[0] * dpi[0] / 72.0, sheet[1] * dpi[1] / 72.0];
    let factor = match scale {
        Scale::Fit => (area[0] / natural[0]).min(area[1] / natural[1]),
        Scale::Percent { width, height } => width.max(height) / 100.0,
    };
    let size = match scale {
        Scale::Fit => [natural[0] * factor, natural[1] * factor],
        Scale::Percent { width, height } => [natural[0] * width / 100.0, natural[1] * height / 100.0],
    };
    let origin = match position {
        Position::UpperLeft => [0.0, 0.0],
        Position::CenterHorizontally => [(area[0] - size[0]) / 2.0, 0.0],
        Position::CenterVertically => [0.0, (area[1] - size[1]) / 2.0],
        Position::Centered => [(area[0] - size[0]) / 2.0, (area[1] - size[1]) / 2.0],
    };
    (origin, size, dpi[0].min(dpi[1]).min(MAX_DPI) / 72.0 * factor.max(0.01))
}


/// A name that marks a virtual PDF printer ("Microsoft Print to PDF", "CUPS-PDF", "Mock PDF Printer").
pub fn is_virtual_pdf(name: &str) -> bool { name.to_ascii_uppercase().contains("PDF") }

/// Checks a request against the device's printers and makes it what the device will be given: the same request, but a paper the printer does not
/// offer becomes the driver's own paper (with a warning, not an error). Nothing reaches the device here, so no PDF needs to exist yet.
pub fn prepare(device: &dyn Device, request: &DeviceRequest) -> Result<(DeviceRequest, Vec<String>), PrintError> {
    if request.sheets.is_empty() { return Err(PrintError::new(ErrorKind::InvalidRange, "no pages selected for printing")); }
    if request.copies == 0 || request.copies > MAX_COPIES { return Err(PrintError::new(ErrorKind::InvalidRequest, format!("copies must be 1 to {MAX_COPIES}"))); }
    if let Scale::Percent { width, height } = request.scale
        && ![width, height].iter().all(|v| v.is_finite() && (1.0..=1000.0).contains(v)) { return Err(PrintError::new(ErrorKind::InvalidRequest, "scale must be 1 to 1000 percent")); }
    let printers = device.printers()?;
    let printer = printers.iter().find(|p| p.name == request.printer)
        .ok_or_else(|| PrintError::new(ErrorKind::PrinterNotFound, format!("printer not found: {}", request.printer)))?;
    if let Some(path) = &request.output_file {
        if !printer.virtual_pdf { return Err(PrintError::new(ErrorKind::InvalidRequest, "an output file can only be given to a virtual PDF printer")); }
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) { return Err(PrintError::new(ErrorKind::InvalidRequest, "the output file of a PDF printer must be a .pdf file")); }
    }
    let mut request = request.clone();
    let mut warnings = Vec::new();
    if let Some(want) = request.paper.clone()
        && !printer.papers.is_empty() && !printer.papers.iter().any(|p| p.name.eq_ignore_ascii_case(&want)) {
        warnings.push(format!("the printer does not offer the paper {want}; its default paper is used"));
        request.paper = None;
    }
    Ok((request, warnings))
}

/// `prepare`, then submit. The ticket carries the warnings.
pub fn send(device: &dyn Device, request: &DeviceRequest) -> Result<JobTicket, PrintError> {
    let (request, warnings) = prepare(device, request)?;
    let mut ticket = device.submit(&request)?;
    ticket.warnings.extend(warnings);
    Ok(ticket)
}

/// The settings (a print preset's values, InDesign's property names) a device print uses; every other setting is reported back as ignored.
const USED: &[&str] = &["printer", "copies", "collating", "reverseOrder", "printSpreads", "printBlankPages", "paperSize", "printPageOrientation", "scaleMode", "scaleWidth",
    "scaleHeight", "scaleProportional", "pagePosition", "cropMarks", "bleedMarks", "registrationMarks", "pageInformationMarks", "allPrinterMarks", "markOffset", "markLineWeight", "useDocumentBleedToPrint", "bleedTop", "bleedBottom", "bleedInside", "bleedOutside", "colorOutput"];

/// The settings that differ from their defaults and that a print to a device does not use (and that apply in the current state): shown to the user, never dropped silently.
pub fn ignored_settings(values: &Map<String, Value>) -> Vec<String> {
    let defaults = settings::defaults();
    let na = settings::not_applicable(values);
    settings::FIELDS.iter().map(|f| f.key)
        .filter(|k| !USED.contains(k) && !na.contains(*k) && *k != "ppd" && values.get(*k) != defaults.get(*k))
        .map(str::to_owned).collect()
}

fn text<'a>(v: &'a Map<String, Value>, k: &str) -> &'a str { v.get(k).and_then(Value::as_str).unwrap_or("") }
fn flag(v: &Map<String, Value>, k: &str) -> bool { v.get(k).and_then(Value::as_bool).unwrap_or(false) }
fn number(v: &Map<String, Value>, k: &str) -> f64 { v.get(k).and_then(Value::as_f64).unwrap_or(0.0) }

/// A request from a print preset's values (complete and consistent, as `settings::normalize` makes them). The printer is the caller's choice
/// (a preset never names one); `sheets` and `file` come from the print PDF.
pub fn request_from_settings(values: &Map<String, Value>, printer: &str, job_name: &str, sheets: Vec<SheetRef>, file: PathBuf, output_file: Option<PathBuf>) -> Result<DeviceRequest, PrintError> {
    let values = settings::normalize(values).map_err(|e| PrintError::new(ErrorKind::InvalidRequest, e))?;
    if text(&values, "printer") == "postscriptFile" { return Err(PrintError::new(ErrorKind::Unsupported, "writing a PostScript file is not supported; choose a printer")); }
    let color = match text(&values, "colorOutput") {
        "compositeGray" => ColorMode::Gray,
        "separations" | "inRipSeparations" => return Err(PrintError::new(ErrorKind::Unsupported, "separations cannot be sent to a printer; save a PDF instead")),
        _ => ColorMode::Color,
    };
    let orientation = Orientation::parse(text(&values, "printPageOrientation")).ok_or_else(|| PrintError::new(ErrorKind::InvalidRequest, "unknown page orientation"))?;
    let scale = if text(&values, "scaleMode") == "scaleToFit" { Scale::Fit } else {
        let width = number(&values, "scaleWidth");
        Scale::Percent { width, height: if flag(&values, "scaleProportional") { width } else { number(&values, "scaleHeight") } }
    };
    let position = match text(&values, "pagePosition") {
        "centerHorizontally" => Position::CenterHorizontally, "centerVertically" => Position::CenterVertically, "centered" => Position::Centered, _ => Position::UpperLeft,
    };
    let copies = values.get("copies").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).unwrap_or(1);
    Ok(DeviceRequest {
        printer: printer.to_owned(), job_name: job_name.to_owned(), copies, collate: copies > 1 && flag(&values, "collating"), reverse_order: flag(&values, "reverseOrder"),
        orientation, paper: Some(text(&values, "paperSize").to_owned()).filter(|s| !s.is_empty()), scale, position, color, sheets, file, output_file,
    })
}

/// The device of this platform.
pub fn system() -> Arc<dyn Device> {
    #[cfg(windows)]
    { Arc::new(windows::Spooler) }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    { Arc::new(cups::Cups) }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    { Arc::new(Unsupported) }
}

/// iOS, Android and the rest: no printer reach from a library process (they cannot start `lp`, and have no spooler API here).
#[allow(dead_code)]
pub struct Unsupported;
impl Device for Unsupported {
    fn kind(&self) -> &'static str { "unsupported" }
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> { Err(PrintError::new(ErrorKind::Unsupported, "printing to a device is not supported on this platform")) }
    fn submit(&self, _: &DeviceRequest) -> Result<JobTicket, PrintError> { Err(PrintError::new(ErrorKind::Unsupported, "printing to a device is not supported on this platform")) }
    fn cancel(&self, _: &str, _: &str) -> Result<(), PrintError> { Err(PrintError::new(ErrorKind::Unsupported, "printing to a device is not supported on this platform")) }
}

/// The device a request goes to: the shared mock when `FOLIO_PRINT_DEVICE=mock` (screen checks and tests; no printer is reached), else the system's.
pub fn backend() -> Arc<dyn Device> {
    static MOCK: OnceLock<Arc<MockDevice>> = OnceLock::new();
    if std::env::var("FOLIO_PRINT_DEVICE").is_ok_and(|v| v == "mock") { return MOCK.get_or_init(|| Arc::new(MockDevice::new())).clone(); }
    system()
}

#[cfg(test)]
mod tests;
