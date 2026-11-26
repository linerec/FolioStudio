//! The Windows spooler. The print PDF's sheets are drawn as bitmaps (at most 300 dpi) and sent through GDI, which every installed driver
//! (GDI, PCL, PostScript) accepts; the spooler does not take PDF. The system calls are declared here by hand like the other Windows calls of this
//! workspace, so no crate is added. Windows-only code: the rest of the engine never names it.
use super::*;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::Mutex;

type Handle = isize;

#[link(name = "winspool")]
unsafe extern "system" {
    fn EnumPrintersW(flags: u32, name: *const u16, level: u32, buf: *mut u8, size: u32, needed: *mut u32, returned: *mut u32) -> i32;
    fn GetDefaultPrinterW(buf: *mut u16, size: *mut u32) -> i32;
    fn OpenPrinterW(name: *const u16, handle: *mut Handle, defaults: *const c_void) -> i32;
    fn ClosePrinter(handle: Handle) -> i32;
    fn SetJobW(handle: Handle, job: u32, level: u32, buf: *mut u8, command: u32) -> i32;
    fn DocumentPropertiesW(hwnd: Handle, printer: Handle, name: *const u16, out: *mut u8, input: *const u8, mode: u32) -> i32;
    fn DeviceCapabilitiesW(device: *const u16, port: *const u16, cap: u16, out: *mut u8, devmode: *const u8) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateDCW(driver: *const u16, device: *const u16, port: *const u16, devmode: *const u8) -> Handle;
    fn DeleteDC(dc: Handle) -> i32;
    fn StartDocW(dc: Handle, info: *const DocInfo) -> i32;
    fn EndDoc(dc: Handle) -> i32;
    fn AbortDoc(dc: Handle) -> i32;
    fn StartPage(dc: Handle) -> i32;
    fn EndPage(dc: Handle) -> i32;
    fn GetDeviceCaps(dc: Handle, index: i32) -> i32;
    fn StretchDIBits(dc: Handle, xd: i32, yd: i32, wd: i32, hd: i32, xs: i32, ys: i32, ws: i32, hs: i32, bits: *const u8, info: *const BitmapHeader, usage: u32, rop: u32) -> i32;
}

#[repr(C)]
struct DocInfo { size: i32, name: *const u16, output: *const u16, datatype: *const u16, kind: u32 }
#[repr(C)]
struct BitmapHeader { size: u32, width: i32, height: i32, planes: u16, bit_count: u16, compression: u32, size_image: u32, x_ppm: i32, y_ppm: i32, used: u32, important: u32 }
#[repr(C)]
struct PrinterInfo2 {
    server: *const u16, printer: *const u16, share: *const u16, port: *const u16, driver: *const u16, comment: *const u16, location: *const u16,
    devmode: *const u8, sep_file: *const u16, processor: *const u16, datatype: *const u16, parameters: *const u16, security: *const u8,
    attributes: u32, priority: u32, default_priority: u32, start_time: u32, until_time: u32, status: u32, jobs: u32, average_ppm: u32,
}

const DC_PAPERS: u16 = 2;
const DC_PAPERSIZE: u16 = 3;
const DC_COPIES: u16 = 18;
const DC_PAPERNAMES: u16 = 16;
const DM_OUT_BUFFER: u32 = 2;
const DM_IN_BUFFER: u32 = 8;
const LOGPIXELSX: i32 = 88;
const LOGPIXELSY: i32 = 90;
const HORZRES: i32 = 8;
const VERTRES: i32 = 10;
const JOB_CONTROL_DELETE: u32 = 5;

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }
fn failed(message: impl Into<String>) -> PrintError { PrintError::new(ErrorKind::DeviceFailed, message) }

/// # Safety
/// `p` is null or points at a NUL-terminated UTF-16 string.
unsafe fn from_wide(p: *const u16) -> String {
    if p.is_null() { return String::new(); }
    let mut n = 0;
    while unsafe { *p.add(n) } != 0 { n += 1; }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
}

/// Eight-byte aligned bytes for the structures the spooler fills in.
struct Aligned { words: Vec<u64>, len: usize }
impl Aligned {
    fn new(len: usize) -> Self { Self { words: vec![0; len.div_ceil(8).max(1)], len } }
    fn ptr(&mut self) -> *mut u8 { self.words.as_mut_ptr().cast() }
    fn bytes(&mut self) -> &mut [u8] { let len = self.len; unsafe { std::slice::from_raw_parts_mut(self.ptr(), len) } }
}

fn default_printer() -> Option<String> {
    let mut len = 0u32;
    unsafe { GetDefaultPrinterW(std::ptr::null_mut(), &mut len) };
    if len == 0 { return None; }
    let mut buf = vec![0u16; len as usize];
    (unsafe { GetDefaultPrinterW(buf.as_mut_ptr(), &mut len) } != 0).then(|| String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(buf.len())]))
}

