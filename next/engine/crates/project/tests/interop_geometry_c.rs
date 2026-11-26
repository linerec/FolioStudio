#[path = "support/interop.rs"]
mod support;
use folio_model::{Affine, PathPoint, Point, SubPath};
use serde_json::json;
use support::{attr, by_id, fixture, roundtrip};

#[test]
fn complete_path_points_subpaths_and_affine_coefficients_survive_export() {
    const PART: &str = "Spreads/Spread_uf3.xml";
    for offset in [-7.5, 13.25] {
        let point = |x, y| Point {
            x: x + offset,
            y: y - offset,
        };
        let points = vec![
            PathPoint {
                anchor: point(1.0, 2.0),
                left: point(3.0, 4.0),
                right: point(5.0, 6.0),
            },
            PathPoint {
                anchor: point(11.0, 12.0),
                left: point(13.0, 14.0),
                right: point(15.0, 16.0),
            },
        ];
        let expected = vec![
            SubPath {
                points: points.clone(),
                open: false,
            },
            SubPath {
                points: vec![],
                open: true,
            },
        ];
        let transform = Affine::new(0.0, 2.0, -3.0, 0.5, offset, -offset);
        let mut parts = fixture("basic");
        let xml = parts.get_mut(PART).unwrap();
        let start = xml.find("<Rectangle Self=\"uff\"").unwrap();
        let end = start + xml[start..].find("</Rectangle>").unwrap() + "</Rectangle>".len();
        let mut item = xml[start..end].to_owned();
        attr(
            &mut item,
            "Rectangle",
            0,
            "ItemTransform",
            &format!("0 2 -3 0.5 {offset} {}", -offset),
        );
        let start_path = item.find("<PathGeometry>").unwrap();
        let end_path = start_path
            + item[start_path..].find("</PathGeometry>").unwrap()
            + "</PathGeometry>".len();
        let mut path =
            String::from("<PathGeometry><GeometryPathType PathOpen=\"false\"><PathPointArray>");
        for p in &points {
            path.push_str(&format!("<PathPointType Anchor=\"{} {}\" LeftDirection=\"{} {}\" RightDirection=\"{} {}\"/>",
                p.anchor.x, p.anchor.y, p.left.x, p.left.y, p.right.x, p.right.y));
        }
        path.push_str("</PathPointArray></GeometryPathType><GeometryPathType PathOpen=\"true\"><PathPointArray/></GeometryPathType></PathGeometry>");
        item.replace_range(start_path..end_path, &path);
        xml.replace_range(start..end, &item);
        let (before, after, written) = roundtrip("basic", &parts);
        for doc in [&before, &after] {
            assert_eq!(by_id(doc, "uff")["paths"], json!(expected));
            assert_eq!(by_id(doc, "uff")["transform"], json!(transform));
        }
        assert_eq!(written[PART], parts[PART]);
    }
}

#[test]
fn stroke_art_is_recomputed_from_idml_stroke_settings_and_not_a_stored_part() {
    let part = "Spreads/Spread_uf3.xml";
    for weight in [6.0, 12.0] {
        let mut parts = fixture("basic");
        let xml = parts.get_mut(part).unwrap();
        for (key, value) in [
            ("StrokeType", "StrokeStyle/$ID/Dashed"),
            ("StrokeWeight", &weight.to_string()),
            ("StrokeColor", "Color/Black"),
            ("GapColor", "Color/Oracle Blue"),
            ("StrokeCornerAdjustment", "None"),
            ("EndCap", "RoundEndCap"),
            ("EndJoin", "BevelEndJoin"),
            ("MiterLimit", "3"),
            ("LeftLineEnd", "TriangleArrowHead"),
        ] {
            attr(xml, "GraphicLine", 0, key, value);
        }
        let at = xml.find("<GraphicLine Self=\"").unwrap() + "<GraphicLine Self=\"".len();
        let id = xml[at..at + xml[at..].find('"').unwrap()].to_owned();
        let (before, after, written) = roundtrip("basic", &parts);
        let art = &by_id(&before, &id)["stroke"]["art"];
        assert_eq!(art, &by_id(&after, &id)["stroke"]["art"]);
        let pieces = art["parts"].as_array().unwrap();
        assert!(pieces.iter().any(|p| p["ink"] == "Gap"));
        assert!(
            pieces
                .iter()
                .any(|p| p["ink"] == "Stroke" && p["shape"] == "Fill")
        );
        let line = pieces
            .iter()
            .find_map(|p| p["shape"].get("Line").filter(|l| !l["dash"].is_null()))
            .unwrap();
        assert_eq!(line["width"], json!(weight));
        assert_eq!(line["cap"], "Round");
        assert_eq!(line["join"], "Bevel");
        assert_eq!(line["miter_limit"], json!(3.0));
        assert_eq!(line["dash"]["array"], json!([12.0, 12.0]));
        assert_eq!(line["dash"]["phase"], json!(0.0));
        assert!(
            pieces
                .iter()
                .all(|p| !p["paths"].as_array().unwrap().is_empty())
        );
        // The exported original XML contains only the source stroke settings and paths.
        assert_eq!(written[part], parts[part]);
        assert!(!written.values().any(|xml| xml.contains("StrokeArt")
            || xml.contains("StrokePart")
            || xml.contains("PartShape")));
        let xml = parts.get_mut(part).unwrap();
        attr(xml, "GraphicLine", 0, "StrokeType", "StrokeStyle/$ID/Solid");
        attr(xml, "GraphicLine", 0, "LeftLineEnd", "None");
        let (plain, plain_after, _) = roundtrip("basic", &parts);
        assert!(by_id(&plain, &id)["stroke"]["art"].is_null());
        assert!(by_id(&plain_after, &id)["stroke"]["art"].is_null());
    }
}
