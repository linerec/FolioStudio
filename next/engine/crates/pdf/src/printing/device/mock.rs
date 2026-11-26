//! A device that reaches no printer: the stand-in for tests and screen checks (`FOLIO_PRINT_DEVICE=mock`). It keeps every request it took.
use super::*;
use std::sync::Mutex;

pub struct MockDevice {
    printers: Vec<PrinterInfo>,
    state: Mutex<State>,
}
#[derive(Default)]
struct State { sent: Vec<(String, DeviceRequest)>, canceled: Vec<String>, next: u64, fail_next: Option<PrintError> }

fn paper(name: &str, w: f64, h: f64) -> Paper { Paper { name: name.into(), size: Some([w, h]) } }

impl MockDevice {
    pub fn new() -> Self {
        let papers = || vec![paper("Letter", 612.0, 792.0), paper("A4", 595.276, 841.89), paper("A3", 841.89, 1190.551)];
        let printer = |name: &str, is_default: bool, postscript: bool| PrinterInfo { name: name.into(), is_default, postscript, virtual_pdf: is_virtual_pdf(name), papers: papers() };
        Self { printers: vec![printer("Mock Laser PS", true, true), printer("Mock Inkjet", false, false), printer("Mock PDF Printer", false, false)], state: Mutex::new(State::default()) }
    }
    /// Every request taken so far with the job id it got, oldest first.
    pub fn sent(&self) -> Vec<(String, DeviceRequest)> { self.state.lock().expect("PDF worker/cache lock is not poisoned").sent.clone() }
    pub fn canceled(&self) -> Vec<String> { self.state.lock().expect("PDF worker/cache lock is not poisoned").canceled.clone() }
    /// The next `submit` fails with this error (a printer that is off, a driver that refuses).
    pub fn fail_next(&self, error: PrintError) { self.state.lock().expect("PDF worker/cache lock is not poisoned").fail_next = Some(error); }
}
impl Default for MockDevice { fn default() -> Self { Self::new() } }

impl Device for MockDevice {
    fn kind(&self) -> &'static str { "mock" }
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> { Ok(self.printers.clone()) }
    fn submit(&self, request: &DeviceRequest) -> Result<JobTicket, PrintError> {
        let mut state = self.state.lock().expect("PDF worker/cache lock is not poisoned");
        if let Some(error) = state.fail_next.take() { return Err(error); }
        if let Some(out) = &request.output_file {
            std::fs::copy(&request.file, out).map_err(|e| PrintError::new(ErrorKind::DeviceFailed, format!("cannot write {}: {e}", out.display())))?;
        }
        state.next += 1;
        let id = format!("mock-{}", state.next);
        state.sent.push((id.clone(), request.clone()));
        Ok(JobTicket { id, printer: request.printer.clone(), state: "queued", warnings: Vec::new() })
    }
    fn cancel(&self, printer: &str, job: &str) -> Result<(), PrintError> {
        let mut state = self.state.lock().expect("PDF worker/cache lock is not poisoned");
        if !self.printers.iter().any(|p| p.name == printer) { return Err(PrintError::new(ErrorKind::PrinterNotFound, format!("printer not found: {printer}"))); }
        if !state.sent.iter().any(|(id, r)| id == job && r.printer == printer) || state.canceled.iter().any(|id| id == job) {
            return Err(PrintError::new(ErrorKind::JobNotFound, format!("no such job: {job}")));
        }
        state.canceled.push(job.to_owned());
        Ok(())
    }
}
