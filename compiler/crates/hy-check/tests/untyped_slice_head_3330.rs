//! Issue #3330 (GUARANTEES Row 13, BUILDABLE): an untyped slice literal whose
//! first item is not a basic literal took the `None` branch of
//! `untyped_slice_head_ty` and returned without a diagnostic. `hyb check` said
//! `OK (0 diagnostics)` and `hyb build` then refused the same file with
//! `untyped slice literal of \`x\` — spell the element type`, because hy-emit's
//! `infer_slice_head` reads the element type only from a basic literal, a sign,
//! parentheses, or a typed struct literal.
//!
//! The five shapes below are the ones the issue lists (identifier, call,
//! binary expression, field selection, index expression). Each must now be
//! rejected at check time, and the literal-headed forms the emitter does
//! support must stay accepted.

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

fn program(body: &str) -> String {
    format!(
        "struct box {{ n int }}\nfn mk() box {{ return box{{n: 1}} }}\n\
         fn main() {{ {body}\n fmt.println(\"ok\") }}\n"
    )
}

#[test]
fn a_non_literal_head_is_rejected_at_check_time() {
    for (body, head) in [
        // identifier — the issue's minimal repro
        ("var x int = 5\n xs := [x]\n _ = xs", "`x`"),
        // function call
        ("b := mk()\n bs := [b]\n _ = bs", "`b`"),
        // binary expression
        ("var a int = 1\n var b int = 2\n xs := [a + b]\n _ = xs", "`a + b`"),
        // field selection
        ("var bx box = box{n: 1}\n xs := [bx.n]\n _ = xs", "`bx.n`"),
        // index expression
        ("var arr []int = []int{1, 2}\n var i int = 0\n xs := [arr[i]]\n _ = xs", "`arr[...]`"),
    ] {
        let src = program(body);
        let r = check(&src);
        let errs = errors(&r);
        assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
        assert!(
            errs.iter()
                .any(|e| e.starts_with("HYB-E1002") && e.contains(head)),
            "expected HYB-E1002 naming {head} for:\n{src}\ngot {errs:?}"
        );
    }
}

#[test]
fn a_non_literal_head_is_rejected_in_a_statement_position_too() {
    // The check must not depend on the literal being bound with `:=`.
    for body in [
        "var x int = 5\n show([x])",
        "var x int = 5\n for _, v in [x] { fmt.println(v) }",
    ] {
        let src = program(body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
        assert!(
            errors(&r).iter().any(|e| e.starts_with("HYB-E1002")),
            "expected HYB-E1002 for:\n{src}\ngot {:?}",
            errors(&r)
        );
    }
}

#[test]
fn the_heads_the_emitter_supports_stay_accepted() {
    // One negative control per emitter branch in `infer_slice_head`: basic
    // literals, a negated literal, a parenthesized literal, and a typed
    // struct literal. Over-rejecting any of these would be a regression.
    for body in [
        "xs := [1, 2, 3]\n _ = xs",
        "xs := [1.5, 2]\n _ = xs",
        "xs := [\"a\", \"b\"]\n _ = xs",
        "xs := [true, false]\n _ = xs",
        "xs := ['a', 'b']\n _ = xs",
        "xs := [-1, 2]\n _ = xs",
        "xs := [(1), 2]\n _ = xs",
        "xs := [box{n: 1}, box{n: 2}]\n _ = xs",
        // a later item may still be an identifier: only the head is inferred
        "var x int = 3\n xs := [1, x, x + 1]\n _ = xs",
        // an element slot gives the literal an expected type, so it is not
        // an untyped head at all
        "xs := [][]f64{[1, 2.5]}\n _ = xs",
    ] {
        let src = program(body);
        let r = check(&src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "must accept:\n{src}\ngot {:?}",
            errors(&r)
        );
    }
}
