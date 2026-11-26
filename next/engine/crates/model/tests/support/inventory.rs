//! Rust AST를 읽는다. 정규식이나 기본값 직렬화는 enum·Option·새 필드를 놓친다.
use std::collections::BTreeSet;
use std::path::Path;
use syn::{Fields, Item, Visibility};

fn fields(out: &mut BTreeSet<String>, prefix: &str, fields: &Fields) {
    for (index, field) in fields.iter().enumerate() {
        let name = field.ident.as_ref().map(ToString::to_string).unwrap_or(index.to_string());
        out.insert(format!("{prefix}.{name}"));
    }
}

pub fn declarations(source: &str, prefix: &str) -> BTreeSet<String> {
    let file = syn::parse_file(source).expect("valid Rust model source");
    items_inventory(file.items, prefix)
}

fn items_inventory(items: Vec<Item>, prefix: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for item in items {
        match item {
            Item::Struct(s) if matches!(s.vis, Visibility::Public(_)) => {
                let name = format!("{prefix}{}", s.ident);
                out.insert(name.clone());
                fields(&mut out, &name, &s.fields);
            }
            Item::Enum(e) if matches!(e.vis, Visibility::Public(_)) => {
                let name = format!("{prefix}{}", e.ident);
                out.insert(name.clone());
                for variant in e.variants {
                    let v = format!("{name}::{}", variant.ident);
                    out.insert(v.clone());
                    fields(&mut out, &v, &variant.fields);
                }
            }
            Item::Type(t) if matches!(t.vis, Visibility::Public(_)) => { out.insert(format!("{prefix}{}", t.ident)); }
            Item::Union(u) if matches!(u.vis, Visibility::Public(_)) => {
                let name = format!("{prefix}{}", u.ident);
                out.insert(name.clone());
                fields(&mut out, &name, &Fields::Named(u.fields));
            }
            Item::Macro(_) => panic!("모델 최상위 매크로는 상호운용 목록 수집을 먼저 구현해야 합니다"),
            Item::Mod(m) if m.ident != "tests" && m.ident != "interop" => {
                if let Some((_, items)) = m.content {
                    // Inline modules are handled through the same AST walker.
                    out.extend(items_inventory(items, &format!("{prefix}{}::", m.ident)));
                }
            }
            _ => {}
        }
    }
    out
}

pub fn inventory(src: &Path) -> BTreeSet<String> {
    fn walk(dir: &Path, src: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() { walk(&path, src, out); }
            else if path.extension().is_some_and(|e| e == "rs") && path.file_stem().unwrap() != "interop" {
                let relative = path.strip_prefix(src).unwrap().with_extension("");
                let parts: Vec<_> = relative.iter().map(|p| p.to_string_lossy()).filter(|p| p != "lib" && p != "mod").collect();
                let prefix = if parts.is_empty() { String::new() } else { parts.join("::") + "::" };
                out.extend(declarations(&std::fs::read_to_string(&path).unwrap(), &prefix));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(src, src, &mut out);
    out
}
