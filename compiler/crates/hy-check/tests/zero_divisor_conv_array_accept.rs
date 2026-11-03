//! Regression tests (accept controls) for #2426: runtime float `/ 0.0`, non-zero
//! arrays/strings, and `len(call_returning_zero_array())` remain accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", res.diagnostics);
}

#[test]
fn runtime_float_div_by_zero_and_nonzero_len_cap_accept() {
    assert_accept(
        "fn get_empty() [0]int {\n    var a [0]int\n    return a\n}\nfn main() {\n    var f float64 = 1.0\n    _ = f / 0.0\n    f2 := 1.5\n    _ = f2 / 0.0\n    x := 10\n    _ = x / len([2]int{1, 2})\n    _ = x / cap([2]int{1, 2})\n    _ = x / len(\"a\" + \"\")\n    _ = x / int(real(complex(2, 0)))\n    _ = x / len(get_empty())\n}\n",
    );
}