/// A driver whose name says PostScript ("... PS", "PS Class Driver", "PostScript").
fn speaks_postscript(driver: &str) -> bool {
    let lower = driver.to_ascii_lowercase();
    lower.contains("postscript") || lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "ps")
}

/// The paper sizes the driver offers: Windows paper id, name, size in points.
fn papers(name: &[u16]) -> Vec<(u16, String, Option<[f64; 2]>)> {
    let count = unsafe { DeviceCapabilitiesW(name.as_ptr(), std::ptr::null(), DC_PAPERNAMES, std::ptr::null_mut(), std::ptr::null()) };
    if count <= 0 { return Vec::new(); }
    let n = count as usize;
    let mut names = vec![0u16; n * 64];
    let mut ids = vec![0u16; n];
    let mut sizes = vec![0i32; n * 2];
    let got_names = unsafe { DeviceCapabilitiesW(name.as_ptr(), std::ptr::null(), DC_PAPERNAMES, names.as_mut_ptr().cast(), std::ptr::null()) };
    let got_ids = unsafe { DeviceCapabilitiesW(name.as_ptr(), std::ptr::null(), DC_PAPERS, ids.as_mut_ptr().cast(), std::ptr::null()) };
    let got_sizes = unsafe { DeviceCapabilitiesW(name.as_ptr(), std::ptr::null(), DC_PAPERSIZE, sizes.as_mut_ptr().cast(), std::ptr::null()) };
    let usable = [got_names, got_ids, got_sizes].into_iter().min().unwrap_or(0).max(0) as usize;
    (0..usable.min(n)).map(|i| {
        let raw = &names[i * 64..(i + 1) * 64];
        let paper = String::from_utf16_lossy(&raw[..raw.iter().position(|c| *c == 0).unwrap_or(64)]);
        // tenths of a millimetre -> points
        let size = (sizes[i * 2] > 0 && sizes[i * 2 + 1] > 0).then(|| [f64::from(sizes[i * 2]) / 10.0 / 25.4 * 72.0, f64::from(sizes[i * 2 + 1]) / 10.0 / 25.4 * 72.0]);
        (ids[i], paper, size)
    }).collect()
}

type PaperList = Vec<(u16, String, Option<[f64; 2]>)>;

/// What the drivers answered about their papers, by printer name: the answer and when it came (`None` while the first question is still out).
type PaperEntry = (Option<PaperList>, std::time::Instant, bool);
struct PaperMemory(Mutex<BTreeMap<String, PaperEntry>>);
static PAPERS: PaperMemory = PaperMemory(Mutex::new(BTreeMap::new()));
impl PaperMemory {
    /// Start asking the driver on its own thread unless an answer younger than a minute is there or a question is already out. True when this printer
    /// has never been asked before (only then is an answer worth waiting for).
    fn ask(&'static self, name: &str) -> bool {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let first = !map.contains_key(name);
        let slot = map.entry(name.to_string()).or_insert((None, std::time::Instant::now(), false));
        if slot.2 || (slot.0.is_some() && slot.1.elapsed() < std::time::Duration::from_secs(60)) { return first; }
        slot.2 = true;
        let name = name.to_string();
        std::thread::spawn(move || {
            let answer = papers(&wide(&name));
            self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(name, (Some(answer), std::time::Instant::now(), false));
        });
        first
    }
    /// The last answer, even an old one; `None` when the driver has not answered yet.
    fn get(&self, name: &str) -> Option<PaperList> { self.0.lock().unwrap_or_else(|e| e.into_inner()).get(name).and_then(|slot| slot.0.clone()) }
}

struct Printer(Handle);
impl Printer {
    fn open(name: &[u16]) -> Result<Self, PrintError> {
        let mut h: Handle = 0;
        if unsafe { OpenPrinterW(name.as_ptr(), &mut h, std::ptr::null()) } == 0 || h == 0 { return Err(PrintError::new(ErrorKind::PrinterNotFound, "cannot open the printer")); }
        Ok(Self(h))
    }
}
impl Drop for Printer { fn drop(&mut self) { unsafe { ClosePrinter(self.0) }; } }

fn set_i16(buf: &mut [u8], at: usize, v: i16) { buf[at..at + 2].copy_from_slice(&v.to_le_bytes()); }
fn set_u32(buf: &mut [u8], at: usize, v: u32) { buf[at..at + 4].copy_from_slice(&v.to_le_bytes()); }

