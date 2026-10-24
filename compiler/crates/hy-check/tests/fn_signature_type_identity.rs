//! Regression test: function types are compared by signature.
//!
//! `OpKind::Fn` used to carry only result kinds and a parameter count, and
//! `kinds_conflict` treated any two fn kinds as compatible. A function value
//! whose parameter or result types differ from the declared fn type passed
//! `hyb check` with 0 diagnostics and failed `go build`
//! (`cannot use g (value of type func(s string) int) as func(int) int value ...`).
//!
//! Parameter kinds are now part of the fn kind when every parameter type is in
//! reach, and two known signatures must agree. Unknown parameter types
//! (inferred fn-literal params, type parameters, foreign types) stay fail-open.
//!
//! Rule: `BUILDABLE` (GUARANTEES.md Row 13). Diagnostic: `HYB-E1002`.

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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn rejects_mismatched_parameter_types() {
    // named fn into a declared fn type
    assert_e1002(
        "fn g(s string) int {\n    return len(s)\n}\nfn main() {\n    var f fn(int) int = g\n    fmt.println(f(1))\n}\n",
    );
    // fn literal into a declared fn type
    assert_e1002(
        "fn main() {\n    var f fn(int) = fn(s string) {\n        fmt.println(s)\n    }\n    f(1)\n}\n",
    );
    // struct field of fn type
    assert_e1002(
        "struct s {\n    cb fn(int) int\n}\nfn g(x string) int {\n    return 0\n}\nfn main() {\n    v := s{cb: g}\n    fmt.println(v.cb(1))\n}\n",
    );
    // returned fn value
    assert_e1002(
        "fn g(x string) int {\n    return 0\n}\nfn mk() fn(int) int {\n    return g\n}\nfn main() {\n    fmt.println(mk()(1))\n}\n",
    );
    // fn argument with a different parameter count
    assert_e1002(
        "fn g(a int, b int) int {\n    return a + b\n}\nfn apply(f fn(int) int) int {\n    return f(1)\n}\nfn main() {\n    fmt.println(apply(g))\n}\n",
    );
}

#[test]
fn rejects_mismatched_result_types() {
    assert_e1002(
        "fn main() {\n    var f fn() int = fn() string {\n        return \"s\"\n    }\n    fmt.println(f())\n}\n",
    );
    // int and float64 results are distinct: no implicit conversion inside a signature
    assert_e1002(
        "fn g(n int) f64 {\n    return 1.0\n}\nfn main() {\n    var f fn(int) int = g\n    fmt.println(f(1))\n}\n",
    );
    // void vs one result, both directions
    assert_e1002(
        "fn g(n int) {\n}\nfn main() {\n    var f fn(int) int = g\n    fmt.println(f(1))\n}\n",
    );
    assert_e1002(
        "fn g(n int) int {\n    return n\n}\nfn main() {\n    var f fn(int) = g\n    f(1)\n}\n",
    );
}

#[test]
fn accepts_matching_and_unknown_signatures() {
    assert_ok(
        "fn g(s string) int {\n    return len(s)\n}\nfn twice(n int) int {\n    return n * 2\n}\nfn apply(f fn(int) int, x int) int {\n    return f(x)\n}\nfn main() {\n    var f fn(string) int = g\n    h := twice\n    fmt.println(f(\"abc\"), apply(twice, 3), apply(h, 4))\n    var k fn(int) int = fn(n int) int {\n        return n + 1\n    }\n    k = twice\n    fmt.println(k(1))\n    fs := []fn(int) int{twice, k}\n    m := map[string]fn(int) int{\"t\": twice}\n    fmt.println(len(fs), m[\"t\"](5))\n}\n",
    );
    // grouped parameter names in a fn type: `a` is a name, not a type
    assert_ok(
        "fn add(a int, b int) int {\n    return a + b\n}\nfn main() {\n    var f fn(a, b int) int = add\n    fmt.println(f(1, 2))\n}\n",
    );
    // type parameters and inferred fn-literal params stay fail-open
    assert_ok(
        "fn ap[t any](f fn(t) t, x t) t {\n    return f(x)\n}\nfn inc(n int) int {\n    return n + 1\n}\nfn main() {\n    fmt.println(ap(inc, 1))\n    s := []int{3, 1, 2}\n    sort.slice(s, fn(i, j int) bool {\n        return s[i] < s[j]\n    })\n    fmt.println(s)\n}\n",
    );
}
