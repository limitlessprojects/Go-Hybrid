//! Regression (#3990): `foreign_method_surface` must reject phantom methods
//! on `sort.string_slice`, `sort.int_slice`, and `sort.float64_slice` with
//! `HYB-E1002` while accepting all real Go `sort.*Slice` methods.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn phantom_sort_slice_methods_rejected_e1002() {
    for (field_decl, recv_expr, call) in [
        ("s sort.string_slice", "h.s", "h.s.totally_fake()"),
        ("i sort.int_slice", "h.i", "h.i.bogus()"),
        ("f sort.float64_slice", "h.f", "h.f.fake_call()"),
    ] {
        let src = format!(
            "import \"sort\"\n\
             struct holder {{\n\
                 {field_decl}\n\
             }}\n\
             fn main() {{\n\
                 var h holder\n\
                 {call}\n\
                 _ = {recv_expr}\n\
             }}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "{call}: {:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
            "{call}: expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn real_sort_slice_methods_accepted() {
    let r = check(
        "import \"sort\"\n\
         import \"fmt\"\n\
         struct holder {\n\
             s sort.string_slice\n\
             i sort.int_slice\n\
             f sort.float64_slice\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.s.sort()\n\
             h.i.swap(0, 1)\n\
             _ = h.f.len()\n\
             _ = h.f.less(0, 1)\n\
             fmt.println(h.s.search(\"a\"), h.i.search(1), h.f.search(1.0))\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
