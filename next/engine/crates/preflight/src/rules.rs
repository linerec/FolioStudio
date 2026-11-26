//! Rule ids. Stable: the UI and saved reports refer to them.

pub const FONT_MISSING: &str = "FONT_MISSING";
pub const FONT_SUBSTITUTED: &str = "FONT_SUBSTITUTED";
/// A placed PDF / EPS uses a font it does not contain.
pub const FONT_NOT_EMBEDDED: &str = "FONT_NOT_EMBEDDED";
/// Characters that print as an empty box (the font's .notdef glyph) because their font has
/// no glyph for them (InDesign Preflight "Missing glyphs").
pub const MISSING_GLYPH: &str = "MISSING_GLYPH";

pub const LINK_MISSING: &str = "LINK_MISSING";
pub const LINK_MODIFIED: &str = "LINK_MODIFIED";
pub const LINK_AMBIGUOUS: &str = "LINK_AMBIGUOUS";
pub const LINK_EMBEDDED: &str = "LINK_EMBEDDED";
/// Found somewhere else than recorded, by file name only (could be another file).
pub const LINK_NAME_ONLY: &str = "LINK_NAME_ONLY";
/// Found by InDesign's relative search or by name + folder: which file is used.
pub const LINK_RELOCATED: &str = "LINK_RELOCATED";
pub const LINK_UNREADABLE: &str = "LINK_UNREADABLE";
/// A placed EPS: Folio shows and prints the preview the file carries, not its PostScript (build-a).
pub const LINK_EPS_PREVIEW: &str = "LINK_EPS_PREVIEW";

pub const IMAGE_RESOLUTION: &str = "IMAGE_RESOLUTION";
pub const COLOR_SPACE: &str = "COLOR_SPACE";

pub const SPOT_COLORS: &str = "SPOT_COLORS";
pub const SPOT_UNUSED: &str = "SPOT_UNUSED";
pub const SPOT_LOOKS_PROCESS: &str = "SPOT_LOOKS_PROCESS";
pub const INK_ALIAS: &str = "INK_ALIAS";
pub const SPOT_TO_PROCESS: &str = "SPOT_TO_PROCESS";

pub const INK_COVERAGE: &str = "INK_COVERAGE";
pub const IMAGE_INK_COVERAGE: &str = "IMAGE_INK_COVERAGE";

pub const SMALL_TEXT_INKS: &str = "SMALL_TEXT_INKS";
pub const REGISTRATION_COLOR: &str = "REGISTRATION_COLOR";
pub const WHITE_OVERPRINT: &str = "WHITE_OVERPRINT";
pub const OVERPRINT_COLOR: &str = "OVERPRINT_COLOR";
pub const BLACK_TEXT_KNOCKOUT: &str = "BLACK_TEXT_KNOCKOUT";
pub const HAIRLINE: &str = "HAIRLINE";
pub const OVERSET_TEXT: &str = "OVERSET_TEXT";

pub const BLEED_MISSING: &str = "BLEED_MISSING";
pub const SAFETY_MARGIN: &str = "SAFETY_MARGIN";
pub const HIDDEN_CONTENT: &str = "HIDDEN_CONTENT";
pub const EMPTY_FRAME: &str = "EMPTY_FRAME";
pub const TRANSPARENCY_SPOT: &str = "TRANSPARENCY_SPOT";
/// Opacity, blend mode or effects (InDesign "Transparency not allowed").
pub const TRANSPARENCY_USED: &str = "TRANSPARENCY_USED";
pub const BLANK_PAGE: &str = "BLANK_PAGE";
pub const UNSUPPORTED_EFFECT: &str = "UNSUPPORTED_EFFECT";
pub const NO_CMYK_PROFILE: &str = "NO_CMYK_PROFILE";
pub const PAGE_SIZES: &str = "PAGE_SIZES";

