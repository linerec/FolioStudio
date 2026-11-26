//! Layout of MathML into drawings: relations between sizes that TeX's rules give, and the
//! dimensions InDesign's own drawings of the same equations have (approximately).

use folio_math::{layout, normalize_source, styled_source, Equation, Options, EM_PER_SIZE};

fn eq(src: &str) -> Equation {
    layout(src, &Options::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}
fn math(inner: &str) -> String {
    format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{inner}</math>")
}
fn dims(inner: &str) -> (f64, f64) {
    let e = eq(&math(inner));
    (e.width, e.height)
}

#[test]
fn simple_row_has_size_and_drawing() {
    let e = eq(&math("<mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow>"));
    assert!(e.width > 20.0 && e.width < 60.0, "{}", e.width);
    assert!(e.height > 5.0 && e.height < 25.0, "{}", e.height);
    assert!(e.svg.starts_with("<svg") && e.svg.contains("viewBox=\"") && e.svg.contains("scale(1,-1)"));
    assert!(e.svg.contains("<path d=\"M"));
    assert!(e.preview_svg.contains("pt\""));
    assert!(!e.svg.contains("font-family"), "the drawing must not need a font");
}

#[test]
fn svg_data_is_what_indesign_stores() {
    let e = eq(&math("<mi>x</mi>"));
    let v: serde_json::Value = serde_json::from_str(&e.svg_data()).unwrap();
    assert!(v["width"].as_f64().unwrap() > 0.0 && v["height"].as_f64().unwrap() > 0.0);
    assert_eq!(v["svg_str"].as_str().unwrap(), e.svg);
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["height", "svg_str", "width"]);
}

#[test]
fn fraction_is_taller_than_its_parts_and_nests() {
    let (_, h_x) = dims("<mi>x</mi>");
    let (_, h_f) = dims("<mfrac><mi>a</mi><mi>b</mi></mfrac>");
    let (_, h_ff) = dims("<mfrac><mfrac><mi>a</mi><mi>b</mi></mfrac><mi>c</mi></mfrac>");
    assert!(h_f > 1.4 * h_x, "{h_f} {h_x}");
    assert!(h_ff > h_f);
}

#[test]
fn fraction_without_bar_has_no_rect() {
    let with = eq(&math("<mfrac><mi>a</mi><mi>b</mi></mfrac>"));
    let without = eq(&math("<mfrac linethickness=\"0\"><mi>a</mi><mi>b</mi></mfrac>"));
    assert!(with.svg.contains("<rect"));
    assert!(!without.svg.contains("<rect"));
}

#[test]
fn square_root_is_wider_and_has_a_rule() {
    let (w, _) = dims("<mi>x</mi>");
    let e = eq(&math("<msqrt><mi>x</mi></msqrt>"));
    assert!(e.width > w + 5.0);
    assert!(e.svg.contains("<rect"));
    let root = eq(&math("<mroot><mi>x</mi><mn>3</mn></mroot>"));
    assert!(root.width > e.width);
}

#[test]
fn superscript_rises_and_subscript_sinks() {
    let base = eq(&math("<mi>x</mi>"));
    let sup = eq(&math("<msup><mi>x</mi><mn>2</mn></msup>"));
    let sub = eq(&math("<msub><mi>x</mi><mn>2</mn></msub>"));
    let both = eq(&math("<msubsup><mi>x</mi><mn>1</mn><mn>2</mn></msubsup>"));
    // `height` is the whole drawing, `depth` what lies below the baseline
    let above = |e: &Equation| e.height - e.depth;
    assert!(above(&sup) > above(&base) + 1.0 && sup.depth <= base.depth + 0.5);
    assert!(sub.depth > base.depth + 1.0 && above(&sub) <= above(&base) + 0.5);
    assert!(above(&both) >= above(&sup) - 0.5 && both.depth >= sub.depth - 0.5);
    assert!(sup.width > base.width);
}

#[test]
fn scripts_are_smaller_than_the_base() {
    // the same digit in a superscript is narrower than on the line
    let (w_line, _) = dims("<mrow><mi>x</mi><mn>2</mn></mrow>");
    let (w_sup, _) = dims("<msup><mi>x</mi><mn>2</mn></msup>");
    assert!(w_sup < w_line);
}

