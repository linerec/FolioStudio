use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn read_zip(path: &Path) -> BTreeMap<String, String> {
    let f = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        out.insert(e.name().unwrap().into_owned(), s);
    }
    out
}
