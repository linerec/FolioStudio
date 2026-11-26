//! CUPS (macOS and Linux): the print PDF goes to `lp` as it is, CUPS converts it for the printer. The command lines are built and the tools' text is read
//! by plain functions that every platform compiles and tests; only `Cups` itself runs the tools.
use super::*;

/// The `lp` arguments of a request (the file last). A scale CUPS cannot do is an error, not a different print.
pub fn lp_args(r: &DeviceRequest) -> Result<Vec<String>, PrintError> {
    let mut a: Vec<String> = vec!["-d".into(), r.printer.clone(), "-n".into(), r.copies.to_string(), "-t".into(), r.job_name.clone()];
    let mut option = |text: String| { a.push("-o".into()); a.push(text); };
    if r.copies > 1 { option(format!("collate={}", r.collate)); }
    if r.reverse_order { option("outputorder=reverse".into()); }
    option(format!("orientation-requested={}", match r.orientation { Orientation::Portrait => 3, Orientation::Landscape => 4, Orientation::ReverseLandscape => 5, Orientation::ReversePortrait => 6 }));
    if let Some(paper) = &r.paper { option(format!("media={paper}")); }
    match r.scale {
        Scale::Fit => option("print-scaling=fit".into()),
        Scale::Percent { width, height } if (width - height).abs() > 1e-9 => return Err(PrintError::new(ErrorKind::Unsupported, "CUPS cannot scale width and height differently")),
        Scale::Percent { width, .. } if (width - 100.0).abs() > 1e-9 => option(format!("scaling={}", width.round() as i64)),
        Scale::Percent { .. } => option("print-scaling=none".into()),
    }
    option(format!("position={}", match r.position { Position::UpperLeft => "top-left", Position::CenterHorizontally => "top", Position::CenterVertically => "left", Position::Centered => "center" }));
    if r.color == ColorMode::Gray { option("print-color-mode=monochrome".into()); }
    a.push("--".into());
    a.push(r.file.display().to_string());
    Ok(a)
}

/// `request id is Mock-12 (1 file(s))` -> `Mock-12`.
pub fn parse_job_id(output: &str) -> Option<String> {
    let rest = output.lines().find_map(|l| l.trim().strip_prefix("request id is "))?;
    rest.split_whitespace().next().map(str::to_owned)
}

/// `system default destination: Office` -> `Office`; `no system default destination` -> none.
pub fn parse_default(output: &str) -> Option<String> {
    output.lines().find_map(|l| l.trim().strip_prefix("system default destination:")).map(|n| n.trim().to_owned()).filter(|n| !n.is_empty())
}

/// `lpstat -e`: one destination per line.
pub fn parse_destinations(output: &str) -> Vec<String> {
    output.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect()
}

fn known_size(name: &str) -> Option<[f64; 2]> {
    Some(match name { "Letter" => [612.0, 792.0], "Legal" => [612.0, 1008.0], "Tabloid" | "11x17" => [792.0, 1224.0], "A3" => [841.89, 1190.551], "A4" => [595.276, 841.89], "A5" => [419.528, 595.276], _ => return None })
}

/// `lpoptions -p NAME -l`: the `PageSize/Media Size: *Letter A4 ...` line. The name before a `/` is the PPD key; a leading `*` marks the default.
pub fn parse_papers(output: &str) -> Vec<Paper> {
    let Some(line) = output.lines().find(|l| l.starts_with("PageSize/") || l.starts_with("PageSize:")) else { return Vec::new() };
    let Some((_, list)) = line.split_once(':') else { return Vec::new() };
    list.split_whitespace().map(|n| n.trim_start_matches('*')).filter(|n| !n.is_empty())
        .map(|n| Paper { name: n.to_owned(), size: known_size(n) }).collect()
}

/// `lpoptions -p NAME`: `printer-make-and-model='HP LaserJet Foomatic/Postscript'` says whether the queue's driver speaks PostScript or PDF.
pub fn parse_postscript(output: &str) -> bool {
    output.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase().split("printer-make-and-model=").nth(1)
        .is_some_and(|rest| { let model: String = rest.chars().take(80).collect(); model.contains("postscript") || model.contains("pdf") })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub struct Cups;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod run {
    use super::*;
    use std::process::Command;

    pub fn tool(name: &str, args: &[&str]) -> Result<String, PrintError> {
        let out = Command::new(name).args(args).output().map_err(|e| PrintError::new(ErrorKind::Unsupported, format!("cannot run {name} (is CUPS installed?): {e}")))?;
        if !out.status.success() { return Err(PrintError::new(ErrorKind::DeviceFailed, format!("{name}: {}", String::from_utf8_lossy(&out.stderr).trim()))); }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Device for Cups {
    fn kind(&self) -> &'static str { "cups" }
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> {
        let default = run::tool("lpstat", &["-d"]).ok().and_then(|t| parse_default(&t));
        // `lpstat -e` fails (not empty) when no scheduler runs: that is "no printers", not a broken tool
        let names = run::tool("lpstat", &["-e"]).map(|t| parse_destinations(&t))?;
        Ok(names.into_iter().map(|name| PrinterInfo {
            is_default: default.as_deref() == Some(name.as_str()),
            postscript: run::tool("lpoptions", &["-p", &name]).is_ok_and(|t| parse_postscript(&t)),
            virtual_pdf: is_virtual_pdf(&name),
            papers: run::tool("lpoptions", &["-p", &name, "-l"]).map(|t| parse_papers(&t)).unwrap_or_default(),
            name,
        }).collect())
    }
    fn submit(&self, request: &DeviceRequest) -> Result<JobTicket, PrintError> {
        if request.output_file.is_some() { return Err(PrintError::new(ErrorKind::Unsupported, "CUPS PDF printers write where they are set up to; an output file cannot be given")); }
        let args = lp_args(request)?;
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let text = run::tool("lp", &refs)?;
        let id = parse_job_id(&text).ok_or_else(|| PrintError::new(ErrorKind::DeviceFailed, format!("lp gave no job id: {}", text.trim())))?;
        Ok(JobTicket { id, printer: request.printer.clone(), state: "queued", warnings: Vec::new() })
    }
    fn cancel(&self, _printer: &str, job: &str) -> Result<(), PrintError> {
        run::tool("cancel", &[job]).map(|_| ()).map_err(|e| if e.kind == ErrorKind::DeviceFailed { PrintError::new(ErrorKind::JobNotFound, e.message) } else { e })
    }
}