#[test]
fn limits_go_above_and_below_in_display_style() {
    let src = "<munderover><mo>&#x2211;</mo><mn>1</mn><mi>n</mi></munderover>";
    let inline = layout(&format!("<math>{src}</math>"), &Options::default()).unwrap();
    let block = layout(&format!("<math display=\"block\">{src}</math>"), &Options::default()).unwrap();
    // inline the limits are scripts beside the sign; stacked ones make it taller
    assert!(block.height + block.depth > inline.height + inline.depth);
}

#[test]
fn matrix_has_rows_and_columns() {
    let cell = |s: &str| format!("<mtd><mi>{s}</mi></mtd>");
    let one = dims("<mtable><mtr><mtd><mi>a</mi></mtd></mtr></mtable>");
    let m = dims(&format!("<mtable><mtr>{}{}</mtr><mtr>{}{}</mtr></mtable>", cell("a"), cell("b"), cell("c"), cell("d")));
    assert!(m.0 > one.0 * 1.8 && m.1 > one.1 * 1.8, "{m:?} {one:?}");
}

#[test]
fn fences_stretch_to_what_they_enclose() {
    let plain = dims("<mrow><mo>(</mo><mi>a</mi><mo>)</mo></mrow>");
    let tall = dims("<mrow><mo>(</mo><mfrac><mi>a</mi><mi>b</mi></mfrac><mo>)</mo></mrow>");
    let frac = dims("<mfrac><mi>a</mi><mi>b</mi></mfrac>");
    assert!(tall.1 >= frac.1 - 0.5);
    assert!(tall.1 > plain.1 * 1.15, "{tall:?} {plain:?}");
    // a fence pair beside a tall sibling does not stretch to it
    let beside = dims("<mrow><mo>(</mo><mi>a</mi><mo>)</mo><mfrac><mi>a</mi><mi>b</mi></mfrac></mrow>");
    assert!(beside.1 < tall.1 + frac.1);
}

#[test]
fn fenced_is_a_row_with_fences() {
    let f = dims("<mfenced open=\"[\" close=\"]\"><mi>a</mi><mi>b</mi></mfenced>");
    let r = dims("<mrow><mo>[</mo><mi>a</mi><mo>,</mo><mi>b</mi><mo>]</mo></mrow>");
    assert!((f.0 - r.0).abs() < 1.0 && (f.1 - r.1).abs() < 1.0, "{f:?} {r:?}");
}

#[test]
fn relation_spacing_widens_a_row() {
    let tight = dims("<mrow><mi>a</mi><mi>b</mi></mrow>");
    let rel = dims("<mrow><mi>a</mi><mo>=</mo><mi>b</mi></mrow>");
    let bin = dims("<mrow><mi>a</mi><mo>+</mo><mi>b</mi></mrow>");
    assert!(rel.0 > tight.0 + 10.0 && bin.0 > tight.0 + 10.0);
    // a leading minus is unary: it gets no space after it
    let unary = dims("<mrow><mo>-</mo><mi>b</mi></mrow>");
    let binary = dims("<mrow><mi>a</mi><mo>-</mo><mi>b</mi></mrow>");
    assert!(unary.0 < binary.0 - 5.0, "{unary:?} {binary:?}");
}

#[test]
fn mspace_adds_its_width() {
    let a = dims("<mrow><mi>x</mi><mi>x</mi></mrow>");
    let b = dims("<mrow><mi>x</mi><mspace width=\"1em\"/><mi>x</mi></mrow>");
    assert!(((b.0 - a.0) - EM_PER_SIZE * 10.0).abs() < 1.0, "{} {}", b.0, a.0);
}

#[test]
fn size_scales_the_drawing() {
    let src = math("<mfrac><mi>a</mi><mi>b</mi></mfrac>");
    let a = layout(&src, &Options { size: 10.0, display: false, color: [0.0; 3] }).unwrap();
    let b = layout(&src, &Options { size: 20.0, display: false, color: [0.0; 3] }).unwrap();
    assert!((b.width / a.width - 2.0).abs() < 1e-9 && (b.height / a.height - 2.0).abs() < 1e-9);
    // the stored drawing does not depend on the size
    assert_eq!(a.svg, b.svg);
}

