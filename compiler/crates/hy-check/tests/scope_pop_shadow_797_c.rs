//! scope_pop_shadow_797_c.rs — regression tests for `scope_pop` preserving
//! non-owned parameter and `const` origins across block shadowing (#797).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

#[test]
fn scope_pop_preserves_param_and_const_origins_across_shadow_issue_797() {
    let r = check(
        "const k = [][]int{[]int{0}}\n\
         fn link(dst [][]int, src []int) {\n\
           dst[0] = src\n\
         }\n\
         fn worker(s []int) {\n\
           s[0] = 1\n\
         }\n\
         fn race_shadow_param(p [][]int) {\n\
           s2 := []int{0}\n\
           if true {\n\
             p[0] = s2\n\
             s2 := 0\n\
             _ = s2\n\
           }\n\
           task worker(move s2)\n\
           p[0][0] = 3\n\
         }\n\
         fn mutate_const_via_shadow() {\n\
           s2 := [][]int{[]int{0}}\n\
           if true {\n\
             link(s2, k[0])\n\
             s2 := 0\n\
             _ = s2\n\
           }\n\
           s2[0][0] = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 on parameter-linked s2 after scope_pop, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 on const-linked s2 after scope_pop, got: {:?}",
        r.diagnostics
    );
}