/// Korean title of a rule, as the preflight panel and the package report name it.
pub fn title(rule: &str) -> Option<&'static str> {
    Some(match rule {
        FONT_MISSING => "없는 글꼴",
        FONT_SUBSTITUTED => "대체된 글꼴",
        FONT_NOT_EMBEDDED => "포함할 수 없는 글꼴",
        MISSING_GLYPH => "없는 글리프",
        LINK_MISSING => "없는 링크",
        LINK_MODIFIED => "수정된 링크",
        LINK_AMBIGUOUS => "같은 이름이 여러 개인 링크",
        LINK_NAME_ONLY => "이름으로만 찾은 링크",
        LINK_RELOCATED => "다른 위치에서 찾은 링크",
        LINK_UNREADABLE => "읽을 수 없는 링크",
        LINK_EPS_PREVIEW => "미리 보기로 출력되는 EPS",
        LINK_EMBEDDED => "포함된 링크",
        IMAGE_RESOLUTION => "이미지 해상도 부족",
        COLOR_SPACE => "RGB·Lab 색 공간",
        IMAGE_INK_COVERAGE => "이미지 잉크 총량 초과",
        NO_CMYK_PROFILE => "CMYK 프로파일 없음",
        INK_COVERAGE => "잉크 총량 초과",
        SPOT_COLORS => "별색",
        SPOT_UNUSED => "쓰이지 않는 별색",
        SPOT_LOOKS_PROCESS => "원색 이름의 별색",
        SPOT_TO_PROCESS => "원색으로 바뀌는 별색",
        INK_ALIAS => "잉크 별칭",
        REGISTRATION_COLOR => "맞춰찍기 색 사용",
        TRANSPARENCY_SPOT => "투명도와 겹친 별색",
        WHITE_OVERPRINT => "흰색 오버프린트",
        OVERPRINT_COLOR => "오버프린트",
        BLACK_TEXT_KNOCKOUT => "녹아웃된 검정 글자",
        SMALL_TEXT_INKS => "여러 잉크로 된 작은 글자",
        OVERSET_TEXT => "넘친 텍스트",
        HAIRLINE => "가는 선",
        HIDDEN_CONTENT => "숨은 개체",
        EMPTY_FRAME => "빈 프레임",
        TRANSPARENCY_USED => "투명도 사용",
        UNSUPPORTED_EFFECT => "아직 그리지 못하는 효과",
        BLEED_MISSING => "도련 부족",
        SAFETY_MARGIN => "재단선에 가까운 개체",
        BLANK_PAGE => "빈 페이지",
        PAGE_SIZES => "페이지 크기가 다름",
        _ => return None,
    })
}

/// Every rule id, in report order.
pub const ALL: &[&str] = &[
    FONT_MISSING,
    FONT_SUBSTITUTED,
    FONT_NOT_EMBEDDED,
    MISSING_GLYPH,
    LINK_MISSING,
    LINK_MODIFIED,
    LINK_AMBIGUOUS,
    LINK_EMBEDDED,
    LINK_NAME_ONLY,
    LINK_RELOCATED,
    LINK_UNREADABLE,
    LINK_EPS_PREVIEW,
    IMAGE_RESOLUTION,
    COLOR_SPACE,
    SPOT_COLORS,
    SPOT_UNUSED,
    SPOT_LOOKS_PROCESS,
    INK_ALIAS,
    SPOT_TO_PROCESS,
    INK_COVERAGE,
    IMAGE_INK_COVERAGE,
    SMALL_TEXT_INKS,
    REGISTRATION_COLOR,
    WHITE_OVERPRINT,
    OVERPRINT_COLOR,
    BLACK_TEXT_KNOCKOUT,
    HAIRLINE,
    OVERSET_TEXT,
    BLEED_MISSING,
    SAFETY_MARGIN,
    HIDDEN_CONTENT,
    EMPTY_FRAME,
    TRANSPARENCY_SPOT,
    TRANSPARENCY_USED,
    BLANK_PAGE,
    UNSUPPORTED_EFFECT,
    NO_CMYK_PROFILE,
    PAGE_SIZES,
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_rule_has_a_title() {
        for r in super::ALL {
            assert!(super::title(r).is_some(), "{r}");
        }
        assert_eq!(super::title("NOT_A_RULE"), None);
    }
}
