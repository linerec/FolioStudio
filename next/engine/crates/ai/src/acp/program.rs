use std::path::{Path, PathBuf};
use std::process::Command;

pub fn executable(command: &str) -> Result<Command, String> {
    let directories = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .filter(|dir| dir.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    resolve(command, directories)
}

fn resolve(command: &str, directories: Vec<PathBuf>) -> Result<Command, String> {
    let path = Path::new(command);
    if path.is_absolute() {
        if let Some(program) = native(path) {
            return Ok(Command::new(program));
        }
    } else if path.components().count() == 1 {
        for dir in &directories {
            if let Some(program) = native(&dir.join(command)) {
                return Ok(Command::new(program));
            }
            #[cfg(windows)]
            if let Some(entry) = npm_entry(dir, command)
                && let Some(node) = directories.iter().find_map(|dir| native(&dir.join("node")))
            {
                let mut program = Command::new(node);
                program.arg(entry);
                return Ok(program);
            }
        }
    }
    Err("ACP executable not found".into())
}

#[cfg(windows)]
fn native(path: &Path) -> Option<PathBuf> {
    let exe = if path.extension().is_none() {
        path.with_extension("exe")
    } else {
        path.to_owned()
    };
    (exe.extension()?.to_str()?.eq_ignore_ascii_case("exe") && exe.is_file()).then_some(exe)
}

#[cfg(unix)]
fn native(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = path.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then(|| path.to_owned())
}

#[cfg(windows)]
fn npm_entry(dir: &Path, command: &str) -> Option<PathBuf> {
    let package = match command {
        "claude-agent-acp" => "@agentclientprotocol/claude-agent-acp",
        "codex-acp" => "@agentclientprotocol/codex-acp",
        "gemini" => "@google/gemini-cli",
        _ => return None,
    };
    let root = dir.join("node_modules").join(package);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("package.json")).ok()?).ok()?;
    if manifest["name"] != package {
        return None;
    }
    let relative = Path::new(manifest["bin"][command].as_str()?);
    if !relative.components().all(|part| {
        matches!(
            part,
            std::path::Component::CurDir | std::path::Component::Normal(_)
        )
    }) {
        return None;
    }
    let entry = root.join(relative);
    (matches!(entry.extension()?.to_str()?, "js" | "cjs" | "mjs") && entry.is_file())
        .then_some(entry)
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    #[test]
    fn npm_preset_uses_node_and_ignores_batch_text() {
        let root = PathBuf::from(
            std::env::var_os("FOLIO_USER_DATA")
                .unwrap_or_else(|| std::env::temp_dir().into_os_string()),
        )
        .join(folio_project::util::uuid_v4());
        let pkg = root.join("node_modules/@google/gemini-cli");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(root.join("node.exe"), []).unwrap();
        std::fs::write(root.join("gemini.cmd"), "ignored").unwrap();
        std::fs::write(
            pkg.join("package.json"),
            r#"{"name":"@google/gemini-cli","bin":{"gemini":"entry.js"}}"#,
        )
        .unwrap();
        std::fs::write(pkg.join("entry.js"), []).unwrap();
        let command = resolve("gemini", vec![root.clone()]).unwrap();
        assert_eq!(command.get_program(), root.join("node.exe"));
        assert_eq!(
            command.get_args().map(Path::new).collect::<Vec<_>>(),
            vec![pkg.join("entry.js")]
        );
        assert!(resolve("gemini.cmd", vec![root.clone()]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
