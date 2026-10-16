//! unbuildable_declaration.rs — regression for issue #134: `hyb check`
//! used to accept three declaration shapes whose emitted Go can never build:
//!
//!   1. an unused `:=` binding — Go: `declared and not used` (including
//!      `_ := 5` / `_, _ := ...`, Go: `no new variables on left side of :=`);
//!   2. an unused import — Go: `imported and not used`;
//!   3. a repeated name on the left of one `:=` — Go: `x repeated on left
//!      side of :=`.
//!
//! All three now reject fail-closed as HYB-E1002 (rule `BUILDABLE`). The
//! read-set follows Go's own unused-variable check: a mention anywhere in
//! the function counts (closure bodies included; `x++`, `x += 1`,
//! `x[k] = v`, `x.f = v`, `&x` and `_ = x` all count), while a bare `x = v`
//! store does not.

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

fn first_error_code(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
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
}

// ---------------------------------------------------------------------------
// Form 1 — unused `:=` binding
// ---------------------------------------------------------------------------

const UNUSED_DEFINE: &str = "\
fn main() {
    x := 5
    fmt.println(\"halo\")
}
";

const USED_DEFINE: &str = "\
fn main() {
    x := 1
    fmt.println(x)
}
";

const WRITE_ONLY_DEFINE: &str = "\
fn main() {
    x := 1
    x = 2
    fmt.println(\"done\")
}
";

const COMPOUND_ASSIGN_COUNTS: &str = "\
fn main() {
    x := 0
    x++
    fmt.println(\"done\")
}
";

const CLOSURE_READ_COUNTS: &str = "\
fn main() {
    hits := 0
    f := fn() { hits++ }
    f()
}
";

const CLOSURE_LOCAL_UNUSED: &str = "\
fn main() {
    f := fn() { z := 1 }
    f()
}
";

const SHADOWED_INNER_UNUSED: &str = "\
fn main() {
    x := 1
    _ = x
    if true {
        x := 2
    }
}
";

const SAME_SCOPE_REDEFINE_WITH_FRESH: &str = "\
fn main() {
    x := 1
    x, y := 2, 3
    fmt.println(x)
    fmt.println(y)
}
";

const SAME_SCOPE_REDEFINE_NO_NEW: &str = "\
fn main() {
    x := 1
    x := 2
    fmt.println(x)
}
";

const IF_INIT_UNUSED: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    if v, ok := m[\"k\"]; ok {
        fmt.println(\"hit\")
    }
}
";

const IF_INIT_USED: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    if v, ok := m[\"k\"]; ok {
        fmt.println(v)
    }
}
";

const MULTI_VALUE_SECOND_UNUSED: &str = "\
fn pair() (int, int) {
    return 1, 2
}

fn main() {
    a, b := pair()
    fmt.println(a)
}
";

#[test]
fn if_init_unused_name_is_rejected() {
    // The if-init `v, ok := ...` is a `:=` binding like any other: Go
    // rejects the emitted Go with `declared and not used: v`.
    let r = check(IF_INIT_UNUSED);
    assert_rejects_e1002(&r, "an unread if-init v");
}

#[test]
fn if_init_used_names_accept() {
    let r = check(IF_INIT_USED);
    assert!(
        error_codes(&r).is_empty(),
        "a read if-init must stay accepted, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn multi_value_second_name_unused_is_rejected() {
    // `a, b := pair()` with b never read is `declared and not used: b`.
    let r = check(MULTI_VALUE_SECOND_UNUSED);
    assert_rejects_e1002(&r, "an unread multi-value b");
}

#[test]
fn unused_define_is_rejected_with_e1002_first() {
    let r = check(UNUSED_DEFINE);
    assert_rejects_e1002(&r, "x := 5 with no read of x");
    assert_eq!(
        first_error_code(&r).as_deref(),
        Some("HYB-E1002"),
        "the buildability reject is the first (source-order) error"
    );
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        !d.fixits.is_empty(),
        "O17: the diagnostic must teach the fix"
    );
    assert!(d.message.contains('x'), "{}", d.message);
}