/// The driver's DEVMODE with the request's copies, collation, orientation, paper and colour set. `copies` is what the driver is asked for (1 when the app repeats the pages itself).
fn devmode(printer: &Printer, name: &[u16], r: &DeviceRequest, copies: u32, paper_id: Option<u16>) -> Result<Aligned, PrintError> {
    let size = unsafe { DocumentPropertiesW(0, printer.0, name.as_ptr(), std::ptr::null_mut(), std::ptr::null(), 0) };
    if size <= 0 { return Err(failed("the driver gave no print settings")); }
    let mut dm = Aligned::new(size as usize);
    if unsafe { DocumentPropertiesW(0, printer.0, name.as_ptr(), dm.ptr(), std::ptr::null(), DM_OUT_BUFFER) } != 1 { return Err(failed("the driver gave no print settings")); }
    let (mut fields, bytes) = (0u32, dm.bytes());
    // DEVMODEW: dmFields @72, dmOrientation @76, dmPaperSize @78, dmCopies @86, dmColor @92, dmCollate @100
    set_i16(bytes, 76, if matches!(r.orientation, Orientation::Portrait | Orientation::ReversePortrait) { 1 } else { 2 }); fields |= 0x1;
    if let Some(id) = paper_id { set_i16(bytes, 78, id as i16); fields |= 0x2; }
    set_i16(bytes, 86, copies.min(i16::MAX as u32) as i16); fields |= 0x100;
    set_i16(bytes, 92, if r.color == ColorMode::Gray { 1 } else { 2 }); fields |= 0x800;
    set_i16(bytes, 100, i16::from(r.collate && copies > 1)); fields |= 0x8000;
    let old = u32::from_le_bytes(bytes[72..76].try_into().unwrap_or([0; 4]));
    set_u32(bytes, 72, old | fields);
    // the driver checks and completes what was set
    let input = dm.bytes().to_vec();
    if unsafe { DocumentPropertiesW(0, printer.0, name.as_ptr(), dm.ptr(), input.as_ptr(), DM_IN_BUFFER | DM_OUT_BUFFER) } != 1 { return Err(failed("the driver refused the print settings")); }
    Ok(dm)
}

pub struct Spooler;

impl Device for Spooler {
    fn kind(&self) -> &'static str { "windows" }

    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> {
        let (mut needed, mut returned) = (0u32, 0u32);
        // PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS, level 2
        unsafe { EnumPrintersW(6, std::ptr::null(), 2, std::ptr::null_mut(), 0, &mut needed, &mut returned) };
        if needed == 0 { return Ok(Vec::new()); }
        let mut buf = Aligned::new(needed as usize);
        if unsafe { EnumPrintersW(6, std::ptr::null(), 2, buf.ptr(), needed, &mut needed, &mut returned) } == 0 { return Err(failed("the printer list could not be read")); }
        let default = default_printer();
        let rows = buf.ptr().cast::<PrinterInfo2>();
        let found: Vec<(String, String)> = (0..returned as usize).map(|i| {
            let row = unsafe { &*rows.add(i) };
            (unsafe { from_wide(row.printer) }, unsafe { from_wide(row.driver) })
        }).collect();
        // The paper lists are remembered for a minute: an offline network printer can keep the driver waiting for half a minute, and the print
        // dialog asks for this list every time it opens. The first call waits at most a second for the answers (all printers are asked at once; a
        // printer that has not answered is listed without papers and shows them on the next call); later calls answer from the memory.
        let fresh: Vec<&String> = found.iter().map(|(name, _)| name).filter(|name| PAPERS.ask(name)).collect();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < deadline && fresh.iter().any(|name| PAPERS.get(name).is_none()) { std::thread::sleep(std::time::Duration::from_millis(10)); }
        Ok(found.into_iter().map(|(name, driver)| PrinterInfo {
            is_default: default.as_deref() == Some(name.as_str()), postscript: speaks_postscript(&driver), virtual_pdf: is_virtual_pdf(&name),
            papers: PAPERS.get(&name).unwrap_or_default().into_iter().map(|(_, name, size)| Paper { name, size }).collect(), name,
        }).collect())
    }

