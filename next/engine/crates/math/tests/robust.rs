//! Nasty input: an error or a drawing, never a panic or a stack overflow.
use folio_math::{layout, Options};

#[test]
fn damaged_xml_and_invalid_sizes_return_errors() {
    for src in ["", "<", "<math", "<math><mi>x</math>", "<math><mi>"] {
        assert!(layout(src, &Options::default()).is_err(), "{src}");
    }
    for size in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 1001.0] {
        assert!(layout("<math><mi>x</mi></math>", &Options { size, ..Options::default() }).is_err());
    }
}

#[test]
fn nesting_is_limited() {
    let deep = |n: usize| format!("<math>{}<mi>x</mi>{}</math>", "<mrow>".repeat(n), "</mrow>".repeat(n));
    assert!(layout(&deep(50), &Options::default()).is_ok());
    assert!(layout(&deep(5000), &Options::default()).is_err(), "very deep nesting is refused, not a stack overflow");
}

#[test]
fn odd_markup_gives_a_finite_drawing_or_an_error() {
    let cases = [
        "<math/>",
        "<math></math>",
        "<math><mrow/></math>",
        "<math><mfrac/></math>",
        "<math><msqrt/></math>",
        "<math><mroot><mi>x</mi></mroot></math>",
        "<math><msubsup><mi>x</mi></msubsup></math>",
        "<math><munderover/></math>",
        "<math><mtable/></math>",
        "<math><mtable><mtr/></mtable></math>",
        "<math><mtable><mtr><mtd/></mtr></mtable></math>",
        "<math><mrow><mo>(</mo></mrow></math>",
        "<math><mrow><mo>)</mo><mo>(</mo><mo>|</mo><mo>|</mo><mo>|</mo></mrow></math>",
        "<math><mrow><mo>(</mo><mo>(</mo><mo>)</mo></mrow></math>",
        "<math><mo></mo></math>",
        "<math><mi></mi><mn></mn><mtext></mtext></math>",
        "<math><mo>&#x2211;</mo></math>",
        "<math display=\"block\"><mo>&#x222B;</mo><mo>&#x221A;</mo><mo>(</mo></math>",
        "<math><mspace width=\"-3em\"/><mspace width=\"abc\"/></math>",
        "<math><mfrac linethickness=\"-5\"><mi>a</mi><mi>b</mi></mfrac></math>",
        "<math><mpadded width=\"+1em\" height=\"50%\" depth=\"-2pt\" lspace=\"x\"><mi>a</mi></mpadded></math>",
        "<math><menclose notation=\"circle box radical longdiv\"><mi>a</mi></menclose></math>",
        "<math><mmultiscripts/></math>",
        "<math><mmultiscripts><mi>a</mi><mprescripts/></mmultiscripts></math>",
        "<math><mover accent=\"true\"><mo>&#xAF;</mo><mo>&#xAF;</mo></mover></math>",
        "<math><mi mathvariant=\"nonsense\">\u{1F600}</mi></math>",
        "<math><mtext>&#x0;</mtext></math>",
        "<math><mn>&unknown;</mn></math>",
        "<math>&missing;</math>",
        "<math><mrow><mo stretchy=\"true\">+</mo><mo stretchy=\"true\">=</mo></mrow></math>",
        "<math><mtable><mtr><mtd><mtable><mtr><mtd><mi>a</mi></mtd></mtr></mtable></mtd></mtr></mtable></math>",
    ];
    for src in cases {
        if let Ok(e) = layout(src, &Options::default()) {
            assert!(e.width.is_finite() && e.height.is_finite() && e.depth.is_finite(), "{src}");
            assert!(e.width >= 0.0 && e.height >= 0.0, "{src}");
        }
    }
}
