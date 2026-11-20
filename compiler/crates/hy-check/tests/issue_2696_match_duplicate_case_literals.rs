//! Regression test for Issue #2696:
//! Soundness: match statement permits duplicate case literals, emitting unbuildable Go (duplicate case in switch).
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must emit Go code that passes `go build`.

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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {cs:?} for:\n{src}"
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
fn test_duplicate_int_literal_rejects_e1002() {
    // 1. Identical integer literals
    let src1 = r#"
fn main() {
    x := 1
    match x {
        1 => { _ = 1 }
        1 => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src1);

    // 2. Hex and decimal same value (10 vs 0xa)
    let src2 = r#"
fn main() {
    x := 10
    match x {
        10 => { _ = 1 }
        0xa => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src2);

    // 3. Underscore formatting (1_000 vs 1000)
    let src3 = r#"
fn main() {
    x := 1000
    match x {
        1_000 => { _ = 1 }
        1000 => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src3);
}

#[test]
fn test_duplicate_string_literal_rejects_e1002() {
    // 1. Identical string literals
    let src1 = r#"
fn main() {
    s := "hello"
    match s {
        "hello" => { _ = 1 }
        "hello" => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src1);

    // 2. Raw string and cooked string
    let src2 = r#"
fn main() {
    s := "hello"
    match s {
        `hello` => { _ = 1 }
        "hello" => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src2);
}

#[test]
fn test_duplicate_bool_literal_rejects_e1002() {
    let src = r#"
fn main() {
    b := true
    match b {
        true => { _ = 1 }
        true => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src);
}

#[test]
fn test_duplicate_char_literal_rejects_e1002() {
    // 1. Identical char literals
    let src1 = r#"
fn main() {
    c := 'a'
    match c {
        'a' => { _ = 1 }
        'a' => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src1);

    // 2. Char literal and its ASCII int code ('a' and 97)
    let src2 = r#"
fn main() {
    c := 'a'
    match c {
        'a' => { _ = 1 }
        97 => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src2);
}

#[test]
fn test_duplicate_float_literal_rejects_e1002() {
    let src = r#"
fn main() {
    f := 1.5
    match f {
        1.5 => { _ = 1 }
        1.5 => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    assert_e1002(src);
}

#[test]
fn test_distinct_literals_accept() {
    let src = r#"
fn main() {
    x := 1
    match x {
        1 => { _ = 1 }
        2 => { _ = 2 }
        3 => { _ = 3 }
        _ => { _ = 4 }
    }

    s := "a"
    match s {
        "a" => { _ = 1 }
        "b" => { _ = 2 }
        "c" => { _ = 3 }
        _ => { _ = 4 }
    }
}
"#;
    assert_ok(src);
}

#[test]
fn test_separate_matches_same_literal_accept() {
    let src = r#"
fn main() {
    x := 1
    match x {
        1 => { _ = 1 }
        _ => { _ = 2 }
    }

    y := 2
    match y {
        1 => { _ = 1 }
        _ => { _ = 2 }
    }
}
"#;
    assert_ok(src);
}

#[test]
fn test_issue_2696_minimal_reproduction() {
    let src = r#"
fn main() {
    // 1. Duplicate integer literal pattern
    x := 1
    match x {
        1 => { _ = 1 }
        1 => { _ = 2 }
        _ => { _ = 3 }
    }

    // 2. Duplicate string literal pattern
    s := "hello"
    match s {
        "hello" => { _ = 1 }
        "hello" => { _ = 2 }
        _ => { _ = 3 }
    }
}
"#;
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    let diags = res.diagnostics;
    let e1002_count = diags.iter().filter(|d| d.code == "HYB-E1002").count();
    assert_eq!(e1002_count, 2, "expected exactly 2 HYB-E1002 diagnostics for the 2 duplicates, got {diags:?}");
}
