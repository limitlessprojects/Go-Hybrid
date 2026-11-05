//! Regression tests (accept controls) for #2422: constant string conversions,
//! valid numeric `complex` / `real` / `imag` initializers, and run-time `string(int)` accept.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", res.diagnostics);
}

#[test]
fn const_string_conversions_and_runtime_string_int_accept() {
    assert_accept(
        "type my_str = string\nconst base = \"ok\"\nconst s1 = string(\"ok\")\nconst s2 = my_str(base + \"!\")\nconst n1 = len(string(\"ok\"))\nfn main() {\n    rt := string(65)\n    _ = rt\n    _ = s1\n    _ = s2\n    _ = n1\n}\n",
    );
}

#[test]
fn valid_complex_real_and_imag_constants_accept() {
    assert_accept(
        "const c1 = complex(1.5, 2.5)\nconst c2 = complex(1, -2)\nconst r1 = real(c1)\nconst i1 = imag(c2)\nconst r2 = real(42)\nfn main() {\n    _ = r1\n    _ = i1\n    _ = r2\n}\n",
    );
}
