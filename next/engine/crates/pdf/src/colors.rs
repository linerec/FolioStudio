//! Ink-native colour: document paints → PDF colour spaces and values. The resolution itself
//! (swatches, tints, spot inks, mixed inks, Ink Manager conversion and aliases) is owned by
//! `folio_render::inks`, shared with the separations preview.

pub use folio_render::inks::*;

pub fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}
