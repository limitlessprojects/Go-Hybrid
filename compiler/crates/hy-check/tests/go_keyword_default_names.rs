//! A declaration named `default` must be HYB-E1002 at check time
//! (GUARANTEES Row 13). `default` is the one Go keyword LX3 leaves
//! unreserved (it is contextual at a select arm head), and hy-emit spells
//! names verbatim, so before this pass every rejected program below was
//! `OK (0 diagnostics)` and then `hyb build` failed in gofmt / go build
//! (`expected '}', found 'default'`).

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
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
    let needle = format!("{what} named `default`");
    assert!(
        errs.iter()
            .any(|e| e.starts_with("HYB-E1002") && e.contains(&needle) && e.contains("Go keyword")),
        "expected HYB-E1002 `{needle}` for:\n{src}\ngot {errs:?}"
    );
}

#[test]
fn every_declaration_site_named_default_is_rejected() {
    let cases: &[(&str, &str)] = &[
        ("local", "fn main() {\n    default := make(chan int, 1)\n    default <- 1\n    v := <-default\n    fmt.println(\"{v}\")\n}\n"),
        ("local", "fn main() {\n    var default int = 1\n    fmt.println(\"{default}\")\n}\n"),
        ("parameter", "fn f(default int) int { return default + 1 }\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n"),
        ("parameter", "fn main() {\n    f := fn(default int) int { return default }\n    fmt.println(\"{f(1)}\")\n}\n"),
        ("named result", "fn f() (default int) {\n    default = 2\n    return\n}\nfn main() {\n    fmt.println(\"{f()}\")\n}\n"),
        ("fn", "fn default() int { return 1 }\nfn main() {\n    fmt.println(\"{default()}\")\n}\n"),
        ("method", "struct c { n int }\nfn (x c) default() int { return x.n }\nfn main() {\n    v := c{n: 1}\n    fmt.println(\"{v.default()}\")\n}\n"),
        ("receiver", "struct c { n int }\nfn (default c) get() int { return default.n }\nfn main() {\n    v := c{n: 1}\n    fmt.println(\"{v.get()}\")\n}\n"),
        ("type parameter", "fn id[default any](x default) default { return x }\nfn main() {\n    fmt.println(\"{id(1)}\")\n}\n"),
        ("struct field", "struct p { default int }\nfn main() {\n    v := p{default: 1}\n    fmt.println(\"{v.default}\")\n}\n"),
        ("const", "const default = 3\nfn main() {\n    fmt.println(\"{default}\")\n}\n"),
        ("type alias", "type default = int\nfn main() {\n    var x default = 3\n    fmt.println(\"{x}\")\n}\n"),
        ("loop variable", "fn main() {\n    for default in [1, 2] {\n        fmt.println(\"{default}\")\n    }\n}\n"),
        ("loop variable", "fn main() {\n    for default in 0..2 {\n        fmt.println(\"{default}\")\n    }\n}\n"),
        ("select receive binding", "fn main() {\n    ch := make(chan int, 1)\n    ch <- 1\n    select {\n        default := <-ch: fmt.println(\"{default}\")\n    }\n}\n"),
        ("match binding", "fn main() {\n    o := some(1)\n    match o {\n        some(default) => fmt.println(\"{default}\")\n        none => fmt.println(\"n\")\n    }\n}\n"),
        ("local", "fn main() {\n    f := fn() int {\n        if true {\n            default := 1\n            return default\n        }\n        return 0\n    }\n    fmt.println(\"{f()}\")\n}\n"),
        // TypeExpr cases (Issue #3780)
        ("parameter", "type cb = fn(default int) int\nfn main() {\n    var f cb = fn(x int) int { return x }\n    fmt.println(\"{f(1)}\")\n}\n"),
        ("parameter", "struct s {\n    cb fn(default int) int\n}\nfn main() {\n    fmt.println(\"ok\")\n}\n"),
        ("parameter", "fn run(f fn(default int) int) int {\n    return f(1)\n}\nfn main() {\n    fmt.println(\"ok\")\n}\n"),
        ("parameter", "fn make_cb() fn(default int) int {\n    return fn(x int) int { return x }\n}\nfn main() {\n    fmt.println(\"ok\")\n}\n"),
        ("parameter", "fn main() {\n    var f fn(default int) int\n    fmt.println(\"ok\")\n}\n"),
        ("parameter", "fn main() {\n    var a [len([1]int{fn(default int) int { return default }(0)})]int\n    fmt.println(\"{len(a)}\")\n}\n"),
    ];
    for (what, src) in cases {
        assert_rejects(src, what);
    }
}

#[test]
fn select_default_arm_and_similar_names_stay_accepted() {
    for src in [
        // the contextual select arm head is not a declaration
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n        v := <-ch: fmt.println(\"{v}\")\n        default: fmt.println(\"none\")\n    }\n}\n",
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n        v := <-ch: fmt.println(\"{v}\")\n        default => fmt.println(\"none\")\n    }\n}\n",
        // names that merely contain the word
        "fn main() {\n    default_ch := make(chan int, 1)\n    defaults := [1]\n    is_default := true\n    default_ch <- defaults[0]\n    v := <-default_ch\n    fmt.println(\"{v} {is_default}\")\n}\n",
        // option's `.or(default)` keeps its documented parameter name
        "fn main() {\n    o := some(1)\n    v := o.or(2)\n    fmt.println(\"{v}\")\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "must accept:\n{src}\ngot {:?}", errors(&r));
    }
}
