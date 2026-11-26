use lopdf::{Document, Object, Stream, dictionary};

fn document(count: usize, width: i64, profile: &[u8]) -> Vec<u8> {
    let mut d = Document::with_version("1.7");
    let pages = d.new_object_id();
    let font = d.add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"});
    let resources = d.add_object(dictionary! {"Font"=>dictionary! {"F1"=>font}});
    let mut kids = Vec::new();
    for i in 0..count {
        let content = d.add_object(Stream::new(dictionary! {}, format!("BT /F1 12 Tf 10 10 Td ({width}-{i}) Tj ET").into_bytes()));
        kids.push(Object::Reference(d.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "Contents"=>content})));
    }
    d.objects.insert(pages, dictionary! {"Type"=>"Pages", "Kids"=>kids, "Count"=>count as i64, "MediaBox"=>vec![0.into(),0.into(),width.into(),100.into()], "Resources"=>resources}.into());
    let icc = d.add_object(Stream::new(dictionary! {"N"=>4}, profile.to_vec()));
    let intent = d.add_object(dictionary! {"DestOutputProfile"=>icc});
    let catalog = d.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages,"OutputIntents"=>vec![intent.into()]});
    d.trailer.set("Root", catalog);
    let mut out = Vec::new(); d.save_to(&mut out).unwrap(); out
}

#[test]
fn six_pages_keep_order_resources_and_inherited_boxes() {
    let inputs = [document(1, 101, b"same"), document(2, 202, b"same"), document(3, 303, b"same")];
    let (bytes, count) = folio_pdf::book::join(&inputs).unwrap();
    assert_eq!(count, 6);
    let d = Document::load_mem(&bytes).unwrap();
    for ((_, id), (width, index)) in d.get_pages().into_iter().zip([(101,0),(202,0),(202,1),(303,0),(303,1),(303,2)]) {
        assert!(String::from_utf8(d.get_page_content(id)).unwrap().contains(&format!("{width}-{index}")));
        let page = d.get_object(id).unwrap().as_dict().unwrap();
        let parent = d.get_object(page.get(b"Parent").unwrap().as_reference().unwrap()).unwrap().as_dict().unwrap();
        assert_eq!(parent.get(b"MediaBox").unwrap().as_array().unwrap()[2].as_i64().unwrap(), width);
        assert!(d.get_page_fonts(id).unwrap().contains_key(b"F1".as_slice()));
    }
}

#[test]
fn incompatible_profiles_and_empty_book_are_rejected() {
    assert!(folio_pdf::book::join(&[]).is_err());
    assert!(folio_pdf::book::join(&[document(1,100,b"a"),document(1,100,b"b")]).unwrap_err().contains("different output profiles"));
}
