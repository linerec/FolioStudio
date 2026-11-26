//! Guard the storage boundary behind the runtime-only interop classifications.
use std::{collections::{BTreeMap, BTreeSet}, path::Path};
use syn::{GenericArgument, Item, PathArguments, Type};

fn type_names(ty: &Type, names: &mut BTreeSet<String>) {
    match ty {
        Type::Path(p) => {
            for segment in &p.path.segments {
                names.insert(segment.ident.to_string());
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let GenericArgument::Type(ty) = arg { type_names(ty, names); }
                    }
                }
            }
        }
        Type::Array(a) => type_names(&a.elem, names),
        Type::Slice(s) => type_names(&s.elem, names),
        Type::Reference(r) => type_names(&r.elem, names),
        Type::Ptr(p) => type_names(&p.elem, names),
        Type::Tuple(t) => for ty in &t.elems { type_names(ty, names); },
        Type::Paren(p) => type_names(&p.elem, names),
        _ => panic!("extend the storage boundary audit for this Rust field type"),
    }
}

fn declarations(items: Vec<Item>, graph: &mut BTreeMap<String, BTreeSet<String>>) {
    for item in items {
        let (name, types): (String, Vec<Type>) = match item {
            Item::Struct(s) => (s.ident.to_string(), s.fields.into_iter().map(|f| f.ty).collect()),
            Item::Enum(e) => (e.ident.to_string(), e.variants.into_iter().flat_map(|v| v.fields).map(|f| f.ty).collect()),
            Item::Type(t) => (t.ident.to_string(), vec![*t.ty]),
            Item::Mod(m) if m.ident != "tests" => {
                if let Some((_, items)) = m.content { declarations(items, graph); }
                continue;
            }
            _ => continue,
        };
        for ty in types { type_names(&ty, graph.entry(name.clone()).or_default()); }
        graph.entry(name).or_default();
    }
}

fn walk(dir: &Path, graph: &mut BTreeMap<String, BTreeSet<String>>) {
    for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() { walk(&path, graph); }
        else if path.extension().is_some_and(|e| e == "rs") {
            declarations(syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap().items, graph);
        }
    }
}

#[test]
fn composition_types_are_not_stored_document_fields() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut graph = BTreeMap::new();
    walk(&src, &mut graph);
    let mut runtime = BTreeMap::new();
    declarations(syn::parse_file(&std::fs::read_to_string(src.join("layout.rs")).unwrap()).unwrap().items, &mut runtime);
    assert!(runtime.contains_key("DocumentLayout") && runtime.contains_key("OutlineCmd") && runtime.contains_key("FontId"));
    // Union declarations sharing a short name: over-reporting a dependency is safer than missing one.
    let mut visited = BTreeSet::new();
    let mut queue = vec!["Document".to_owned()];
    while let Some(name) = queue.pop() {
        if visited.insert(name.clone()) {
            assert!(!runtime.contains_key(&name), "{name} now enters the document schema; re-audit runtime interop rows");
            if let Some(edges) = graph.get(&name) { queue.extend(edges.iter().cloned()); }
        }
    }
    assert!(visited.contains("Story") && visited.contains("PageItem") && visited.contains("SavedLine"));
    // These are arguments/results of geometry calculations. The corresponding IDML
    // stroke attributes live in Stroke / stroke_styles::Definition, not these values.
    for name in ["Seg", "StrokeSpec", "StrokePattern", "CornerAdjust", "HashKind"] {
        assert!(graph.contains_key(name), "geometry helper disappeared: {name}");
        assert!(!visited.contains(name), "{name} entered the document schema; re-audit its interop rows");
    }
    assert!(visited.contains("StrokeArt"), "lowered stroke art is a document field, not a transient helper");
}
