//! shared_return_pairs_798_c.rs — regression test for struct-embedded array
//! slice backing storage sharing across closure returns (#798).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

#[test]
fn struct_embedded_array_closure_return_view_issue_798() {
    let r = check(
        "struct buf {\n\
           arr [4]int\n\
         }\n\
         fn worker(s []int) {\n\
           s[0] = 1\n\
         }\n\
         fn main() {\n\
           var b4 buf\n\
           get_slice := fn() []int {\n\
             return b4.arr[:]\n\
           }\n\
           s4 := get_slice()\n\
           task worker(move s4)\n\
           b4.arr[0] = 5\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" && d.message.contains("write through a live alias")),
        "expected HYB-E3003 write-through-live-alias for struct-embedded array closure return, got: {:?}",
        r.diagnostics
    );
}
