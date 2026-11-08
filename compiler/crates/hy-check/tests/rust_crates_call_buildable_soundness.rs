//! Soundness and buildable validation tests for pinned Rust crates (`sha3`, `regex`, `serde_json`)
//! under GUARANTEES.md Row 13 (BUILDABLE):
//!
//! 1. Call arity violations on pinned Rust crate functions must reject fail-closed with `HYB-E1002`:
//!    - `sha3.digest256` (exact 1 arg: `data`)
//!    - `sha3.stream_checksum` (exact 2 args: `iters`, `seed`)
//!    - `regex.is_match` (exact 2 args: `pattern`, `text`)
//!    - `regex.is_valid` (exact 1 arg: `pattern`)
//!    - `serde_json.parse` (exact 1 arg: `data`)
//!    - `serde_json.new_object` (exact 0 args)
//!    - `serde_json.new_array` (exact 0 args)
//!    - `serde_json.to_string_pretty` (exact 1 arg: `v`)
//!
//! 2. Unpinned symbols in `regex` (`regex.compile`, `regex.match_string`) and package types
//!    (`serde_json.value`) must reject fail-closed with `HYB-E1002`.
//!
//! 3. Named arguments with unrecognized parameter names must reject with `HYB-E1002`.
//!
//! 4. Positive controls: valid calls with correct positional or named arguments accept cleanly.
//!
//! 5. Shadowing: local declarations named after packages preserve user semantics.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected Accept with 0 errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

// ---------------------------------------------------------------------------
// sha3 call arity and parameter validation
// ---------------------------------------------------------------------------

#[test]
fn test_sha3_digest256_arity_rejects() {
    rejects_e1002(r#"
import "rust/sha3"
fn main() {
    _ = sha3.digest256()?
}
"#);
    rejects_e1002(r#"
import "rust/sha3"
fn main() {
    _ = sha3.digest256("a", "b")?
}
"#);
    rejects_e1002(r#"
import "rust/sha3"
fn main() {
    _ = sha3.digest256(unknown_param="a")?
}
"#);
}

#[test]
fn test_sha3_stream_checksum_arity_rejects() {
    rejects_e1002(r#"
import "rust/sha3"
fn main() {
    _ = sha3.stream_checksum()?
}
"#);
    rejects_e1002(r#"
import "rust/sha3"
fn main() {
    _ = sha3.stream_checksum(100)?
}
"#);
    rejects_e1002(r#"
import "rust/sha3"
fn main() {
    _ = sha3.stream_checksum(100, 7, 99)?
}
"#);
    rejects_e1002(r#"
import "rust/sha3"
fn main() {
    _ = sha3.stream_checksum(100, bad=7)?
}
"#);
}

// ---------------------------------------------------------------------------
// regex call arity, unpinned symbols, and parameter validation
// ---------------------------------------------------------------------------

#[test]
fn test_regex_is_match_arity_rejects() {
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    _ = regex.is_match()?
}
"#);
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    _ = regex.is_match("a")?
}
"#);
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    _ = regex.is_match("a", "b", "c")?
}
"#);
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    _ = regex.is_match("a", bad="b")?
}
"#);
}

#[test]
fn test_regex_is_valid_arity_rejects() {
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    _ = regex.is_valid()
}
"#);
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    _ = regex.is_valid("a", "b")
}
"#);
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    _ = regex.is_valid(bad="a")
}
"#);
}

#[test]
fn test_regex_unpinned_symbols_reject() {
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    r, err := regex.compile("^[a-z]+$")
    _ = r
    _ = err
}
"#);
    rejects_e1002(r#"
import "rust/regex"
fn main() {
    b, err := regex.match_string("^[a-z]+$", "abc")
    _ = b
    _ = err
}
"#);
}

// ---------------------------------------------------------------------------
// serde_json call arity, types, and parameter validation
// ---------------------------------------------------------------------------

#[test]
fn test_serde_json_value_type_call_rejects() {
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    _ = serde_json.value()
}
"#);
}

#[test]
fn test_serde_json_parse_arity_rejects() {
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    _ = serde_json.parse()?
}
"#);
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    _ = serde_json.parse("a", "b")?
}
"#);
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    _ = serde_json.parse(bad="a")?
}
"#);
}

#[test]
fn test_serde_json_constructors_zero_arity_reject() {
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    _ = serde_json.new_object(1)
}
"#);
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    _ = serde_json.new_array(1, 2)
}
"#);
}

#[test]
fn test_serde_json_to_string_pretty_arity_rejects() {
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    _ = serde_json.to_string_pretty()?
}
"#);
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    v := serde_json.new_object()
    _ = serde_json.to_string_pretty(v, "extra")?
}
"#);
    rejects_e1002(r#"
import "rust/serde_json"
fn main() {
    v := serde_json.new_object()
    _ = serde_json.to_string_pretty(bad=v)?
}
"#);
}

// ---------------------------------------------------------------------------
// Positive controls
// ---------------------------------------------------------------------------

#[test]
fn test_rust_crates_valid_calls_accept() {
    let src = r#"
import "rust/sha3"
import "rust/regex"
import "rust/serde_json"

fn test_sha3() error {
    d1 := sha3.digest256("hybrid")?
    d2 := sha3.digest256(data="hybrid")?
    c1 := sha3.stream_checksum(1000, 7)?
    c2 := sha3.stream_checksum(1000, seed=7)?
    c3 := sha3.stream_checksum(iters=1000, seed=7)?
    _ = d1
    _ = d2
    _ = c1
    _ = c2
    _ = c3
    return nil
}

fn test_regex() error {
    m1 := regex.is_match("^[a-z]+$", "hybrid")?
    m2 := regex.is_match(pattern="^[a-z]+$", text="hybrid")?
    v1 := regex.is_valid("^[a-z]+$")
    v2 := regex.is_valid(pattern="^[a-z]+$")
    _ = m1
    _ = m2
    _ = v1
    _ = v2
    return nil
}

fn test_serde_json() error {
    obj := serde_json.new_object()
    arr := serde_json.new_array()
    p1 := serde_json.parse(`{"a": 1}`)?
    p2 := serde_json.parse(data=`{"b": 2}`)?
    s1 := serde_json.to_string_pretty(obj)?
    s2 := serde_json.to_string_pretty(v=arr)?
    _ = p1
    _ = p2
    _ = s1
    _ = s2
    return nil
}

fn main() {
    _ = test_sha3()
    _ = test_regex()
    _ = test_serde_json()
}
"#;
    accepts(src);
}

#[test]
fn test_rust_crates_shadowed_package_preserves_user_semantics() {
    let src = r#"
struct my_sha3 {
    val int
}

fn (s my_sha3) digest256() int {
    return s.val
}

fn main() {
    sha3 := my_sha3{val: 42}
    _ = sha3.digest256()
}
"#;
    accepts(src);
}
