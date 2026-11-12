//! Issue #4222 — package-scope redeclarations across SIBLING files.
//!
//! `hyb check` sees one file, but Go keeps ONE package block across every
//! sibling file of the package directory. A top-level declaration a sibling
//! `.hy` file — or a hand-written sibling `.go` file — already declares used
//! to accept in `hyb check` with 0 diagnostics and fail `go build` with
//! `X redeclared in this block` (GUARANTEES.md Row 13 BUILDABLE).
//!
//! `check_file_with_siblings` (the API `hyb check` feeds from its directory
//! scan) must reject every such hidden redeclaration with `HYB-E1003`,
//! while plain `check_file` (no sibling context — single-file checks and
//! the corpus harness) stays unchanged.

use std::collections::HashMap;

use hy_check::{check_file, check_file_with_siblings, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn run(src: &str, sibs: &[(&str, &str)]) -> CheckResult {
    let ast = parse_file("b.hy", src).expect("test snippet must parse");
    let mut names: HashMap<String, String> = HashMap::new();
    for (name, file) in sibs {
        names.insert(name.to_string(), file.to_string());
    }
    check_file_with_siblings(&ast, names)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects(src: &str, sibs: &[(&str, &str)]) {
    let r = run(src, sibs);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1003"),
        "expected HYB-E1003 for:\n{src}\ngot: {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
}

fn assert_accepts(src: &str, sibs: &[(&str, &str)]) {
    let r = run(src, sibs);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected Accept with 0 errors for:\n{src}\ngot: {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{src}");
}

const HELPER: &str = "package main\n\nfn helper(n int) int {\n    return n * 2\n}\n\nfn main() {\n    _ = helper(3)\n}\n";

// ---------------------------------------------------------------------------
// Issue reproduction A: a sibling hand-written `.go` file
// ---------------------------------------------------------------------------

#[test]
fn sibling_go_fn_declaration_rejects() {
    assert_rejects(HELPER, &[("helper", "main.go")]);
}

#[test]
fn sibling_hy_fn_declaration_rejects() {
    // Issue reproduction B: `a.hy` and `b.hy` both declare `fn helper`.
    assert_rejects(HELPER, &[("helper", "a.hy")]);
}

#[test]
fn rejection_names_the_declaring_sibling_file() {
    let r = run(HELPER, &[("helper", "main.go")]);
    let hit = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1003")
        .expect("rejection expected");
    assert!(
        hit.message.contains("main.go"),
        "message must name the sibling file: {}",
        hit.message
    );
    assert!(
        hit.message.contains("redeclared in this block"),
        "message must mirror Go's error text: {}",
        hit.message
    );
}

// ---------------------------------------------------------------------------
// Every package-scope kind: fn / const / struct / enum / type / emitted assoc
// ---------------------------------------------------------------------------

#[test]
fn sibling_const_collision_rejects() {
    let src = "package main\n\nconst helper = 4\n\nfn main() {\n    _ = helper\n}\n";
    assert_rejects(src, &[("helper", "main.go")]);
}

#[test]
fn sibling_struct_collision_rejects() {
    let src = "package main\n\nstruct helper {\n    n int\n}\n\nfn main() {\n    _ = helper{n: 1}\n}\n";
    assert_rejects(src, &[("helper", "main.go")]);
}

#[test]
fn sibling_enum_collision_rejects() {
    let src = "package main\n\nenum helper {\n    yes,\n    no,\n}\n\nfn main() {\n}\n";
    assert_rejects(src, &[("helper", "a.hy")]);
}

#[test]
fn sibling_type_alias_collision_rejects() {
    let src = "package main\n\ntype helper = int\n\nfn main() {\n    _ = 0\n}\n";
    assert_rejects(src, &[("helper", "main.go")]);
}

#[test]
fn sibling_emitted_associated_name_rejects() {
    // `fn counter.make` emits the package-level Go identifier
    // `counter_make` — a sibling spelling it directly must collide.
    let src = "package main\n\nstruct counter {\n    n int\n}\n\nfn counter.make() counter {\n    return counter{n: 1}\n}\n";
    assert_rejects(src, &[("counter_make", "p.go")]);
}

// ---------------------------------------------------------------------------
// Fail-open surfaces: methods, unused sibling names, single-file contract
// ---------------------------------------------------------------------------

#[test]
fn method_never_collides_with_sibling_package_name() {
    // Methods live in the receiver's method set, not package scope.
    let src = "package main\n\nstruct box {\n    n int\n}\n\nfn (b box) helper() int {\n    return b.n\n}\n\nfn main() {\n    _ = 0\n}\n";
    assert_accepts(src, &[("helper", "main.go")]);
}

#[test]
fn unused_sibling_names_stay_accepted() {
    assert_accepts(HELPER, &[("other_name", "a.hy"),("third", "b.go")]);
}

#[test]
fn plain_check_file_contract_unchanged() {
    // No sibling context: check_file must never reject for sibling reasons,
    // so the corpus harness and single-file callers keep bit-identical
    // behavior.
    let ast = parse_file("a.hy", HELPER).expect("must parse");
    let r = check_file(&ast);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}

#[test]
fn intra_file_duplicate_message_unchanged() {
    // The original issue #201-C message must survive the sibling arm.
    let src = "package main\n\nfn helper(n int) int {\n    return n\n}\n\nfn helper(n int) int {\n    return n + 1\n}\n";
    let r = run(src, &[]);
    assert_eq!(r.verdict, Verdict::Reject);
    let dups: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1003")
        .collect();
    assert!(
        dups.iter()
            .any(|d| d.message.contains("first declared as a")),
        "intra-file duplicate wording changed: {:?}",
        dups.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}
