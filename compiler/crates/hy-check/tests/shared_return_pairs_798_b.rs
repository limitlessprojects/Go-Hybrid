//! shared_return_pairs_798_b.rs — regression test for struct-embedded array
//! slice backing storage sharing across multi-returns (#798).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

#[test]
fn struct_embedded_array_multi_return_views_issue_798() {
    let r = check(
        "struct buf {\n\
           arr [4]int\n\
         }\n\
         fn worker(s []int) {\n\
           s[0] = 1\n\
         }\n\
         fn struct_array_views() ([]int, []int) {\n\
           var b buf\n\
           return b.arr[0:2], b.arr[0:2]\n\
         }\n\
         fn main() {\n\
           a3, b3 := struct_array_views()\n\
           task worker(move a3)\n\
           b3[0] = 4\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" && d.message.contains("write through a live alias")),
        "expected HYB-E3003 write-through-live-alias for struct-embedded array multi-return, got: {:?}",
        r.diagnostics
    );
}