#[test]
fn variants_change_the_glyphs() {
    let normal = eq(&math("<mi>x</mi>"));
    let bold = eq(&math("<mi mathvariant=\"bold\">x</mi>"));
    let double = eq(&math("<mi mathvariant=\"double-struck\">R</mi>"));
    assert_ne!(normal.svg, bold.svg);
    assert!(double.width > 0.0);
    // a single letter is italic by default, upright when asked
    assert_ne!(eq(&math("<mi>f</mi>")).svg, eq(&math("<mi mathvariant=\"normal\">f</mi>")).svg);
}

#[test]
fn entities_and_characters_agree() {
    let a = eq(&math("<mi>&#x3B1;</mi>"));
    let b = eq(&math("<mi>&alpha;</mi>"));
    let c = eq(&math("<mi>\u{3B1}</mi>"));
    assert_eq!(a.svg, b.svg);
    assert_eq!(a.svg, c.svg);
}

#[test]
fn accents_sit_above_and_bars_span_the_base() {
    let base = eq(&math("<mi>x</mi>"));
    let hat = eq(&math("<mover accent=\"true\"><mi>x</mi><mo>^</mo></mover>"));
    let bar = eq(&math("<mover><mrow><mi>a</mi><mi>b</mi></mrow><mo>&#xAF;</mo></mover>"));
    let ab = eq(&math("<mrow><mi>a</mi><mi>b</mi></mrow>"));
    assert!(hat.height > base.height + 1.0);
    assert!(bar.height > ab.height + 1.0 && (bar.width - ab.width).abs() < 0.5);
    assert!(bar.svg.contains("<rect"));
}

#[test]
fn enclosures_and_multiscripts_lay_out() {
    let plain = dims("<mi>x</mi>");
    let boxed = dims("<menclose notation=\"box\"><mi>x</mi></menclose>");
    assert!(boxed.0 > plain.0 && boxed.1 > plain.1);
    let circle = eq(&math("<menclose notation=\"circle\"><mi>x</mi></menclose>"));
    assert!(circle.svg.matches('C').count() >= 4);
    let multi = dims("<mmultiscripts><mi>X</mi><mi>a</mi><mi>b</mi><mprescripts/><mi>c</mi><mi>d</mi></mmultiscripts>");
    assert!(multi.0 > dims("<mi>X</mi>").0 * 1.5);
}

#[test]
fn text_in_korean_is_drawn() {
    let e = eq(&math("<mtext>\u{D569}\u{ACC4}</mtext>"));
    assert!(e.width > 10.0);
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    for src in ["", "x", "<math>", "<math><mi>x</mn></math>", "<mrow><mi>x</mi></mrow>", "<math><mi>x</mi></math>x"] {
        assert!(layout(src, &Options::default()).is_err(), "{src:?} should fail");
    }
    // a fraction with a missing part lays out with a note instead of failing
    let e = eq(&math("<mfrac><mi>x</mi></mfrac>"));
    assert!(!e.notes.is_empty());
    assert!(layout(&math("<mi>x</mi>"), &Options { size: -1.0, display: false, color: [0.0; 3] }).is_err());
    assert!(layout(&math("<mi>x</mi>"), &Options { size: 0.0, display: false, color: [0.0; 3] }).is_err());
}

#[test]
fn unknown_elements_are_noted_and_laid_out_as_rows() {
    let e = eq(&math("<mrow><mi>a</mi><mfoo><mi>b</mi></mfoo></mrow>"));
    assert!(e.notes.iter().any(|n| n.contains("mfoo")), "{:?}", e.notes);
    assert!(e.width > 0.0);
}

