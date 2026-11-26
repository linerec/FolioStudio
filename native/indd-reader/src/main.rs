//! Isolated, read-only INDD decoder. The Windows host supplies a private input
//! snapshot and an empty output directory, and enforces a Job memory/time limit.
#![forbid(unsafe_code)]
mod epsimage;
mod large;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::ExitCode,
};

const MIB: usize = 1024 * 1024;
fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(bytes)?;
    Ok(())
}
/// Largest saved-lines sidecar written; a larger one is left out with a
/// warning (the host then composes every story itself).
const MAX_LINES_JSON: usize = 32 * MIB;

/// Writes the saved composition sidecar and returns its manifest entry
/// (JSON): file, format, version and counts, or `null` when left out.
fn write_lines(
    folder: &Path,
    json: &[u8],
    s: indd_large::idml::lines::SavedLinesSummary,
    warnings: &mut Vec<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    if json.is_empty() {
        return Ok("null".into());
    }
    if json.len() > MAX_LINES_JSON {
        warnings.push(format!("Saved composition ({} bytes) omitted: sidecar limit", json.len()));
        return Ok("null".into());
    }
    write_new(&folder.join("saved-lines.json"), json)?;
    Ok(format!(
        "{{\"file\":\"saved-lines.json\",\"format\":{},\"version\":{},\"stories\":{},\"lines\":{},\"skipped\":{}}}",
        quoted(indd_large::idml::lines::FORMAT),
        indd_large::idml::lines::VERSION,
        s.stories,
        s.lines,
        s.skipped
    ))
}
fn decode(input: &Path, folder: &Path, name: &str, embedded: bool) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::File::open(input)?;
    if file.metadata()?.len() > (64 * MIB) as u64 {
        drop(file);
        return large::decode(input, folder, name, embedded);
    }
    let mut data = Vec::new();
    file.take((64 * MIB + 1) as u64).read_to_end(&mut data)?;
    if data.len() > 64 * MIB {
        return Err("INDD exceeds 64 MiB".into());
    }
    let result = indd_large::convert(&data, name)?;
    if result.idml.len() > 32 * MIB {
        return Err("Converted IDML exceeds 32 MiB".into());
    }
    write_new(&folder.join("document.idml"), &result.idml)?;
    let mut warnings: Vec<String> = Vec::new();
    let lines = write_lines(folder, &result.lines, result.lines_summary, &mut warnings)?;
    warnings.extend(result
        .warnings
        .iter()
        .take(400)
        .map(|w| w.message().chars().take(2000).collect::<String>()));
    if result.warnings.len() > 400 {
        warnings.push(format!(
            "{} additional decoder warnings omitted",
            result.warnings.len() - 400
        ));
    }
    let mut previews = Vec::new();
    let mut total = 0usize;
    for (i, preview) in result.previews.iter().enumerate() {
        if i >= 200
            || preview.data.len() > 20 * MIB
            || total.saturating_add(preview.data.len()) > 32 * MIB
        {
            warnings.push(format!("Preview {i} omitted: preview resource limit"));
            continue;
        }
        if preview.graphics.len() > 10000 {
            warnings.push(format!("Preview {i} omitted: too many graphic references"));
            continue;
        }
        total += preview.data.len();
        let filename = format!("preview-{i}.bin");
        write_new(&folder.join(&filename), &preview.data)?;
        let graphics = preview
            .graphics
            .iter()
            .map(|g| quoted(g))
            .collect::<Vec<_>>()
            .join(",");
        previews.push(format!(
            "{{\"file\":{},\"graphics\":[{}]}}",
            quoted(&filename),
            graphics
        ));
    }
    let warnings = warnings
        .iter()
        .map(|s| quoted(s))
        .collect::<Vec<_>>()
        .join(",");
    let manifest = format!(
        "{{\"ok\":true,\"parser\":\"indd-utils@7f3060e5+folio.7\",\"mode\":\"buffered\",\"warnings\":[{}],\"previews\":[{}],\"savedLines\":{},\"embedded\":null}}",
        warnings,
        previews.join(","),
        lines
    );
    if manifest.len() > MIB {
        return Err("Decoder manifest exceeds 1 MiB".into());
    }
    write_new(&folder.join("result.json"), manifest.as_bytes())
}
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    // --embedded (folio.7): also write the embedded files the lean IDML leaves out
    // (embedded.json); buffered documents keep them in the IDML as Contents.
    let embedded = args.len() == 5 && args[4] == "--embedded";
    if args.len() != 4 && !embedded {
        eprintln!("usage: folio-indd-reader INPUT EMPTY_OUTPUT_FOLDER DOCUMENT_NAME [--embedded]");
        return ExitCode::from(2);
    }
    let folder = Path::new(&args[2]);
    match decode(Path::new(&args[1]), folder, &args[3].to_string_lossy(), embedded) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let message: String = error.to_string().chars().take(4000).collect();
            let result = format!("{{\"ok\":false,\"error\":{}}}", quoted(&message));
            let _ = write_new(&folder.join("result.json"), result.as_bytes());
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
