//! One field registry produces both serde messages and the renderer's types.
use serde::{Deserialize, Serialize};

trait Ts { fn ts() -> String; }
macro_rules! scalar { ($($ty:ty),*) => { $(impl Ts for $ty { fn ts()->String {"number".into()} })* }; }
scalar!(u16, u32, usize, i16);
impl Ts for String { fn ts()->String {"string".into()} }
impl<T:Ts> Ts for Vec<T> { fn ts()->String {format!("Array<{}>",T::ts())} }
impl<T:Ts> Ts for Option<T> { fn ts()->String {format!("{} | null",T::ts())} }
impl<T:Ts,const N:usize> Ts for [T;N] { fn ts()->String {format!("[{}]",vec![T::ts();N].join(", "))} }
fn camel(name:&str)->String {
    let mut upper=false;
    name.chars().filter_map(|c| if c=='_' {upper=true;None} else if upper {upper=false;Some(c.to_ascii_uppercase())} else {Some(c)}).collect()
}
macro_rules! message {
    ($name:ident, $ts:literal { $($(#[$meta:meta])* $field:ident:$ty:ty),* $(,)? }) => {
        #[derive(Debug, Serialize, Deserialize)]
        #[serde(rename_all="camelCase")]
        pub struct $name { $($(#[$meta])* pub $field:$ty),* }
        impl Ts for $name { fn ts()->String {$ts.into()} }
        impl $name {
            fn declaration()->String {
                format!("export type {} = {{ {} }};\n",$ts,vec![$(format!("{}: {}",camel(stringify!($field)),<$ty as Ts>::ts())),*].join("; "))
            }
        }
    };
}
macro_rules! filter {
    ($first:ident $(,$other:ident)*) => {
        #[derive(Debug, Default, Serialize, Deserialize)]
        #[serde(rename_all="camelCase")]
        pub enum Filter {#[default] $first, $($other),*}
        impl Ts for Filter {fn ts()->String {"GlyphFilter".into()}}
        impl Filter {fn declaration()->String {format!("export type GlyphFilter = {};\n",vec![stringify!($first),$(stringify!($other)),*].iter().map(|s|format!("'{}'",s.to_lowercase())).collect::<Vec<_>>().join(" | "))}}
    };
}
filter!(All, Mapped, Unmapped, Alternates);
fn page_size()->usize {64}
message!(Request, "GlyphRequest" {
    family:String, style:String,
    #[serde(default)] offset:usize, #[serde(default="page_size")] limit:usize,
    #[serde(default)] search:String, #[serde(default)] filter:Filter,
    /// The selected text, for the `alternates` filter (Alternates for Selection).
    #[serde(default)] text:String,
    /// "gid" (default, the panel menu's By CID / GID) or "unicode" (By Unicode): the order of the list (not of alternates).
    #[serde(default)] order:String,
    /// The Show list item to list (Glyphs panel): "" or "alternates" / "entire" (by `filter`), "range:<id>" or "feature:<tag>" (see `show`),
    /// or "ids:<glyph id>,..." (those glyphs in that order: Recent Glyphs).
    #[serde(default)] show:String,
});
message!(ShowItem, "GlyphShowItem" {
    /// "recent", "alternates", "entire", "range:<id>" or "feature:<tag>".
    id:String, name:String,
    /// 0 Recent Glyphs, 1 Alternates / Entire Font, 2 Unicode ranges, 3 right-to-left script features, 4 other features (the panel's dividers).
    group:u16,
});
message!(Page, "GlyphPage" {
    family:String, style:String, postscript:String, glyph_count:usize, total:usize,
    offset:usize, limit:usize, units_per_em:u16, entries:Vec<Entry>,
    /// The Show list of this face (InDesign's items, in its order).
    show_items:Vec<ShowItem>,
});
message!(Entry, "GlyphEntry" {
    id:u16, unicode:Vec<u32>, name:Option<String>, insertable:Vec<u32>,
    path:String, bounds:Option<[i16;4]>, advance:u16,
    /// The substitution feature tags that give the glyph (`alternates` filter only).
    #[serde(default)] features:Vec<String>,
});
pub fn typescript()->String {
    format!("// Generated: cargo run -p folio-text --example export_glyph_types\n// Do not edit; regenerate from the Rust registry. Keep LF line endings.\n{}{}{}{}{}",Filter::declaration(),Request::declaration(),ShowItem::declaration(),Entry::declaration(),Page::declaration())
}
#[test]
fn generated_types_are_current() {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app/renderer/src/generated/glyphs.ts");
    assert_eq!(std::fs::read_to_string(path).unwrap(),typescript());
}
