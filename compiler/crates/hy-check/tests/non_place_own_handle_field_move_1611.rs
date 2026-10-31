#[test]
fn non_place_own_handle_field_move_rejects_e3004_issue_1611() {
    let p = r#"import "rust/serde_json"

struct holder {
    doc serde_json.value
}

struct wrap {
    h holder
}

fn consume_doc(d serde_json.value) {
    _ = d
}
"#;

    // Repro 1: move out of composite literal field expression rejects with HYB-E3004
    let repro_lit = format!(
        "{p}\nfn main() error {{\n    h := holder{{doc: serde_json.parse(`{{}}`)?}}\n    d1 := move (wrap{{h: h}}).h.doc\n    task consume_doc(move d1)\n    return nil\n}}\n"
    );
    let r1 = hy_check::check_file(&hy_syntax::parse_file("repro_lit.hy", &repro_lit).unwrap());
    assert_eq!(r1.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r1.diagnostics);
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3004"), "{:?}", r1.diagnostics);

    // Repro 2: move out of indexed slice element field expression rejects with HYB-E3004
    let repro_slice = format!(
        "{p}\nfn main() error {{\n    h := holder{{doc: serde_json.parse(`{{}}`)?}}\n    xs := []holder{{h}}\n    d1 := move xs[0].doc\n    task consume_doc(move d1)\n    return nil\n}}\n"
    );
    let r2 = hy_check::check_file(&hy_syntax::parse_file("repro_slice.hy", &repro_slice).unwrap());
    assert_eq!(r2.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r2.diagnostics);
    assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3004"), "{:?}", r2.diagnostics);

    // Repro 3: move out of map element field expression rejects with HYB-E3004
    let repro_map = format!(
        "{p}\nfn main() error {{\n    h := holder{{doc: serde_json.parse(`{{}}`)?}}\n    m := map[string]holder{{\"k\": h}}\n    d1 := move m[\"k\"].doc\n    task consume_doc(move d1)\n    return nil\n}}\n"
    );
    let r3 = hy_check::check_file(&hy_syntax::parse_file("repro_map.hy", &repro_map).unwrap());
    assert_eq!(r3.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r3.diagnostics);
    assert!(r3.diagnostics.iter().any(|d| d.code == "HYB-E3004"), "{:?}", r3.diagnostics);

    // Positive control: binding-rooted field chain `move w.h.doc` is legal
    let pos = format!(
        "{p}\nfn main() error {{\n    h := holder{{doc: serde_json.parse(`{{}}`)?}}\n    w := wrap{{h: h}}\n    d1 := move w.h.doc\n    task consume_doc(move d1)\n    return nil\n}}\n"
    );
    let r_pos = hy_check::check_file(&hy_syntax::parse_file("pos.hy", &pos).unwrap());
    assert_eq!(r_pos.verdict, hy_check::Verdict::Accept, "expected accept: {:?}", r_pos.diagnostics);
}
