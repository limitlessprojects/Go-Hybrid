//! foreign_multireturn_and_curated_shapes.rs — regression tests for:
//! 1. Multi-value unpacking (`v, err := ...`) of foreign package and handle
//!    method calls preserving `TypeShape` so loan tracking, post-move guards,
//!    freeze sharing checks, and sendability boundaries remain sound.
//! 2. Curated stdlib package roots (`url`, `bufio`, `json`, `hex`, `path`,
//!    `filepath`, `sha3`, `regex`) recognized without explicit `import` lines,
//!    and curated foreign types (`url.values` / `url.Values` as map headers,
//!    `json.raw_message` / `json.RawMessage` as byte slices, `ForeignOpaque`
//!    and unproven foreign struct fields in `shape_may_share_at`).
//! 3. Parenthesized package root calls `(serde_json).parse(s)?` in `call_shape`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn passes(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?}: {:?}",
        r.verdict,
        r.diagnostics
    );
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject({}), got Accept",
        code
    );
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == code),
        "expected diagnostic {}, got {:?}: {:?}",
        code,
        codes,
        r.diagnostics
    );
}

#[test]
fn multireturn_foreign_handle_unpack_tracks_borrow_loan_and_post_move() {
    // 2-LHS unpack of `serde_json.parse` and `doc.array` must preserve
    // `OwnHandle` / `BorrowHandle` shapes so mutating `doc` while `items` is
    // live is rejected with HYB-E3101.
    rejects_with(
        r#"
fn main() {
    doc, err := serde_json.parse("[]")
    _ = err
    items, err2 := doc.array("k")
    _ = err2
    doc.set("x", "y")
    _ = items.len()
}
"#,
        "HYB-E3101",
    );

    // Post-move use of a handle unpacked via `doc, err := serde_json.parse(...)`
    // must be rejected with HYB-E3003.
    rejects_with(
        r#"
fn consume(d serde_json.value) {
    _ = d
}

fn main() {
    doc, err := serde_json.parse("[]")
    _ = err
    task consume(move doc)
    _ = doc.string("k")
}
"#,
        "HYB-E3003",
    );
}

#[test]
fn parenthesized_package_root_preserves_handle_shape() {
    // `(serde_json).parse(s)?` must classify as `OwnHandle("serde_json.value")`
    // in `call_shape` just like `serde_json.parse(s)?`.
    rejects_with(
        r#"
fn main() error {
    doc := (serde_json).parse("[]")?
    items := doc.array("k")?
    doc.set("x", "y")
    _ = items.len()
    return nil
}
"#,
        "HYB-E3101",
    );
}

#[test]
fn curated_packages_recognized_without_explicit_import_and_shapes_enforced() {
    // Curated stdlib packages (`url`, `json`, `bufio`, `hex`, `path`,
    // `filepath`, `sha3`, `regex`) are recognized without an import statement.
    passes(
        r#"
fn main() error {
    s := url.query_escape("a b")
    h := hex.encode_to_string(s)
    p := path.join("a", h)
    fp := filepath.clean(p)
    ok := json.valid(fp)
    _ = ok
    return nil
}
"#,
    );

    // `url.parse_query` returns `url.Values` (`map[string][]string`), a
    // reference-sharing map header — spawning without `move` must be rejected
    // with HYB-E3001.
    rejects_with(
        r#"
fn worker(q url.values) {
    _ = q
}

fn main() error {
    q := url.parse_query("a=1")?
    task worker(q)
    return nil
}
"#,
        "HYB-E3001",
    );

    // `json.marshal` returns `[]byte` (`Slice(Primitive)`), so spawning without
    // `move` must be rejected with HYB-E3001.
    rejects_with(
        r#"
fn worker(b []byte) {
    _ = b
}

fn main() error {
    raw, err := json.marshal("hi")
    _ = err
    task worker(raw)
    return nil
}
"#,
        "HYB-E3001",
    );
}

#[test]
fn struct_wrapping_foreign_opaque_or_unproven_atomic_counts_as_sharing_in_freeze() {
    // A struct containing `ForeignOpaque` (`http.serve_mux`) or an unproven
    // dotted foreign type (`atomic.value`) may share underlying state, so
    // an alias copied before `const shared = move ...` is invalidated on freeze
    // and reading the alias is rejected with HYB-E3003.
    rejects_with(
        r#"
struct holder {
    mux http.serve_mux
}

fn main() {
    h := holder{mux: http.new_serve_mux()}
    alias := h
    const shared = move h
    _ = alias
    _ = shared
}
"#,
        "HYB-E3003",
    );

    rejects_with(
        r#"
struct box_val {
    v atomic.value
}

fn main() {
    b := box_val{}
    alias := b
    const shared = move b
    _ = alias
    _ = shared
}
"#,
        "HYB-E3003",
    );
}
