//! Regression tests (accept controls) for #2420: read-only borrows, fresh heap
//! literals, and shadowed IIFE locals do not falsely freeze outer bindings.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", res.diagnostics);
}

#[test]
fn read_only_borrow_and_fresh_struct_addr_accept() {
    assert_accept(
        "struct box_val { n int }\nfn read_box(p *box_val) int { return p.n }\nfn bg(ps []*box_val) { ps[0].n = 99 }\nfn store(dst []*box_val, src *box_val) { dst[0] = src }\nfn main() {\n    w := box_val{n: 1}\n    _ = read_box(&w)\n    slot := make([]*box_val, 1)\n    store(slot, ([]*box_val{&box_val{n: 7}})[0])\n    task bg(move slot)\n    w.n = 2\n}\n",
    );
}

#[test]
fn iife_shadowed_local_addr_does_not_freeze_outer_binding() {
    assert_accept(
        "struct box_val { n int }\nfn read_box(p *box_val) int { return p.n }\nfn bg(pss [][]*box_val) { pss[0] = make([]*box_val, 1) }\nfn store_all(dst [][]*box_val, src []*box_val) { dst[0] = src }\nfn main() {\n    w := box_val{n: 1}\n    slot := make([][]*box_val, 1)\n    store_all(slot, (fn() []*box_val {\n        w := box_val{n: 5}\n        _ = read_box(&w)\n        return make([]*box_val, 1)\n    })())\n    task bg(move slot)\n    w.n = 2\n}\n",
    );
}
