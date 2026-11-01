//! Regression tests (reject) for #2420: `collect_addr_of_roots` must preserve
//! `&x` provenance through composite projections, `some`/`append`, and IIFEs (`HYB-E3003`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn assert_e3003_a1b(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A1b")),
        "expected HYB-E3003 (A1b), got {:?} for:\n{src}",
        res.diagnostics
    );
}

#[test]
fn composite_index_slice_and_field_projections_preserve_addr_of_roots() {
    assert_e3003_a1b(
        "struct box_val { n int }\nfn bg(ps []*box_val) { ps[0].n = 99 }\nfn store(dst []*box_val, src *box_val) { dst[0] = src }\nfn main() {\n    w := box_val{n: 1}\n    slot := make([]*box_val, 1)\n    store(slot, ([]*box_val{&w})[0])\n    task bg(move slot)\n    w.n = 2\n}\n",
    );
    assert_e3003_a1b(
        "struct box_val { n int }\nfn bg(pss [][]*box_val) { pss[0][0].n = 99 }\nfn store_all(dst [][]*box_val, src []*box_val) { dst[0] = src }\nfn main() {\n    w := box_val{n: 1}\n    slot := make([][]*box_val, 1)\n    store_all(slot, ([]*box_val{&w})[:])\n    task bg(move slot)\n    w.n = 2\n}\n",
    );
    assert_e3003_a1b(
        "struct box_val { n int }\nstruct holder { p *box_val }\nfn bg(ps []*box_val) { ps[0].n = 99 }\nfn store(dst []*box_val, src *box_val) { dst[0] = src }\nfn main() {\n    w := box_val{n: 1}\n    slot := make([]*box_val, 1)\n    store(slot, (holder{p: &w}).p)\n    task bg(move slot)\n    w.n = 2\n}\n",
    );
}

#[test]
fn some_append_and_iife_wrappers_preserve_addr_of_roots() {
    assert_e3003_a1b(
        "struct box_val { n int }\nfn store_opt(dst []option[*box_val], src option[*box_val]) { dst[0] = src }\nfn bg_opts(os []option[*box_val]) {\n    match os[0] {\n        some(v): { v.n = 99 }\n        none: {}\n    }\n}\nfn main() {\n    y := box_val{n: 1}\n    oslot := make([]option[*box_val], 1)\n    store_opt(oslot, some(&y))\n    task bg_opts(move oslot)\n    y.n = 2\n}\n",
    );
    assert_e3003_a1b(
        "struct box_val { n int }\nfn store_all(dst [][]*box_val, src []*box_val) { dst[0] = src }\nfn bg_nested(pss [][]*box_val) { pss[0][0].n = 99 }\nfn main() {\n    z := box_val{n: 1}\n    nslot := make([][]*box_val, 1)\n    store_all(nslot, append([]*box_val{}, &z))\n    task bg_nested(move nslot)\n    z.n = 2\n}\n",
    );
    assert_e3003_a1b(
        "struct box_val { n int }\nfn bg(ps []*box_val) { ps[0].n = 99 }\nfn store(dst []*box_val, src *box_val) { dst[0] = src }\nfn main() {\n    w := box_val{n: 1}\n    slot := make([]*box_val, 1)\n    store(slot, (fn() *box_val { return &w })())\n    task bg(move slot)\n    w.n = 2\n}\n",
    );
}
