use hy_check::check_file;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("test.hy", src).unwrap())
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E5001"),
        "expected HYB-E5001 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, hy_check::Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, hy_check::Verdict::Accept, "verdict for {:?}", src);
}

// ---------------------------------------------------------------------------
// 1. Rejection of `some`/`none` patterns on non-option subjects (HYB-E5001)
// ---------------------------------------------------------------------------

#[test]
fn match_int_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match 5 {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_int_var_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    x := 5
    match x {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_string_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match "hi" {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_string_var_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    s := "hello"
    match s {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_bool_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match true {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_float_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match 3.14 {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_rune_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match 'a' {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_slice_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match [1, 2] {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_struct_var_some_none_rejects_e5001_3613() {
    let src = r#"
struct item {
    val int
}

fn main() {
    it := item{val: 1}
    match it {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_map_var_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    m := map[string]int{"a": 1}
    match m {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_fn_call_int_some_none_rejects_e5001_3613() {
    let src = r#"
fn get_num() int {
    return 42
}

fn main() {
    match get_num() {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_neg_int_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match -5 {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

#[test]
fn match_parenthesized_int_lit_some_none_rejects_e5001_3613() {
    let src = r#"
fn main() {
    match (42) {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    rejects(src);
}

// ---------------------------------------------------------------------------
// 2. Legal option matches must stay accepted (no false positives)
// ---------------------------------------------------------------------------

#[test]
fn match_option_var_some_none_accepted_3613() {
    let src = r#"
fn main() {
    o := some(2)
    match o {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    accepts(src);
}

#[test]
fn match_some_call_accepted_3613() {
    let src = r#"
fn main() {
    match some(7) {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    accepts(src);
}

#[test]
fn match_option_struct_field_accepted_3613() {
    let src = r#"
struct container {
    v option[int]
}

fn main() {
    b := container{v: some(9)}
    match b.v {
        some(v): { _ = v }
        none: { _ = 1 }
    }
}
"#;
    accepts(src);
}

#[test]
fn match_option_with_guard_and_wildcard_accepted_3613() {
    let src = r#"
fn main() {
    o := some(3)
    match o {
        some(v) if v > 0: { _ = v }
        some(w): { _ = w }
        none: { _ = 1 }
    }
}
"#;
    accepts(src);
}

#[test]
fn match_non_option_with_literals_and_wildcard_accepted_3613() {
    let src = r#"
fn main() {
    x := 5
    match x {
        1: { _ = 10 }
        2: { _ = 20 }
        _: { _ = 0 }
    }
}
"#;
    accepts(src);
}
