//! Binding-shape guards (parallel counts + declared-type compat) — #905/#903 class.
//!
//! A parallel declaration/assignment whose name and value counts differ
//! fails go build with `assignment mismatch`; a provably single value bound
//! to several names likewise. A typed declaration's initializers must
//! satisfy the declared type, and a parallel assignment is positional —
//! each pair must satisfy the same kind check as the single-target form
//! (with the O8 slot-loan fail-open preserved). Valid counts and adaptive
//! initializers stay legal.

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

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn parallel_decl_count_mismatch_reject() {
    assert_code(
        "fn main() {\n    a, b, c := 1, 2\n    _ = a\n    _ = b\n    _ = c\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    d, e := 1, 2, 3\n    _ = d\n    _ = e\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    x, y := 1\n    _ = x\n    _ = y\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn parallel_assign_count_mismatch_reject() {
    assert_code(
        "fn main() {\n    var x int\n    var y int\n    x, y = 10, 20, 30\n    _ = x\n    _ = y\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    var x int\n    var y int\n    x, y = 10\n    _ = x\n    _ = y\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn typed_decl_initializer_conflict_reject() {
    assert_code(
        "fn main() {\n    var a int = \"hello\"\n    _ = a\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    let b bool = 123\n    _ = b\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    const s string = 456\n    _ = s\n}\n",
        "HYB-E1002",
    );
    // NOTE: the grammar has no multi-name typed declaration (`var a, b int =
    // 1, 2` is E0002 at the parser), so the positional zip guard in `local`
    // is defensive class-shape; the single-name forms above are the issue's
    // actual repros.
}

#[test]
fn parallel_assign_positional_conflict_reject() {
    assert_code(
        "fn main() {\n    x := 1\n    s := \"a\"\n    x, s = s, x\n    _ = x\n    _ = s\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn valid_parallel_forms_stay_legal() {
    // matching counts stay legal
    assert_ok(
        "fn main() {\n    a, b, c := 1, 2, 3\n    _ = a\n    _ = b\n    _ = c\n}\n",
    );
    // same-type swap stays legal
    assert_ok(
        "fn main() {\n    x := 1\n    s := \"a\"\n    x, s = x, s\n    _ = x\n    _ = s\n}\n",
    );
    // the untyped-int-to-float adaptation stays legal (kinds_conflict arm)
    assert_ok(
        "fn main() {\n    var f float64 = 1\n    _ = f\n}\n",
    );
    // an interface target accepts any initializer
    assert_ok(
        "fn main() {\n    var a any = 1\n    var b any = \"str\"\n    _ = a\n    _ = b\n}\n",
    );
    // matching counts in a parallel declaration stay legal
    assert_ok(
        "fn main() {\n    a, b := 1, 2\n    _ = a\n    _ = b\n}\n",
    );
    // typed parallel assignment with matching kinds (the zip loop's happy path)
    assert_ok(
        "fn main() {\n    var x int\n    var y int\n    x, y = 1, 2\n    _ = x\n    _ = y\n}\n",
    );
    // `f()?` on a (T1, T2, error) call yields two values after the unwrap —
    // stays legal (the 10-a5-http-handler-capture corpus class); the Try
    // source is fail-open for the count guards.
    assert_ok(
        "fn main() {\n    code, body := http.get(\"/x\")?\n    fmt.println(code, body)\n}\n",
    );
}
