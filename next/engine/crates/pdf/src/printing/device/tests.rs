use super::*;
use serde_json::json;

fn sheets(n: usize) -> Vec<SheetRef> { (0..n).map(|i| SheetRef { page_ids: vec![format!("p{i}")], label: format!("A-{}", 10 + i), size: [240.0, 300.0] }).collect() }
fn values(patch: Value) -> Map<String, Value> {
    let mut m = settings::defaults();
    for (k, v) in patch.as_object().unwrap() { m.insert(k.clone(), v.clone()); }
    m
}
fn request(printer: &str, patch: Value) -> DeviceRequest {
    request_from_settings(&values(patch), printer, "job", sheets(2), PathBuf::from("print.pdf"), None).unwrap()
}
fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folio-device-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn settings_map_onto_the_request() {
    let r = request("Mock Laser PS", json!({"copies": 3, "collating": true, "reverseOrder": true, "printPageOrientation": "landscape", "paperSize": "A4",
        "scaleMode": "scaleWidthHeight", "scaleWidth": 50.0, "scaleProportional": true, "pagePosition": "centered", "colorOutput": "compositeGray"}));
    assert_eq!((r.copies, r.collate, r.reverse_order), (3, true, true));
    assert_eq!((r.orientation, r.paper.as_deref(), r.color, r.position), (Orientation::Landscape, Some("A4"), ColorMode::Gray, Position::Centered));
    assert_eq!(r.scale, Scale::Percent { width: 50.0, height: 50.0 });
    // not proportional: the height is its own
    assert_eq!(request("p", json!({"scaleProportional": false, "scaleWidth": 50.0, "scaleHeight": 80.0})).scale, Scale::Percent { width: 50.0, height: 80.0 });
    assert_eq!(request("p", json!({"scaleMode": "scaleToFit"})).scale, Scale::Fit);
    // collation means nothing for one copy
    assert!(!request("p", json!({"collating": true})).collate);
}

#[test]
fn a_request_reaches_the_mock_exactly_and_gets_a_job_id() {
    let device = MockDevice::new();
    let sent = request("Mock Inkjet", json!({"copies": 2, "printPageOrientation": "landscape"}));
    let ticket = send(&device, &sent).unwrap();
    assert_eq!((ticket.id.as_str(), ticket.printer.as_str(), ticket.state), ("mock-1", "Mock Inkjet", "queued"));
    let taken = device.sent();
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].0, "mock-1");
    let got = &taken[0].1;
    assert_eq!((got.copies, got.orientation), (2, Orientation::Landscape));
    assert_eq!(got.sheets, sheets(2)); // the pages, in print order
    assert_eq!(send(&device, &sent).unwrap().id, "mock-2");
}

#[test]
fn bad_requests_are_structured_errors_and_reach_no_device() {
    let device = MockDevice::new();
    let kind = |r: &DeviceRequest| send(&device, r).unwrap_err().kind;
    assert_eq!(kind(&request("No Such Printer", json!({}))), ErrorKind::PrinterNotFound);
    let mut none = request("Mock Inkjet", json!({}));
    none.sheets.clear();
    assert_eq!(kind(&none), ErrorKind::InvalidRange);
    let mut zero = request("Mock Inkjet", json!({}));
    zero.copies = 0;
    assert_eq!(kind(&zero), ErrorKind::InvalidRequest);
    zero.copies = MAX_COPIES + 1;
    assert_eq!(kind(&zero), ErrorKind::InvalidRequest);
    let mut scaled = request("Mock Inkjet", json!({}));
    scaled.scale = Scale::Percent { width: 0.0, height: 100.0 };
    assert_eq!(kind(&scaled), ErrorKind::InvalidRequest);
    // an output file is for a virtual PDF printer only, and is a .pdf
    let mut file = request("Mock Inkjet", json!({}));
    file.output_file = Some(PathBuf::from("out.pdf"));
    assert_eq!(kind(&file), ErrorKind::InvalidRequest);
    let mut wrong = request("Mock PDF Printer", json!({}));
    wrong.output_file = Some(PathBuf::from("out.txt"));
    assert_eq!(kind(&wrong), ErrorKind::InvalidRequest);
    assert!(device.sent().is_empty());
    // the settings a printer cannot take
    let err = |patch: Value| request_from_settings(&values(patch), "p", "j", sheets(1), PathBuf::new(), None).unwrap_err().kind;
    assert_eq!(err(json!({"colorOutput": "separations"})), ErrorKind::Unsupported);
    assert_eq!(err(json!({"printer": "postscriptFile"})), ErrorKind::Unsupported);
    assert_eq!(err(json!({"nonsense": 1})), ErrorKind::InvalidRequest);
}

