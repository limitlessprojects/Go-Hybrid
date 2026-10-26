//! Issue #1021 — a match-arm literal pattern whose type conflicts with the
//! subject's resolved type must be rejected by `hyb check` with `HYB-E1002`.
//!
//! `match n { "abc" => ... }` on an `int`, or `match s { true => ... }` on a
//! `string`, used to pass the checker and then fail `go build` with
//! `invalid case in switch` (mismatched types). A literal pattern of the
//! subject's own type stays accepted.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn int_subject_string_pattern_rejects_e1002() {
    assert_code(
        "import \"fmt\"\n\nfn main() {\n    x := 42\n    match x {\n        \"hello\" => { fmt.println(\"str\") }\n        _ => { fmt.println(\"other\") }\n    }\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn string_subject_bool_pattern_rejects_e1002() {
    assert_code(
        "import \"fmt\"\n\nfn main() {\n    s := \"abc\"\n    match s {\n        true => { fmt.println(\"bool\") }\n        _ => { fmt.println(\"other\") }\n    }\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn compatible_int_literal_pattern_stays_accepted() {
    assert_ok(
        "import \"fmt\"\n\nfn main() {\n    x := 42\n    match x {\n        42 => { fmt.println(\"forty-two\") }\n        _ => { fmt.println(\"other\") }\n    }\n}\n",
    );
}
