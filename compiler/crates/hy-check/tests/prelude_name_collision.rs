//! prelude_name_collision.rs — regression tests for issue #777.
//!
//! `hyb check` accepted programs whose emitted Go could never build, because
//! the emitter injects a prelude (`OPTION_PRELUDE` / `SET_PRELUDE` /
//! `POP_PRELUDE`, `hy-emit/src/lib.rs`) into the same Go file as the user's
//! declarations. A user top-level name the prelude declares — or a
//! predeclared identifier the prelude's body calls — therefore collides.
//!
//! Two shapes, both rejected as HYB-E1002 (rule `BUILDABLE`):
//!
//!   1. **redeclaration** — `option`, `some`, `set`, `hy_pop` are declared at
//!      package scope by the prelude, so a user declaration of one of those
//!      names is `redeclared in this block`;
//!   2. **predeclared capture** — a package-level `make` / `len` / `delete` /
//!      `panic` shadows the builtin for the whole file, so the prelude's own
//!      body resolves its unqualified call to the user's declaration.
//!
//! Every reject case below was reproduced against `main` as a real `go build`
//! failure *before* the check was written, and every accept case was verified
//! to build (see the corpus fixtures
//! `prelude-name-collision-reject.hy` / `-controls-accept.hy`).
//!
//! The accept-side cases are the point of the issue: `some` and `hy_pop` are
//! rejected unconditionally because their own declaration spelling trips the
//! emitter's predicate, while `option` / `set` / `len` / `make` / `delete` /
//! `panic` are gated on the surface that actually pulls their prelude in — a
//! bare `fn set`, `struct option` or `fn len` emits no prelude and must stay
//! accepted.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