#[test]
fn a_paper_the_printer_lacks_falls_back_with_a_warning() {
    let device = MockDevice::new();
    let ticket = send(&device, &request("Mock Inkjet", json!({"paperSize": "Tabloid"}))).unwrap();
    assert_eq!(device.sent()[0].1.paper, None);
    assert_eq!(ticket.warnings.len(), 1);
    let offered = send(&device, &request("Mock Inkjet", json!({"paperSize": "A4"}))).unwrap();
    assert!(offered.warnings.is_empty());
    assert_eq!(device.sent()[1].1.paper.as_deref(), Some("A4"));
}

#[test]
fn a_job_can_be_canceled_once() {
    let device = MockDevice::new();
    let id = send(&device, &request("Mock Laser PS", json!({}))).unwrap().id;
    assert_eq!(device.cancel("Mock Inkjet", &id).unwrap_err().kind, ErrorKind::JobNotFound); // another printer's
    assert_eq!(device.cancel("No Such Printer", &id).unwrap_err().kind, ErrorKind::PrinterNotFound);
    device.cancel("Mock Laser PS", &id).unwrap();
    assert_eq!(device.canceled(), vec![id.clone()]);
    assert_eq!(device.cancel("Mock Laser PS", &id).unwrap_err().kind, ErrorKind::JobNotFound);
    assert_eq!(device.cancel("Mock Laser PS", "mock-99").unwrap_err().kind, ErrorKind::JobNotFound);
}

#[test]
fn a_device_failure_is_reported_as_it_is() {
    let device = MockDevice::new();
    device.fail_next(PrintError::new(ErrorKind::DeviceFailed, "out of paper"));
    let err = send(&device, &request("Mock Laser PS", json!({}))).unwrap_err();
    assert_eq!((err.kind, err.message.as_str()), (ErrorKind::DeviceFailed, "out of paper"));
    assert!(send(&device, &request("Mock Laser PS", json!({}))).is_ok());
}

