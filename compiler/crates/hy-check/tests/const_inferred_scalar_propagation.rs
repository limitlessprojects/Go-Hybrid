//! Regression tests for propagating numeric scalar widths and sized-array lengths
//! from package-level and local `const` bindings into function-body checks (`HYB-E1002`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    check_file(&parse_file("test.hy", src).expect("test program must parse"))
}

#[test]
fn fn_body_rejects_mismatched_top_level_and_local_typed_consts_e1002() {
    let cases = [
        "const a int32 = 1\nfn main() {\n    var x int64 = a\n    _ = x\n}\n",
        "const a = int32(1)\nfn main() {\n    var x int64 = a\n    _ = x\n}\n",
        "const a = int(1)\nfn main() {\n    var f float64 = a\n    _ = f\n}\n",
        "const n = len(\"abc\")\nfn main() {\n    var x int64 = n\n    _ = x\n}\n",
        "const arr [3]int = [3]int{1, 2, 3}\nfn main() {\n    var b [4]int = arr\n    _ = b\n}\n",
        "const pkg_arr = [3]int{1, 2, 3}\nfn main() {\n    var b [4]int = pkg_arr\n    _ = b\n}\n",
        "fn main() {\n    const a = int32(1)\n    var x int64 = a\n    _ = x\n}\n",
        "fn main() {\n    const a = int(1)\n    var f float64 = a\n    _ = f\n}\n",
        "fn main() {\n    const n = len(\"abc\")\n    var f float64 = n\n    _ = f\n}\n",
        "fn main() {\n    const loc_arr = [3]int{1, 2, 3}\n    var b [4]int = loc_arr\n    _ = b\n}\n",
        "fn main() {\n    const a int32 = 1\n    const b = a\n    var x int64 = b\n    _ = x\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
    }
}

#[test]
fn cyclic_top_level_consts_through_calls_and_ops_terminate_cleanly() {
    for src in [
        "const a = min(a, 1)\nfn main() {\n    _ = a\n}\n",
        "const a = max(b, 1)\nconst b = min(a, 1)\nfn main() {\n    _ = a + b\n}\n",
        "const a = a + a\nfn main() {\n    _ = a\n}\n",
        "const a = b\nconst b = a\nconst c [4]int = a\nfn main() {\n    _ = c\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    }
}

#[test]
fn untyped_and_matching_typed_consts_in_fn_body_accepted() {
    let r = check(
        "const u = 10\nconst a = int32(5)\nconst n = len(\"abc\")\nconst arr = [3]int{1, 2, 3}\n\
         fn main() {\n    const lu = 20\n    const la = int32(7)\n    const ln = len(\"xy\")\n\
         var x int64 = u + lu\n    var f float64 = u + lu\n    var y int32 = a + la\n\
         var z int = n + ln\n    var b [3]int = arr\n    fmt.println(x, f, y, z, len(b))\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got: {:?}", r.diagnostics);
}
