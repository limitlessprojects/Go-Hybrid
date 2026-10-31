//! Issue #1021 — a `match` subject (and a match-guard condition) that produces
//! no value must be rejected by `hyb check` with `HYB-E1002`.
//!
//! Void function calls (`fn noop() {}`) and 0-value builtins (`close`,
//! `delete`, `clear`) used where a value is required used to pass the checker
//! and then fail `go build` with `(no value) used as value`.

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

#[test]
fn void_fn_call_as_match_subject_rejects_e1002() {
    assert_code(
        "import \"fmt\"\n\nfn noop() {}\n\nfn main() {\n    match noop() {\n        1 => { fmt.println(\"one\") }\n        _ => { fmt.println(\"ok\") }\n    }\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn close_as_match_subject_rejects_e1002() {
    assert_code(
        "import \"fmt\"\n\nfn main() {\n    ch := make(chan int, 1)\n    match close(ch) {\n        1 => { fmt.println(\"one\") }\n        _ => { fmt.println(\"ok\") }\n    }\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn delete_as_match_subject_rejects_e1002() {
    assert_code(
        "import \"fmt\"\n\nfn main() {\n    m := map[string]int{\"a\": 1}\n    match delete(m, \"a\") {\n        1 => { fmt.println(\"one\") }\n        _ => { fmt.println(\"ok\") }\n    }\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn clear_as_match_subject_rejects_e1002() {
    assert_code(
        "import \"fmt\"\n\nfn main() {\n    xs := []int{1, 2}\n    match clear(xs) {\n        1 => { fmt.println(\"one\") }\n        _ => { fmt.println(\"ok\") }\n    }\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn void_match_guard_rejects_e1002() {
    assert_code(
        "import \"fmt\"\n\nfn noop() {}\n\nfn main() {\n    x := 1\n    match x {\n        1 if noop() => { fmt.println(\"one\") }\n        _ => { fmt.println(\"other\") }\n    }\n}\n",
        "HYB-E1002",
    );
}