#[test]
fn a_virtual_pdf_printer_writes_the_file_it_is_given() {
    let dir = temp("virtual");
    std::fs::write(dir.join("print.pdf"), b"%PDF-1.7 test").unwrap();
    let mut r = request("Mock PDF Printer", json!({}));
    r.file = dir.join("print.pdf");
    r.output_file = Some(dir.join("out.pdf"));
    send(&MockDevice::new(), &r).unwrap();
    assert_eq!(std::fs::read(dir.join("out.pdf")).unwrap(), b"%PDF-1.7 test");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn settings_the_device_print_does_not_use_are_listed() {
    assert!(ignored_settings(&settings::defaults()).is_empty());
    let changed = values(json!({"printGuidesGrids": true, "printLayers": "allLayers", "copies": 4, "cropMarks": true}));
    let ignored = ignored_settings(&settings::normalize(&changed).unwrap());
    assert!(ignored.contains(&"printGuidesGrids".to_owned()) && ignored.contains(&"printLayers".to_owned()), "{ignored:?}");
    assert!(!ignored.contains(&"copies".to_owned()) && !ignored.contains(&"cropMarks".to_owned()));
}

#[test]
fn cups_command_line_carries_copies_orientation_and_scale() {
    let mut r = request("Office Laser", json!({"copies": 2, "collating": true, "printPageOrientation": "landscape", "paperSize": "A4", "scaleMode": "scaleToFit"}));
    r.file = PathBuf::from("/tmp/print.pdf");
    let args = cups::lp_args(&r).unwrap();
    assert_eq!(args.join(" "), "-d Office Laser -n 2 -t job -o collate=true -o orientation-requested=4 -o media=A4 -o print-scaling=fit -o position=top-left -- /tmp/print.pdf");
    assert_eq!(args[1], "Office Laser");
    let mut gray = request("p", json!({"colorOutput": "compositeGray", "reverseOrder": true, "scaleWidth": 150.0, "pagePosition": "centered"}));
    gray.file = PathBuf::from("a.pdf");
    let text = cups::lp_args(&gray).unwrap().join(" ");
    assert!(text.contains("-o outputorder=reverse") && text.contains("-o scaling=150") && text.contains("-o position=center") && text.contains("-o print-color-mode=monochrome"), "{text}");
    assert!(!text.contains("collate="), "one copy has no collation: {text}");
    assert_eq!(cups::lp_args(&request("p", json!({"scaleProportional": false, "scaleWidth": 50.0, "scaleHeight": 80.0}))).unwrap_err().kind, ErrorKind::Unsupported);
}

#[test]
fn cups_text_is_read() {
    assert_eq!(cups::parse_job_id("request id is Office_Laser-42 (1 file(s))\n").as_deref(), Some("Office_Laser-42"));
    assert_eq!(cups::parse_job_id("lp: error"), None);
    assert_eq!(cups::parse_default("system default destination: Office_Laser\n").as_deref(), Some("Office_Laser"));
    assert_eq!(cups::parse_default("no system default destination\n"), None);
    assert_eq!(cups::parse_destinations("Office_Laser\nCUPS-PDF\n\n"), vec!["Office_Laser", "CUPS-PDF"]);
    let papers = cups::parse_papers("PageSize/Media Size: *Letter Legal A4 Custom.WIDTHxHEIGHT\nResolution/Resolution: *600dpi\n");
    assert_eq!(papers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Letter", "Legal", "A4", "Custom.WIDTHxHEIGHT"]);
    assert_eq!(papers[2].size, Some([595.276, 841.89]));
    assert_eq!(papers[3].size, None);
    assert!(cups::parse_postscript("printer-info='x' printer-make-and-model='HP LaserJet 4 Foomatic/Postscript (recommended)' printer-state=3"));
    assert!(!cups::parse_postscript("printer-make-and-model='Epson Stylus raster'"));
}

#[test]
fn the_sheet_is_placed_and_scaled_on_the_printable_area() {
    // 240 x 300 pt at 600 dpi on a 4000 x 6000 px area
    let dpi = [600.0, 600.0];
    let area = [4000.0, 6000.0];
    let (origin, size, per_point) = placement([240.0, 300.0], Scale::Percent { width: 100.0, height: 100.0 }, Position::UpperLeft, dpi, area);
    assert_eq!((origin, size), ([0.0, 0.0], [2000.0, 2500.0]));
    assert!((per_point - 300.0 / 72.0).abs() < 1e-9, "never above 300 dpi: {per_point}");
    let (origin, size, _) = placement([240.0, 300.0], Scale::Percent { width: 50.0, height: 50.0 }, Position::Centered, dpi, area);
    assert_eq!((size, origin), ([1000.0, 1250.0], [1500.0, 2375.0]));
    let (_, size, _) = placement([240.0, 300.0], Scale::Fit, Position::UpperLeft, dpi, area);
    assert!((size[0] - 4000.0).abs() < 1e-6 && (size[1] - 5000.0).abs() < 1e-6, "{size:?}");
    let (origin, _, _) = placement([240.0, 300.0], Scale::Percent { width: 100.0, height: 100.0 }, Position::CenterHorizontally, dpi, area);
    assert_eq!(origin, [1000.0, 0.0]);
}

#[test]
fn the_system_device_names_itself_and_pdf_printers_are_recognised() {
    assert!(["windows", "cups", "unsupported"].contains(&system().kind()));
    assert!(is_virtual_pdf("Microsoft Print to PDF") && is_virtual_pdf("CUPS-PDF") && !is_virtual_pdf("EPSON WF-3620"));
}