#[test]
fn used_define_accepts() {
    let r = check(USED_DEFINE);
    assert!(
        error_codes(&r).is_empty(),
        "a read must keep `x := 1` legal, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn write_only_define_is_rejected() {
    // Go: `x := 1; x = 2` is still `declared and not used` — a bare store
    // is not a read.
    let r = check(WRITE_ONLY_DEFINE);
    assert_rejects_e1002(&r, "a write-only x");
}

#[test]
fn compound_assign_and_incdec_count_as_reads() {
    // Go accepts `x := 0; x++` (the ++ reads x); the checker must too.
    let r = check(COMPOUND_ASSIGN_COUNTS);
    assert!(
        error_codes(&r).is_empty(),
        "x++ is a read, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn closure_read_counts_as_use() {
    let r = check(CLOSURE_READ_COUNTS);
    assert!(
        error_codes(&r).is_empty(),
        "a closure read must count, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn closure_local_unused_is_rejected() {
    // Go checks closure locals too: `z := 1` inside the fn literal is
    // `declared and not used` in the emitted Go.
    let r = check(CLOSURE_LOCAL_UNUSED);
    assert_rejects_e1002(&r, "an unused closure-local z");
}

#[test]
fn shadowed_inner_define_is_still_checked() {
    // The outer `x` is read; the INNER `x := 2` is not — name-keyed "used
    // anywhere" analysis would wrongly accept this.
    let r = check(SHADOWED_INNER_UNUSED);
    assert_rejects_e1002(&r, "the shadowing inner x := 2");
}

#[test]
fn same_scope_redefine_with_fresh_name_accepts() {
    // Go: `x := 1; x, y := 2, 3` reuses x (y is the new variable) — valid.
    let r = check(SAME_SCOPE_REDEFINE_WITH_FRESH);
    assert!(
        error_codes(&r).is_empty(),
        "x, y := after x := is valid Go, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn same_scope_redefine_without_new_name_is_rejected() {
    // Go: `no new variables on left side of :=` — the same Go error family
    // the `_ := 5` case belongs to.
    let r = check(SAME_SCOPE_REDEFINE_NO_NEW);
    assert_rejects_e1002(&r, "x := 1; x := 2 (no new variable)");
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        d.message.contains("no new variable"),
        "message names the Go rule: {}",
        d.message
    );
}

// ---------------------------------------------------------------------------
// Form 1 (the `_` half) — `_ := 5` declares no new variable
// ---------------------------------------------------------------------------

const UNDERSCORE_DEFINE: &str = "\
fn main() {
    _ := 5
    fmt.println(\"halo\")
}
";

const MIXED_BLANK_DEFINE: &str = "\
fn main() {
    a, _ := 1, 2
    fmt.println(a)
}
";

const DOUBLE_BLANK_DEFINE: &str = "\
fn main() {
    _, _ := 1, 2
    fmt.println(\"halo\")
}
";

#[test]
fn underscore_define_is_rejected() {
    // Issue #134's explicit sub-case: `_ := 5` emits `_ := 5`, which Go
    // rejects with `no new variables on left side of :=`.
    let r = check(UNDERSCORE_DEFINE);
    assert_rejects_e1002(&r, "_ := 5");
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        d.message.contains("no new variable"),
        "message names the Go rule: {}",
        d.message
    );
}

#[test]
fn mixed_blank_define_accepts() {
    // `a, _ := 1, 2` has a new variable (a) — valid Go; `_` is not checked.
    let r = check(MIXED_BLANK_DEFINE);
    assert!(
        error_codes(&r).is_empty(),
        "a, _ := 1, 2 is valid Go, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn double_blank_define_is_rejected() {
    let r = check(DOUBLE_BLANK_DEFINE);
    assert_rejects_e1002(&r, "_, _ := 1, 2");
}

// ---------------------------------------------------------------------------
// Form 2 — unused import
// ---------------------------------------------------------------------------

const UNUSED_IMPORT: &str = "\
import \"strings\"

fn main() {
    fmt.println(\"halo\")
}
";

const USED_IMPORT: &str = "\
import \"strings\"

fn main() {
    s := strings.repeat(\"a\", 3)
    fmt.println(s)
}
";

const IMPORT_USED_IN_TYPE_ONLY: &str = "\
import \"sync/atomic\"

fn main() {
    var hits atomic.int64
    hits.add(1)
}
";

const IMPORT_USED_IN_CLOSURE_ONLY: &str = "\
import \"fmt\"

fn main() {
    f := fn() { fmt.println(\"x\") }
    f()
}
";

const UNUSED_RUST_IMPORT: &str = "\
import \"rust/serde_json\"

fn main() {
    fmt.println(\"halo\")
}
";

#[test]
fn unused_import_is_rejected() {
    // Issue #134's form 2: the declared import is carried into the emitted
    // header (`imported and not used`).
    let r = check(UNUSED_IMPORT);
    assert_rejects_e1002(&r, "import \"strings\" with no use of strings");
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        d.message.contains("strings"),
        "message names the import: {}",
        d.message
    );
}

#[test]
fn used_import_accepts() {
    let r = check(USED_IMPORT);
    assert!(
        error_codes(&r).is_empty(),
        "a used import must stay accepted, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn import_used_only_in_a_type_accepts() {
    // The emitter registers the package at type sites too — the import is
    // used even though no expression mentions `atomic`.
    let r = check(IMPORT_USED_IN_TYPE_ONLY);
    assert!(
        error_codes(&r).is_empty(),
        "a type-position use must count, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn import_used_only_in_a_closure_accepts() {
    let r = check(IMPORT_USED_IN_CLOSURE_ONLY);
    assert!(
        error_codes(&r).is_empty(),
        "a closure use must count, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn unused_rust_import_is_out_of_scope() {
    // Documented scope boundary: a `rust/` import is never emitted as a Go
    // import, so it cannot fail `go build` — this pass leaves it alone
    // (unlike an unused Go import, which is rejected above).
    let r = check(UNUSED_RUST_IMPORT);
    assert!(
        error_codes(&r).is_empty(),
        "an unused rust/ import cannot break the build, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// Form 3 — repeated name on the left of one `:=`
// ---------------------------------------------------------------------------

const REPEATED_DEFINE_NAME: &str = "\
fn main() {
    x, x := 1, 2
    fmt.println(x)
}
";

const DISTINCT_DEFINE_NAMES: &str = "\
fn main() {
    x, y := 1, 2
    fmt.println(x)
    fmt.println(y)
}
";

#[test]
fn repeated_define_name_is_rejected() {
    // Issue #134's form 3: `x repeated on left side of :=`.
    let r = check(REPEATED_DEFINE_NAME);
    assert_rejects_e1002(&r, "x, x := 1, 2");
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        d.message.contains("repeated"),
        "message names the Go rule: {}",
        d.message
    );
}

#[test]
fn distinct_define_names_accept() {
    let r = check(DISTINCT_DEFINE_NAMES);
    assert!(
        error_codes(&r).is_empty(),
        "x, y := 1, 2 is valid Go, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
