//! shared_return_pairs_798_a.rs — regression tests for transitive multi-return
//! shared pairs and local closure return origin propagation (#798).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

#[test]
fn transitive_and_closure_shared_return_pairs_issue_798() {
    let r = check(
        "fn worker(s []int) {\n\
           s[0] = 1\n\
         }\n\
         fn make_shared() ([]int, []int) {\n\
           s := []int{0}\n\
           return s, s\n\
         }\n\
         fn forward_shared() ([]int, []int) {\n\
           x, y := make_shared()\n\
           return x, y\n\
         }\n\
         fn direct_forward_shared() ([]int, []int) {\n\
           return make_shared()\n\
         }\n\
         fn closure_shared() ([]int, []int) {\n\
           s := []int{0}\n\
           get := fn() []int {\n\
             return s\n\
           }\n\
           return get(), s\n\
         }\n\
         fn main() {\n\
           a1, b1 := forward_shared()\n\
           task worker(move a1)\n\
           b1[0] = 2\n\
           a1d, b1d := direct_forward_shared()\n\
           task worker(move a1d)\n\
           b1d[0] = 2\n\
           a2, b2 := closure_shared()\n\
           task worker(move a2)\n\
           b2[0] = 3\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(
        r.diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3003")
            .count(),
        3,
        "expected 3 HYB-E3003 diagnostics for transitive and closure shared return pairs, got: {:?}",
        r.diagnostics
    );
}
