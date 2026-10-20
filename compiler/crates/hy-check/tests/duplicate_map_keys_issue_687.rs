//! duplicate_map_keys_issue_687.rs — regression for issue #687:
//! duplicate constant map-literal keys pass hyb check with 0 diagnostics,
//! but emitted Go fails with `duplicate key X in map literal`.
//!
//! Go spec, Composite literals: "It is an error to specify multiple elements
//! with the same field name or constant key value."
//!
//! Hyb must reject duplicate constant map-literal keys fail-closed with
//! `HYB-E1002` (rule `BUILDABLE`, GUARANTEES row 13).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(r: &hy_check::CheckResult, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
    let diag = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002" && d.message.contains("duplicate key"))
        .unwrap_or_else(|| panic!("expected duplicate key message for {}, got {:?}", what, r.diagnostics));
    assert!(diag.message.contains("duplicate key"), "expected message to contain 'duplicate key'");
}

fn assert_accepts(r: &hy_check::CheckResult, what: &str) {
    let errors: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "expected accept for {}, got errors: {:?}",
        what,
        errors
    );
    assert_eq!(r.verdict, Verdict::Accept, "{}", what);
}

// ---------------------------------------------------------------------------
// 1. Integer constant keys
// ---------------------------------------------------------------------------

#[test]
fn reject_duplicate_int_literal() {
    let src = r#"
fn main() {
    m := map[int]string{1: "a", 1: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate int literal 1");
}

#[test]
fn reject_duplicate_folded_int() {
    let src = r#"
fn main() {
    m := map[int]int{1 + 1: 0, 2: 0}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate folded int 1+1 and 2");
}

#[test]
fn reject_duplicate_int_with_arithmetic_ops() {
    let src = r#"
fn main() {
    m := map[int]string{2 * 3: "a", 10 - 4: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate int folded ops 2*3 and 10-4");
}

#[test]
fn reject_duplicate_negative_int() {
    let src = r#"
fn main() {
    m := map[int]string{-5: "a", -5: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate negative int -5");
}

#[test]
fn reject_duplicate_hex_and_dec_int() {
    let src = r#"
fn main() {
    m := map[int]string{0x10: "a", 16: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate hex 0x10 and dec 16");
}

// ---------------------------------------------------------------------------
// 2. String constant keys
// ---------------------------------------------------------------------------

#[test]
fn reject_duplicate_string_literal() {
    let src = r#"
fn main() {
    m := map[string]int{"a": 1, "a": 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate string literal \"a\"");
}

#[test]
fn reject_duplicate_folded_string() {
    let src = r#"
fn main() {
    m := map[string]int{"a" + "b": 1, "ab": 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate folded string \"a\"+\"b\" and \"ab\"");
}

#[test]
fn reject_duplicate_raw_and_standard_string() {
    let src = r#"
fn main() {
    m := map[string]int{`hello`: 1, "hello": 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate raw string and standard string");
}

// ---------------------------------------------------------------------------
// 3. File-level and local const identifiers
// ---------------------------------------------------------------------------

#[test]
fn reject_duplicate_file_const_and_literal() {
    let src = r#"
const k = 1

fn main() {
    m := map[int]string{k: "a", 1: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate file const k and literal 1");
}

#[test]
fn reject_duplicate_file_const_reverse_order() {
    let src = r#"
const k = 1

fn main() {
    m := map[int]string{1: "a", k: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate literal 1 and file const k");
}

#[test]
fn reject_duplicate_local_const() {
    let src = r#"
fn main() {
    const k = 42
    m := map[int]string{k: "a", 42: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate local const k and literal 42");
}

#[test]
fn reject_duplicate_chained_consts() {
    let src = r#"
const a = "prefix"
const b = a

fn main() {
    m := map[string]int{b: 1, "prefix": 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate chained const b and literal \"prefix\"");
}

// ---------------------------------------------------------------------------
// 4. Boolean constant keys
// ---------------------------------------------------------------------------

#[test]
fn reject_duplicate_bool_literal() {
    let src = r#"
fn main() {
    m := map[bool]int{true: 1, true: 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate bool literal true");
}

#[test]
fn reject_duplicate_folded_bool() {
    let src = r#"
fn main() {
    m := map[bool]int{!false: 1, true: 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate folded bool !false and true");
}

// ---------------------------------------------------------------------------
// 5. Float and Rune constant keys
// ---------------------------------------------------------------------------

#[test]
fn reject_duplicate_float_literal() {
    let src = r#"
fn main() {
    m := map[float64]int{1.5: 1, 1.5: 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate float literal 1.5");
}

#[test]
fn reject_duplicate_rune_literal() {
    let src = r#"
fn main() {
    m := map[rune]int{'a': 1, 'a': 2}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate rune literal 'a'");
}

// ---------------------------------------------------------------------------
// 6. Closures, Nested Maps, and File-Level Const Maps
// ---------------------------------------------------------------------------

#[test]
fn reject_duplicate_in_closure() {
    let src = r#"
fn main() {
    f := fn() {
        m := map[int]string{10: "x", 10: "y"}
        fmt.println("{len(m)}")
    }
    f()
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate in closure body");
}

#[test]
fn reject_duplicate_in_nested_map() {
    let src = r#"
fn main() {
    m := map[string]map[int]string{
        "outer": map[int]string{1: "x", 1: "y"},
    }
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate in nested map literal");
}

#[test]
fn reject_duplicate_in_file_level_const_map() {
    let src = r#"
const m = map[int]string{1: "a", 1: "b"}

fn main() {
    fmt.println("ok")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "duplicate in file-level const map");
}

#[test]
fn reject_multiple_duplicate_entries() {
    let src = r#"
fn main() {
    m := map[int]string{1: "a", 2: "b", 1: "c", 2: "d"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "multiple duplicate entries 1 and 2");
    let count = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002" && d.message.contains("duplicate key"))
        .count();
    assert_eq!(count, 2, "expected 2 duplicate key diagnostics");
}

// ---------------------------------------------------------------------------
// 7. Controls that MUST be accepted
// ---------------------------------------------------------------------------

#[test]
fn accept_distinct_int_keys() {
    let src = r#"
fn main() {
    m := map[int]string{1: "a", 2: "b", 3: "c"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "distinct int keys");
}

#[test]
fn accept_distinct_string_keys() {
    let src = r#"
fn main() {
    m := map[string]int{"a": 1, "b": 2, "c": 3}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "distinct string keys");
}

#[test]
fn accept_distinct_folded_keys() {
    let src = r#"
fn main() {
    m := map[int]int{1 + 1: 0, 1 + 2: 0}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "distinct folded keys 2 and 3");
}

#[test]
fn accept_runtime_variables() {
    let src = r#"
fn main() {
    x := 1
    y := 1
    m := map[int]string{x: "a", y: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "runtime variables as keys");
}

#[test]
fn accept_runtime_variable_and_constant() {
    let src = r#"
fn main() {
    x := 1
    m := map[int]string{x: "a", 1: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "runtime variable and constant key");
}

#[test]
fn accept_shadowed_const_by_runtime_variable() {
    let src = r#"
const k = 1

fn main() {
    k := 2
    m := map[int]string{k: "a", 1: "b"}
    fmt.println("{len(m)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "shadowed file const by runtime variable");
}

#[test]
fn accept_empty_and_single_entry_maps() {
    let src = r#"
fn main() {
    m0 := map[string]int{}
    m1 := map[string]int{"k": 1}
    fmt.println("{len(m0)} {len(m1)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "empty and single-entry maps");
}

#[test]
fn accept_separate_maps_with_same_keys() {
    let src = r#"
fn main() {
    m1 := map[int]string{1: "a", 2: "b"}
    m2 := map[int]string{1: "c", 2: "d"}
    fmt.println("{len(m1)} {len(m2)}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "separate maps with same keys");
}
