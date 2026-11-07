//! A declaration named `none` must be HYB-E1002 at check time
//! (GUARANTEES Row 13). `none` is Hybrid's option constructor; hy-emit
//! treats every bare Ident `none` as that constructor, so before this
//! pass a declaration named `none` was `OK (0 diagnostics)` and then
//! `hyb build` failed with `not yet supported: none outside a return`.
//! `return none` and match arms `none:` / `none =>` are not declarations
//! and stay accepted. `#1657` lowers unbound `none` in value positions —
//! a different niche from rejecting declarations that steal the name.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn assert_rejects(src: &str, what: &str) {
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}\ngot {errs:?}");
    let needle = format!("{what} named `none`");
    assert!(
        errs.iter()
            .any(|e| e.starts_with("HYB-E1002") && e.contains(&needle) && e.contains("option constructor")),
        "expected HYB-E1002 `{needle}` for:\n{src}\ngot {errs:?}"
    );
}

#[test]
fn every_declaration_site_named_none_is_rejected() {
    let cases: &[(&str, &str)] = &[
        ("local", "fn main() {\n    none := 1\n    fmt.println(\"{none}\")\n}\n"),
        ("local", "fn main() {\n    var none int = 1\n    fmt.println(\"{none}\")\n}\n"),
        ("parameter", "fn f(none int) int { return none + 1 }\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n"),
        ("parameter", "fn main() {\n    f := fn(none int) int { return none }\n    fmt.println(\"{f(1)}\")\n}\n"),
        ("named result", "fn f() (none int) {\n    none = 2\n    return\n}\nfn main() {\n    fmt.println(\"{f()}\")\n}\n"),
        ("fn", "fn none() int { return 1 }\nfn main() {\n    fmt.println(\"{none()}\")\n}\n"),
        ("method", "struct c { n int }\nfn (x c) none() int { return x.n }\nfn main() {\n    v := c{n: 1}\n    fmt.println(\"{v.none()}\")\n}\n"),
        ("receiver", "struct c { n int }\nfn (none c) get() int { return none.n }\nfn main() {\n    v := c{n: 1}\n    fmt.println(\"{v.get()}\")\n}\n"),
        ("type parameter", "fn id[none any](x none) none { return x }\nfn main() {\n    fmt.println(\"{id(1)}\")\n}\n"),
        ("struct field", "struct p { none int }\nfn main() {\n    v := p{none: 1}\n    fmt.println(\"{v.none}\")\n}\n"),
        ("const", "const none = 3\nfn main() {\n    fmt.println(\"{none}\")\n}\n"),
        ("type alias", "type none = int\nfn main() {\n    var x none = 3\n    fmt.println(\"{x}\")\n}\n"),
        ("loop variable", "fn main() {\n    for none in [1, 2] {\n        fmt.println(\"{none}\")\n    }\n}\n"),
        ("loop variable", "fn main() {\n    for none in 0..2 {\n        fmt.println(\"{none}\")\n    }\n}\n"),
        ("select receive binding", "fn main() {\n    ch := make(chan int, 1)\n    ch <- 1\n    select {\n        none := <-ch: fmt.println(\"{none}\")\n    }\n}\n"),
        ("match binding", "fn main() {\n    o := some(1)\n    match o {\n        some(none) => fmt.println(\"{none}\")\n        none => fmt.println(\"n\")\n    }\n}\n"),
        ("local", "fn main() {\n    f := fn() int {\n        if true {\n            none := 1\n            return none\n        }\n        return 0\n    }\n    fmt.println(\"{f()}\")\n}\n"),
    ];
    for (what, src) in cases {
        assert_rejects(src, what);
    }
}

#[test]
fn option_none_surface_and_similar_names_stay_accepted() {
    for src in [
        // return none against option[T] — not a declaration
        "fn opt() option[int] { return none }\nfn main() {\n    match opt() {\n        some(v): fmt.println(\"{v}\")\n        none: fmt.println(\"n\")\n    }\n}\n",
        // match arm heads
        "fn main() {\n    o := some(1)\n    match o {\n        some(v) => fmt.println(\"{v}\")\n        none => fmt.println(\"n\")\n    }\n}\n",
        "fn main() {\n    o := some(1)\n    match o {\n        some(v): fmt.println(\"{v}\")\n        none: fmt.println(\"n\")\n    }\n}\n",
        // names that merely contain the word
        "fn main() {\n    none_val := 1\n    is_none := true\n    fmt.println(\"{none_val} {is_none}\")\n}\n",
        // option API
        "fn main() {\n    o := some(1)\n    v := o.or(2)\n    fmt.println(\"{v} {o.is_none()}\")\n}\n",
        // some stays shadowable (emit does not special-case Ident some)
        "fn main() {\n    some := 1\n    fmt.println(\"{some}\")\n}\n",
    ] {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "must accept:\n{src}\ngot {:?}",
            errors(&r)
        );
    }
}
