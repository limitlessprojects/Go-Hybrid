use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    let errs = error_codes(&r);
    assert!(
        errs.is_empty(),
        "expected clean accept, got {:?}\nsrc: {}",
        errs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn const_nil_error_is_rejected_with_e6003() {
    let r_top = check(
        r#"package main

const e error = nil

fn main() {}
"#,
    );
    assert_has(&r_top, "HYB-E6003");

    let r_local = check(
        r#"package main

fn main() {
    const e error = nil
    _ = e
}
"#,
    );
    assert_has(&r_local, "HYB-E6003");
}

#[test]
fn const_spec_float_and_char_conversions_are_accepted() {
    assert_accepts(
        r#"package main

type my_char = char

const a = f64(42)
const b = f32(1.5)
const c = my_char(65)

fn main() {
    _ = a
    _ = b
    _ = c
}
"#,
    );
}

#[test]
fn const_len_on_non_string_typed_file_const_is_rejected_with_e6003() {
    let r = check(
        r#"package main

const s any = "hello"
const n = len(s)

fn main() {
    _ = n
}
"#,
    );
    assert_has(&r, "HYB-E6003");
}

#[test]
fn const_len_string_alias_and_shadowed_string_type_handled() {
    assert_accepts(
        r#"package main

type my_str = string

const n = len(my_str("hello"))

fn main() {
    _ = n
}
"#,
    );

    let r_shadow = check(
        r#"package main

type string = int

const n = len(string(65))

fn main() {
    _ = n
}
"#,
    );
    assert_has(&r_shadow, "HYB-E6003");
}
