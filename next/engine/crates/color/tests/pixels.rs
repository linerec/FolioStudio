//! Pixel conversion must agree with scalar conversion, including repeated colours.
use folio_color::{BlendSpace, Colors};

#[test]
fn managed_rgba_preserves_channel_order_and_alpha() {
    // Generated from mathematical primaries; no installed or bundled ICC file is needed.
    let profile = moxcms::ColorProfile::new_display_p3().encode().unwrap();
    let colors = Colors::from_profiles(None, Some(&profile)).unwrap();
    assert!(colors.rgb_managed());
    let input = [[180, 90, 40, 255], [180, 90, 40, 63], [20, 100, 210, 128],
                 [180, 90, 40, 0], [180, 90, 40, 255]];
    let mut pixels: Vec<u8> = input.iter().flatten().copied().collect();
    colors.rgba_to_display(&mut pixels, BlendSpace::Rgb);
    assert_ne!(&pixels[..3], &input[0][..3], "the managed path must actually transform colour");
    for (actual, source) in pixels.chunks_exact(4).zip(input) {
        assert_eq!(&actual[..3], &colors.rgb_to_srgb([source[0], source[1], source[2]]));
        assert_eq!(actual[3], source[3]);
    }
}
