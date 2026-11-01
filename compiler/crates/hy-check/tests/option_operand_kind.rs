//! `option[T]` values used as plain `T` operands must be rejected.
//!
//! `option[T]` lowers to the prelude's generic `option[T]` struct, so Go
//! refuses arithmetic, indexing, `len`, and passing/assigning it where a `T`
//! is expected. Before this fix the buildable kind model mapped every option
//! to `Unknown`, which made all of these fail-open: `hyb check` reported
//! `OK (0 diagnostics)` and `go build` failed.

fn diags(src: &str) -> Vec<String> {
    let file = hy_syntax::parse_file("t.hy", src).unwrap();
    hy_check::check_file(&file)
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

const PRE: &str = "fn mk() option[int] {\n    return some(1)\n}\nfn takes_int(x int) {\n    fmt.println(\"{x}\")\n}\n";

fn with_main(decl: &str, body: &str) -> String {
    format!("{PRE}fn main() {{\n    {decl}\n{body}}}\n")
}

#[test]
fn option_used_as_payload_is_rejected() {
    let decls = [
        "op := some(1)",
        "var op option[int] = some(1)",
        "var op option[int] = none",
        "op := mk()",
    ];
    let uses = [
        "    z := op + 1\n    fmt.println(\"{z}\")\n",
        "    var k int = 0\n    k = op\n    fmt.println(\"{k}\")\n",
        "    z := op[0]\n    fmt.println(\"{z}\")\n",
        "    fmt.println(\"{len(op)}\")\n",
        "    takes_int(op)\n",
    ];
    for d in decls {
        for u in uses {
            let src = with_main(d, u);
            assert!(
                !diags(&src).is_empty(),
                "accepted unbuildable option use:\n{src}"
            );
        }
    }
}

#[test]
fn option_param_arithmetic_is_rejected() {
    let src = "fn f(op option[int]) int {\n    return op + 1\n}\nfn main() {\n    fmt.println(\"{f(some(2))}\")\n}\n";
    assert!(!diags(src).is_empty(), "{src}");
}

#[test]
fn option_payload_kind_mismatches_are_rejected() {
    let cases = [
        // payload of `unwrap()` / `or()` used as the wrong type
        "fn mk() option[int] {\n    return some(1)\n}\nfn main() {\n    var s string = mk().unwrap()\n    fmt.println(s)\n}\n",
        "fn mk() option[int] {\n    return some(1)\n}\nfn main() {\n    t := mk().or(2) + \"x\"\n    fmt.println(t)\n}\n",
        // option[int] where option[string] is expected (and vice versa)
        "fn main() {\n    var a option[int] = some(\"x\")\n    _ = a\n}\n",
        "fn main() {\n    a := some(1)\n    var b option[string] = none\n    b = a\n    _ = b\n}\n",
        "fn f(o option[string]) {\n    _ = o\n}\nfn main() {\n    f(some(1))\n}\n",
    ];
    for src in cases {
        assert!(
            !diags(src).is_empty(),
            "accepted option payload mismatch:\n{src}"
        );
    }
}

#[test]
fn valid_option_uses_stay_accepted() {
    let src = "fn mk() option[int] {\n    return some(1)\n}\nfn id(o option[int]) option[int] {\n    return o\n}\nfn main() {\n    op := some(1)\n    var t option[int] = none\n    t = op\n    u := id(t)\n    n := u.unwrap() + 1\n    k := mk().or(5) * 2\n    if op.is_some() && !t.is_none() {\n        fmt.println(\"{n} {k}\")\n    }\n    var a any = op\n    _ = a\n    xs := []option[int]{op, none}\n    fmt.println(\"{len(xs)}\")\n}\n";
    assert_eq!(diags(src), Vec::<String>::new(), "{src}");
}
