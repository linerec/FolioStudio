//! What `folio-math` lays out is drawn by the screen renderer (and the PDF writer, which shares the
//! reader of the stored SVG): every shape inside the frame that has the drawing's size.

use folio_model::mathml::MathObject;
use folio_model::Rect;

fn object(eq: &folio_math::Equation) -> MathObject {
    MathObject {
        id: "m".into(),
        source: None,
        font_size: Some(10.0),
        rgb: Some([0.0; 3]),
        tint: Some(100.0),
        swatch: None,
        is_mathml: Some(true),
        svg_data: Some(eq.svg_data()),
    }
}

#[test]
fn laid_out_equations_are_read_back_as_shapes_inside_their_frame() {
    for src in [
        "<math><mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow></math>",
        "<math><mfrac><mn>1</mn><mn>2</mn></mfrac></math>",
        "<math><msqrt><mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow></msqrt></math>",
        "<math display=\"block\"><mrow><munderover><mo>&#x2211;</mo><mi>i</mi><mi>n</mi></munderover><msup><mi>i</mi><mn>2</mn></msup></mrow></math>",
        "<math><mrow><mo>(</mo><mtable><mtr><mtd><mn>1</mn></mtd><mtd><mn>0</mn></mtd></mtr><mtr><mtd><mn>0</mn></mtd><mtd><mn>1</mn></mtd></mtr></mtable><mo>)</mo></mrow></math>",
        "<math><menclose notation=\"circle\"><mi>a</mi></menclose></math>",
    ] {
        let eq = folio_math::layout(src, &folio_math::Options { size: 12.0, display: false, color: [0.0; 3] }).unwrap();
        let frame = Rect { left: 100.0, top: 200.0, right: 100.0 + eq.width, bottom: 200.0 + eq.height };
        let shapes = folio_render::mathsvg::shapes(&object(&eq), &frame).unwrap_or_else(|| panic!("not readable: {src}"));
        assert!(!shapes.is_empty(), "{src}");
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for s in &shapes {
            for p in &s.paths {
                for q in &p.points {
                    for pt in [q.anchor, q.left, q.right] {
                        x0 = x0.min(pt.x);
                        y0 = y0.min(pt.y);
                        x1 = x1.max(pt.x);
                        y1 = y1.max(pt.y);
                    }
                }
            }
        }
        // the shapes fill the frame (the drawing's view box is the frame) and stay inside it
        assert!(x0 >= frame.left - 0.5 && x1 <= frame.right + 0.5 && y0 >= frame.top - 0.5 && y1 <= frame.bottom + 0.5, "{src}: {x0} {y0} {x1} {y1} in {frame:?}");
        assert!(x1 - x0 > eq.width * 0.5 && y1 - y0 > eq.height * 0.4, "{src}");
    }
}

#[test]
fn the_colour_written_on_the_drawing_is_the_colour_drawn() {
    let eq = folio_math::layout("<math><mi>x</mi></math>", &folio_math::Options { size: 12.0, display: false, color: [17.0, 83.0, 149.0] }).unwrap();
    let frame = Rect { left: 0.0, top: 0.0, right: eq.width, bottom: eq.height };
    let shapes = folio_render::mathsvg::shapes(&object(&eq), &frame).unwrap();
    assert!(!shapes.is_empty() && shapes.iter().all(|s| s.fill == Some([17, 83, 149])), "{:?}", shapes.iter().map(|s| s.fill).collect::<Vec<_>>());
}