#[test]
fn normalize_adds_the_namespace_once() {
    let n = normalize_source("<math><mi>x</mi></math>").unwrap();
    assert!(n.starts_with("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">"));
    assert_eq!(normalize_source(&n).unwrap(), n);
    assert!(normalize_source("<mi>x</mi>").is_err());
}

#[test]
fn deterministic() {
    let src = math("<mrow><mi>x</mi><mo>=</mo><mfrac><mn>1</mn><msqrt><mn>2</mn></msqrt></mfrac></mrow>");
    assert_eq!(eq(&src).svg, eq(&src).svg);
}

/// InDesign's own drawings of four equations (10 pt; rules/mathml.ko.md, goldens/mathml-c1/storage):
/// ours are close but not equal, as the font and its rules differ.
#[test]
fn dimensions_are_near_indesigns() {
    let cases: [(&str, f64, f64); 4] = [
        ("<mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow>", 37.141605, 12.106260),
        ("<mfrac><mn>1</mn><mn>2</mn></mfrac>", 12.843225, 19.583235),
        ("<mrow><msup><mi>x</mi><mn>2</mn></msup><msub><mi>y</mi><mn>1</mn></msub></mrow>", 31.324591, 16.821405),
        ("<msqrt><mrow><mi>x</mi><mo>+</mo><mtext>abc</mtext></mrow></msqrt>", 67.135367, 17.157690),
    ];
    let (font, math_font) = folio_math::font_info().unwrap();
    for (src, w, h) in cases {
        let e = eq(&math(src));
        let (rw, rh) = (e.width / w, e.height / h);
        eprintln!("{font} (math table {math_font}): {src}: width x{rw:.3}, height x{rh:.3}");
        assert!(rw > 0.8 && rw < 1.25 && rh > 0.8 && rh < 1.3, "{src}: x{rw:.3} x{rh:.3}");
    }
}

#[test]
fn styled_source_writes_size_and_colour_like_indesign() {
    let ns = "xmlns=\"http://www.w3.org/1998/Math/MathML\"";
    // defaults are not written
    assert_eq!(styled_source("<math><mi>x</mi></math>", 10.0, [0.0; 3]).unwrap(), format!("<math {ns}><mi>x</mi></math>"));
    assert_eq!(
        styled_source("<math><mi>x</mi></math>", 12.0, [17.0, 83.0, 149.0]).unwrap(),
        format!("<math {ns} mathsize=\"12pt\" mathcolor=\"#115395\"><mi>x</mi></math>")
    );
    // ours replace what the source had; other attributes stay; asking again changes nothing
    let once = styled_source("<math display=\"block\" mathsize=\"99pt\" mathcolor=\"#ff0000\"><mi>x</mi></math>", 8.5, [0.0, 0.0, 255.0]).unwrap();
    assert_eq!(once, format!("<math {ns} display=\"block\" mathsize=\"8.5pt\" mathcolor=\"#0000ff\"><mi>x</mi></math>"));
    assert_eq!(styled_source(&once, 8.5, [0.0, 0.0, 255.0]).unwrap(), once);
    // back to the defaults removes them
    assert_eq!(styled_source(&once, 10.0, [0.0; 3]).unwrap(), format!("<math {ns} display=\"block\"><mi>x</mi></math>"));
    // an empty-element root and quotes holding `>`
    assert!(styled_source("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" alttext=\"a>b\"><mi>x</mi></math>", 11.0, [0.0; 3]).unwrap().contains("alttext=\"a>b\" mathsize=\"11pt\""));
    assert!(styled_source("<mi>x</mi>", 10.0, [0.0; 3]).is_err());
}

#[test]
fn the_root_size_in_the_source_does_not_change_the_layout() {
    // InDesign writes mathsize on the root; the size comes from the option, so laying the stored source out again is stable
    let plain = layout("<math><mi>x</mi></math>", &Options { size: 12.0, display: false, color: [0.0; 3] }).unwrap();
    let styled = layout(&styled_source("<math><mi>x</mi></math>", 12.0, [1.0, 2.0, 3.0]).unwrap(), &Options { size: 12.0, display: false, color: [0.0; 3] }).unwrap();
    assert_eq!(plain.svg, styled.svg);
    assert!((plain.width - styled.width).abs() < 1e-9);
}

#[test]
fn a_colour_is_written_on_the_stored_drawing_like_indesign_does() {
    let src = math("<mi>x</mi>");
    let black = layout(&src, &Options::default()).unwrap();
    let blue = layout(&src, &Options { size: 10.0, display: false, color: [17.0, 83.0, 149.0] }).unwrap();
    assert!(black.svg.contains("<g data-mml-node=\"math\">") && !black.svg.contains("#"));
    assert!(blue.svg.contains("<g data-mml-node=\"math\" fill=\"#115395\" stroke=\"#115395\">"));
    // the preview takes its colour from the page (currentColor), whatever the equation's colour
    assert!(!blue.preview_svg.contains("#115395"));
    assert_eq!(black.width, blue.width);
}