/// The rejection must be HYB-E1002 and must name the reserved identifier, so
/// the diagnostic teaches which name to rename.
fn assert_rejected_naming(src: &str, name: &str) {
    let r = check(src);
    assert_eq!(
        codes(&r),
        vec!["HYB-E1002".to_string()],
        "expected exactly one HYB-E1002 for `{}`; got {:?}",
        name,
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
    let msg = &r.diagnostics[0].message;
    assert!(
        msg.contains(&format!("`{}`", name)),
        "diagnostic must name `{}`: {}",
        name,
        msg
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// 1. Redeclaration — names the preludes declare
// ---------------------------------------------------------------------------

#[test]
fn user_fn_some_is_rejected() {
    // `fn some` + an option-typed signature: the option prelude declares
    // `func some[v any](val v) option[v]` and the user declares `func some`.
    assert_rejected_naming(
        "\
fn some(x int) int {
  return x + 1
}

fn has(o option[int]) bool {
  return o.is_some()
}

fn main() {
  n := some(1)
  fmt.println(\"{n}\")
}
",
        "some",
    );
}

#[test]
fn user_fn_some_is_rejected_even_with_no_option_surface() {
    // The declaration spelling `func some(` is ITSELF what puts `some(` in the
    // emitted body, so the option prelude is injected for any program that
    // declares `some`. There is no "unused `some`" escape hatch — which is why
    // this name is rejected unconditionally.
    assert_rejected_naming(
        "\
fn some(x int) int {
  return x + 1
}

fn main() {
  fmt.println(\"ok\")
}
",
        "some",
    );
}

#[test]
fn user_struct_option_is_rejected_when_option_surface_used() {
    assert_rejected_naming(
        "\
struct option {
  n int
}

fn has(o option[int]) bool {
  return o.is_some()
}

fn main() {
  v := option{n: 1}
  fmt.println(\"{v.n}\")
}
",
        "option",
    );
}

#[test]
fn user_fn_hy_pop_is_rejected() {
    // `func hy_pop(` is what trips `needs_pop`, so this name collides for the
    // same unconditional reason as `some`.
    assert_rejected_naming(
        "\
fn hy_pop(xs []int) int {
  return 0
}

fn main() {
  fmt.println(\"{hy_pop([]int{1})}\")
}
",
        "hy_pop",
    );
}

// ---------------------------------------------------------------------------
// 2. Predeclared capture — builtins the prelude bodies call
// ---------------------------------------------------------------------------

#[test]
fn user_fn_len_is_rejected_when_set_surface_used() {
    // SET_PRELUDE's body is `func (s set[t]) len() int { return len(s.m) }`;
    // a package-level `func len` captures that unqualified call.
    assert_rejected_naming(
        "\
fn len(xs []int) int {
  return 42
}

fn main() {
  s := set[int]{}
  s.insert(1)
  fmt.println(\"{len([]int{1})}\")
}
",
        "len",
    );
}

#[test]
fn user_fn_delete_is_rejected_when_set_surface_used() {
    // SET_PRELUDE's `remove` calls `delete(s.m, v)`.
    assert_rejected_naming(
        "\
fn delete(x int) int {
  return x
}

fn main() {
  s := set[int]{}
  s.insert(1)
  s.remove(1)
  fmt.println(\"{delete(1)}\")
}
",
        "delete",
    );
}

#[test]
fn user_fn_make_is_rejected_when_set_surface_used() {
    // SET_PRELUDE's `insert` calls `make(map[t]struct{})`.
    assert_rejected_naming(
        "\
fn make(x int) int {
  return x
}

fn main() {
  s := set[int]{}
  s.insert(1)
  fmt.println(\"{make(1)}\")
}
",
        "make",
    );
}

#[test]
fn user_fn_panic_is_rejected_when_option_surface_used() {
    // OPTION_PRELUDE's `unwrap` calls `panic(\"HY-PANIC/unwrap-none\")`.
    assert_rejected_naming(
        "\
fn panic(x int) int {
  return x
}

fn main() {
  o := some(3)
  fmt.println(\"{o.unwrap()}\")
}
",
        "panic",
    );
}

// ---------------------------------------------------------------------------
// 3. Accept-side controls — these must stay ACCEPTED
// ---------------------------------------------------------------------------

/// Every control: 0 diagnostics, verdict Accept.
fn assert_accepted(src: &str, why: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "{} — but got {:?}",
        why,
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
    assert!(codes(&r).is_empty(), "{} — codes {:?}", why, codes(&r));
}

#[test]
fn user_fn_set_without_set_surface_is_accepted() {
    // `needs_set` keys on `set[`, `.insert(`, `.remove(`, `.contains(` — the
    // declaration `func set(` and a plain `set(...)` call trip none of them,
    // so no set prelude is emitted and there is nothing to collide with.
    // Verified to build on main before this check existed.
    assert_accepted(
        "\
fn set(dst []map[string]int, v map[string]int) {
  dst[0] = v
}

fn main() {
  d := []map[string]int{map[string]int{\"k\": 0}}
  set(d, map[string]int{\"a\": 1})
  for v in d {
    fmt.println(\"{v}\")
  }
}
",
        "`fn set` with no set surface emits no prelude",
    );
}

#[test]
fn user_struct_option_without_option_surface_is_accepted() {
    // A bare `struct option` emits `type option struct {`, which contains
    // neither `option[` nor `some(`, so no prelude lands. This is the case
    // that makes an unconditional "reject `option`" rule wrong.
    assert_accepted(
        "\
struct option {
  n int
}

fn main() {
  fmt.println(\"ok\")
}
",
        "`struct option` alone emits no prelude",
    );
}

#[test]
fn user_fn_len_without_set_surface_is_accepted() {
    assert_accepted(
        "\
fn len(xs []int) int {
  return 42
}

fn main() {
  fmt.println(\"{len([]int{1})}\")
}
",
        "`fn len` with no set surface emits no prelude",
    );
}

#[test]
fn user_method_named_some_is_accepted() {
    // A Go method is `func (p pair) some() int` — its own method set, which
    // does NOT collide with the prelude's package-scope `func some`. The
    // method name does put `some(` in the body (so the prelude IS injected),
    // but nothing is redeclared.
    assert_accepted(
        "\
struct pair {
  a int
  b int
}

fn (p pair) some() int {
  return p.a
}

fn main() {
  q := pair{a: 1, b: 2}
  fmt.println(\"{q.some()}\")
}
",
        "a method named `some` does not redeclare the prelude's function",
    );
}

#[test]
fn user_struct_associated_fn_named_some_is_accepted() {
    // `fn holder.some(...)` lowers to the package-scope name `holder_some`,
    // so it does not collide with the prelude's `some` either.
    assert_accepted(
        "\
struct holder {
  n int
}

fn holder.some(x int) int {
  return x + 1
}

fn main() {
  fmt.println(\"{holder.some(1)}\")
}
",
        "`fn S.some` lowers to `S_some` and does not collide",
    );
}