    fn submit(&self, r: &DeviceRequest) -> Result<JobTicket, PrintError> {
        let name = wide(&r.printer);
        let document = folio_links::pdfraster::RasterDocument::new(std::fs::read(&r.file).map_err(|e| failed(format!("cannot read the print PDF: {e}")))?).map_err(failed)?;
        if document.pages() != r.sheets.len() { return Err(failed("the print PDF does not hold the sheets of the request")); }
        let printer = Printer::open(&name)?;
        let supported = unsafe { DeviceCapabilitiesW(name.as_ptr(), std::ptr::null(), DC_COPIES, std::ptr::null_mut(), std::ptr::null()) }.max(1) as u32;
        // the driver makes the copies when it can; otherwise the pages are sent again here (collated: whole runs, else page by page)
        let (driver_copies, repeat) = if supported >= r.copies { (r.copies, 1) } else { (1, r.copies) };
        let paper_id = r.paper.as_deref().and_then(|want| papers(&name).into_iter().find(|(_, n, _)| n.eq_ignore_ascii_case(want)).map(|(id, _, _)| id));
        let mut mode = devmode(&printer, &name, r, driver_copies, paper_id)?;
        let dc = unsafe { CreateDCW(wide("WINSPOOL").as_ptr(), name.as_ptr(), std::ptr::null(), mode.ptr()) };
        if dc == 0 { return Err(failed("the printer could not be reached")); }
        let result = (|| {
            let output = r.output_file.as_ref().map(|p| wide(&p.display().to_string()));
            let title = wide(&r.job_name);
            let info = DocInfo { size: std::mem::size_of::<DocInfo>() as i32, name: title.as_ptr(), output: output.as_ref().map_or(std::ptr::null(), |o| o.as_ptr()), datatype: std::ptr::null(), kind: 0 };
            let job = unsafe { StartDocW(dc, &info) };
            if job <= 0 { return Err(failed("the printer did not start the job")); }
            let dpi = [f64::from(unsafe { GetDeviceCaps(dc, LOGPIXELSX) }), f64::from(unsafe { GetDeviceCaps(dc, LOGPIXELSY) })];
            let area = [f64::from(unsafe { GetDeviceCaps(dc, HORZRES) }), f64::from(unsafe { GetDeviceCaps(dc, VERTRES) })];
            let mut order: Vec<usize> = (0..r.sheets.len()).collect();
            if r.reverse_order { order.reverse(); }
            let runs: Vec<usize> = if repeat > 1 && r.collate { (0..repeat as usize).flat_map(|_| order.iter().copied()).collect() }
                else if repeat > 1 { order.iter().flat_map(|i| std::iter::repeat_n(*i, repeat as usize)).collect() } else { order };
            for index in runs {
                // the size the PDF really has (marks and bleed make a sheet larger than its page)
                let sheet = document.page_size(index).unwrap_or(r.sheets[index].size);
                let (origin, size, per_point) = placement(sheet, r.scale, r.position, dpi, area);
                let raster = document.render(index, per_point as f32).map_err(|e| { unsafe { AbortDoc(dc) }; failed(e) })?;
                let mut bgra = Vec::with_capacity(raster.rgba.len());
                for px in raster.rgba.chunks_exact(4) { bgra.extend_from_slice(&[px[2], px[1], px[0], 255]); }
                if matches!(r.orientation, Orientation::ReversePortrait | Orientation::ReverseLandscape) {
                    let mut turned: Vec<u8> = bgra.chunks_exact(4).rev().flatten().copied().collect();
                    std::mem::swap(&mut bgra, &mut turned);
                }
                let header = BitmapHeader { size: std::mem::size_of::<BitmapHeader>() as u32, width: raster.width as i32, height: -(raster.height as i32), planes: 1, bit_count: 32, compression: 0, size_image: 0, x_ppm: 0, y_ppm: 0, used: 0, important: 0 };
                if unsafe { StartPage(dc) } <= 0 { unsafe { AbortDoc(dc) }; return Err(failed("the printer did not start a page")); }
                let copied = unsafe { StretchDIBits(dc, origin[0].round() as i32, origin[1].round() as i32, size[0].round() as i32, size[1].round() as i32, 0, 0, raster.width as i32, raster.height as i32, bgra.as_ptr(), &header, 0, 0x00CC_0020) };
                if copied <= 0 { unsafe { AbortDoc(dc) }; return Err(failed("the page could not be drawn for the printer")); }
                if unsafe { EndPage(dc) } <= 0 { unsafe { AbortDoc(dc) }; return Err(failed("the printer did not finish a page")); }
            }
            if unsafe { EndDoc(dc) } <= 0 { return Err(failed("the printer did not finish the job")); }
            Ok(job)
        })();
        unsafe { DeleteDC(dc) };
        result.map(|job| JobTicket { id: job.to_string(), printer: r.printer.clone(), state: "queued", warnings: Vec::new() })
    }

    fn cancel(&self, printer: &str, job: &str) -> Result<(), PrintError> {
        let id: u32 = job.parse().map_err(|_| PrintError::new(ErrorKind::JobNotFound, format!("no such job: {job}")))?;
        let handle = Printer::open(&wide(printer))?;
        if unsafe { SetJobW(handle.0, id, 0, std::ptr::null_mut(), JOB_CONTROL_DELETE) } == 0 { return Err(PrintError::new(ErrorKind::JobNotFound, format!("no such job: {job}"))); }
        Ok(())
    }
}
