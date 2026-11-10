//! Regression tests (accept controls) for #2421: set length/membership,
//! comparable option/struct equality, and user-declared `struct set` stay accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", res.diagnostics);
}

#[test]
fn set_len_and_comparable_option_and_struct_equality_accept() {
    assert_accept(
        "struct pair {\n    a int\n    b string\n}\nfn main() {\n    s1 := set[int]{}\n    s2 := set[int]{}\n    _ = len(s1) == len(s2)\n    _ = some(1) == some(2)\n    _ = some(\"a\") == none\n    _ = pair{a: 1, b: \"x\"} == pair{a: 1, b: \"x\"}\n}\n",
    );
}

#[test]
fn user_declared_comparable_struct_named_set_accepts() {
    assert_accept(
        "struct set { x int }\nfn main() {\n    a := set{x: 1}\n    b := set{x: 2}\n    _ = a == b\n}\n",
    );
}
