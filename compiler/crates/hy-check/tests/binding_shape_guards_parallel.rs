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

#[test]
fn multi_value_assign_and_same_scope_redefine_type_conflict_reject() {
    // Multi-target assignment from a multi-return call with mismatched target types
    assert_code(
        "fn two_strings() (string, string) { return \"a\", \"b\" }\nfn main() {\n    var a int\n    var b int\n    a, b = two_strings()\n    _ = a\n    _ = b\n}\n",
        "HYB-E1002",
    );
    // Multi-target assignment from map comma-ok lookup into non-bool second target
    assert_code(
        "fn main() {\n    m := map[string]int{\"a\": 1}\n    var v int\n    var ok int\n    v, ok = m[\"a\"]\n    _ = v\n    _ = ok\n}\n",
        "HYB-E1002",
    );
    // Multi-target assignment from channel comma-ok receive into non-bool second target
    assert_code(
        "fn main() {\n    ch := make(chan int, 1)\n    var v int\n    var ok string\n    v, ok = <-ch\n    _ = v\n    _ = ok\n}\n",
        "HYB-E1002",
    );
    // Same-scope `:=` re-declaration assigning incompatible type to existing variable
    assert_code(
        "fn main() {\n    a := 1\n    a, b := \"hello\", 2\n    _ = a\n    _ = b\n}\n",
        "HYB-E1002",
    );
    // Triple-return call `a, b, c := triple()` propagates 3rd result kind
    assert_code(
        "fn triple() (int, int, string) { return 1, 2, \"s\" }\nfn main() {\n    a, b, c := triple()\n    var x int = c\n    _ = a\n    _ = b\n    _ = x\n}\n",
        "HYB-E1002",
    );
    // Try-unwrapped multi-return call `a, b := pair_err()?` propagates result kinds
    assert_code(
        "fn pair_err() (int, string, error) { return 1, \"s\", nil }\nfn run() error {\n    a, b := pair_err()?\n    var x int = b\n    _ = a\n    _ = x\n    return nil\n}\nfn main() {}\n",
        "HYB-E1002",
    );
    // Forwarded multi-value call to function or method with mismatched parameter types / arity
    assert_code(
        "fn two_strings() (string, string) { return \"a\", \"b\" }\nfn take_ints(x int, y int) {\n    _ = x\n    _ = y\n}\nfn main() {\n    take_ints(two_strings())\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "struct sink { n int }\nfn (s sink) one(x int) {\n    _ = s.n\n    _ = x\n}\nfn two_ints() (int, int) { return 1, 2 }\nfn main() {\n    s := sink{ n: 0 }\n    s.one(two_ints())\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn multi_value_assign_and_forwarded_method_call_positive_controls() {
    assert_ok(
        "struct sink { n int }\nfn (s sink) pair(x int, y string) {\n    _ = s.n\n    _ = x\n    _ = y\n}\nfn mixed() (int, string) { return 1, \"ok\" }\nfn main() {\n    s := sink{ n: 0 }\n    s.pair(mixed())\n    var a int\n    var b string\n    a, b = mixed()\n    a, c := 2, true\n    _ = a\n    _ = b\n    _ = c\n}\n",
    );
}
