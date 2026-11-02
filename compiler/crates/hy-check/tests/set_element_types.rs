//! #2474: `set[T]` lowers to a prelude struct, so the checker had no kind
//! for it and never compared `insert`/`contains`/`delete` arguments with the
//! element type — `s.insert("x")` on a `set[int]` passed `hyb check` and
//! failed `go build` ("cannot use … as int value in argument to s.insert").

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn errors(r: &CheckResult, code: &str) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == code)
        .count()
}

#[test]
fn issue_2474_repro_rejected() {
    let r = check(
        "fn main() {\n    s := set[int]{}\n    bad := \"incompatible_string\"\n    _ = s.insert(bad)\n    _ = s.contains(bad)\n    delete(s, bad)\n}\n",
    );
    assert_eq!(errors(&r, "HYB-E1002"), 2, "{:?}", r.diagnostics);
    assert_eq!(errors(&r, "HYB-E2005"), 1, "{:?}", r.diagnostics);
}

#[test]
fn every_way_a_set_is_bound_is_checked() {
    for (what, src) in [
        ("literal argument", "fn main() {\n    s := set[int]{}\n    s.insert(\"x\")\n}\n"),
        ("typed var", "fn main() {\n    var s set[string]\n    s.insert(5)\n}\n"),
        (
            "fn parameter",
            "fn add(s set[int], v string) {\n    s.insert(v)\n}\n\nfn main() {\n    add(set[int]{}, \"a\")\n}\n",
        ),
        (
            "fn result",
            "fn mk() set[string] {\n    return set[string]{}\n}\n\nfn main() {\n    s := mk()\n    f := 1.5\n    _ = s.contains(f)\n}\n",
        ),
        (
            "closure parameter",
            "fn main() {\n    f := fn(s set[int]) {\n        s.insert(\"x\")\n    }\n    f(set[int]{})\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(errors(&r, "HYB-E1002"), 1, "{what}: {:?}", r.diagnostics);
    }
}

#[test]
fn legal_set_uses_stay_accepted() {
    for (what, src) in [
        (
            "matching element types",
            "fn main() {\n    s := set[int]{}\n    n := 3\n    s.insert(n)\n    fmt.println(s.contains(3))\n    delete(s, n)\n}\n",
        ),
        (
            "untyped int constant into set[f64]",
            "fn main() {\n    s := set[f64]{}\n    s.insert(1)\n    fmt.println(s.contains(1.0))\n}\n",
        ),
        (
            "struct elements",
            "struct pt {\n    x int\n}\n\nfn main() {\n    s := set[pt]{}\n    s.insert(pt{x: 1})\n    fmt.println(s.contains(pt{x: 1}))\n}\n",
        ),
        (
            "shadowing a set name with a slice",
            "fn main() {\n    s := set[int]{}\n    s.insert(1)\n    if true {\n        s := []string{\"a\"}\n        s = append(s, \"b\")\n        fmt.println(len(s))\n    }\n}\n",
        ),
        (
            "user struct with its own insert",
            "struct bag {\n    n int\n}\n\nfn (b *bag) insert(s string) {\n    b.n += len(s)\n}\n\nfn main() {\n    b := bag{n: 0}\n    b.insert(\"x\")\n    fmt.println(b.n)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(
            errors(&r, "HYB-E1002") + errors(&r, "HYB-E2005"),
            0,
            "{what} must stay legal: {:?}",
            r.diagnostics
        );
    }
}
