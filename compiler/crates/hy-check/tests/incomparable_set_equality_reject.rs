//! Regression tests (reject) for #2421: prelude `set[T]` and `option[set[T]]`
//! / composite carriers of `set[T]` are incomparable (`HYB-E2006`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn assert_e2006(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let codes: Vec<String> = check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect();
    assert!(codes.iter().any(|c| c == "HYB-E2006"), "expected HYB-E2006, got {codes:?} for:\n{src}");
}

#[test]
fn direct_set_eq_and_ne_reject() {
    assert_e2006("fn main() {\n    a := set[int]{}\n    b := set[int]{}\n    _ = a == b\n}\n");
    assert_e2006("fn main() {\n    a := set[string]{}\n    b := set[string]{}\n    _ = a != b\n}\n");
}

#[test]
fn option_array_and_struct_embedding_set_reject() {
    assert_e2006("fn main() {\n    a := set[int]{}\n    b := set[int]{}\n    _ = some(a) == some(b)\n}\n");
    assert_e2006("fn main() {\n    a := set[int]{}\n    _ = some(a) == none\n}\n");
    assert_e2006("fn cmp(x option[set[int]], y option[set[int]]) bool {\n    return x == y\n}\nfn main() {}\n");
    assert_e2006("fn main() {\n    a := set[int]{}\n    b := set[int]{}\n    _ = [1]set[int]{a} == [1]set[int]{b}\n}\n");
    assert_e2006("struct holder { s set[int] }\nfn main() {\n    a := set[int]{}\n    b := set[int]{}\n    _ = holder{s: a} == holder{s: b}\n}\n");
}
