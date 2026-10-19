//! scope_pop_shadow_797_a.rs — regression tests for `scope_pop` preserving
//! sharing when the source or destination peer is shadowed inside a block (#797).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

#[test]
fn scope_pop_preserves_shadowed_source_or_dest_peer_issue_797() {
    let r = check(
        "fn worker(s []int) {\n\
           s[0] = 1\n\
         }\n\
         fn race_shadow_source() {\n\
           s1 := []int{0}\n\
           var s2 []int\n\
           if true {\n\
             s2 = s1\n\
             s1 := 0\n\
             _ = s1\n\
           }\n\
           task worker(move s2)\n\
           s1[0] = 2\n\
         }\n\
         fn race_shadow_dest() {\n\
           s1 := []int{0}\n\
           var s2 []int\n\
           if true {\n\
             s2 = s1\n\
             s2 := 0\n\
             _ = s2\n\
           }\n\
           task worker(move s2)\n\
           s1[0] = 2\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3003")
            .count()
            >= 2,
        "expected HYB-E3003 on shadowed source/dest peers after scope_pop, got: {:?}",
        r.diagnostics
    );
}
