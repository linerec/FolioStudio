//! Workspace boundaries. Keep the permission table and ARCHITECTURE.ko.md together.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

type Graph = BTreeMap<String, BTreeSet<String>>;

// Production dependencies, including target-specific and optional dependencies.
// Known exception to the proposed dependency-free color layer: color uses model's
// Swatch/ColorSpace/Gradient types. Keeping that edge avoids a duplicate color model.
const ALLOWED: &[(&str, &[&str])] = &[
    ("model", &[]),
    ("math", &[]),
    ("spell", &[]),
    ("textimport", &[]),
    ("color", &["model"]),
    ("idml", &["color", "model"]),
    ("text", &["compose", "model"]),
    ("render", &["color", "model", "text"]),
    ("links", &["idml", "model"]),
    ("preflight", &["idml", "links", "model", "text"]),
    ("package", &["links", "model", "preflight", "text"]),
    ("project", &["idml", "links", "model", "package", "preflight", "text"]),
    ("pdf", &["color", "idml", "links", "model", "render", "text"]),
    ("edit", &["color", "idml", "links", "math", "model", "preflight", "spell", "textimport"]),
    // Data Merge from a database (dbmerge-a): the only crate with database drivers and their external dependencies
    ("dbsource", &["idml"]),
    ("engine", &["color", "dbsource", "edit", "idml", "links", "math", "model", "package", "pdf", "preflight", "project", "render", "spell", "text", "textimport"]),
];

// Read fixtures / compare layout in tests only. These do not grant runtime edges.
const TEST_ONLY: &[(&str, &[&str])] = &[
    ("text", &["idml"]),
    ("render", &["color", "idml", "math"]),
    ("edit", &["text"]),
];

fn permissions(rows: &[(&str, &[&str])]) -> Graph {
    rows.iter().map(|(owner, deps)| {
        (owner.to_string(), deps.iter().map(|s| s.to_string()).collect())
    }).collect()
}

fn cycle(graph: &Graph) -> bool {
    fn visit(node: &str, graph: &Graph, active: &mut BTreeSet<String>, done: &mut BTreeSet<String>) -> bool {
        if done.contains(node) { return false; }
        if !active.insert(node.to_string()) { return true; }
        if graph.get(node).is_some_and(|deps| deps.iter().any(|dep| visit(dep, graph, active, done))) {
            return true;
        }
        active.remove(node);
        done.insert(node.to_string());
        false
    }
    let mut done = BTreeSet::new();
    graph.keys().any(|node| visit(node, graph, &mut BTreeSet::new(), &mut done))
}

fn check(metadata: &serde_json::Value) -> Result<(), String> {
    let allowed = permissions(ALLOWED);
    let test_only = permissions(TEST_ONLY);
    let members: BTreeSet<_> = metadata["workspace_members"].as_array().unwrap().iter()
        .map(|id| id.as_str().unwrap()).collect();
    let mut production = Graph::new();
    let mut all = Graph::new();
    for package in metadata["packages"].as_array().unwrap() {
        if !members.contains(package["id"].as_str().unwrap()) { continue; }
        let name = package["name"].as_str().unwrap().strip_prefix("folio-")
            .ok_or("workspace package must be reviewed in the layer table")?;
        let Some(permitted) = allowed.get(name) else {
            return Err(format!("unreviewed workspace crate: {name}"));
        };
        let mut runtime = BTreeSet::new();
        let mut every = BTreeSet::new();
        for dep in package["dependencies"].as_array().unwrap() {
            let Some(target) = dep["name"].as_str().unwrap().strip_prefix("folio-") else { continue; };
            let dev = dep["kind"] == "dev";
            let accepted = if dev {
                test_only.get(name).is_some_and(|deps| deps.contains(target))
            } else {
                permitted.contains(target)
            };
            if !accepted {
                return Err(format!("forbidden {} dependency: {name} -> {target}", if dev { "test" } else { "production/build" }));
            }
            every.insert(target.to_string());
            if !dev { runtime.insert(target.to_string()); }
        }
        production.insert(name.to_string(), runtime);
        all.insert(name.to_string(), every);
    }
    if production.len() != allowed.len() {
        return Err("workspace membership changed: review the layer table".into());
    }
    if cycle(&production) || cycle(&all) { return Err("Folio dependency cycle".into()); }
    Ok(())
}

#[test]
fn workspace_dependencies_follow_architecture() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked", "--offline", "--manifest-path"])
        .arg(manifest).output().expect("run cargo metadata");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata = serde_json::from_slice(&output.stdout).expect("cargo metadata JSON");
    check(&metadata).unwrap();
}

#[test]
fn rejects_reverse_edges_and_cycles() {
    use serde_json::json;
    let mut packages: Vec<_> = ALLOWED.iter().map(|(name, _)| {
        json!({"id": name, "name": format!("folio-{name}"), "dependencies": []})
    }).collect();
    let members: Vec<_> = ALLOWED.iter().map(|(name, _)| *name).collect();
    for kind in [serde_json::Value::Null, json!("build"), json!("dev")] {
        packages[0]["dependencies"] = json!([{"name": "folio-engine", "kind": kind, "target": "cfg(windows)", "optional": true}]);
        assert!(check(&json!({"packages": packages, "workspace_members": members})).unwrap_err().contains("forbidden"));
    }
    assert!(cycle(&permissions(&[("a", &["b"]), ("b", &["a"])])));
    assert!(!cycle(&permissions(&[("a", &["b"]), ("b", &[])])));
}
