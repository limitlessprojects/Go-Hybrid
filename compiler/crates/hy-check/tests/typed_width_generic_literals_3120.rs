//! typed_width_generic_literals_3120.rs — the #2712 width check must see a
//! typed value written into a field of a generic struct literal, with the
//! literal's type arguments substituted into the field type (#3120), plus
//! two binding shapes from the same family: the element of a nested
//! `for … in` over `[][]T`, and the value of a statement-form comma-ok
//! receive `v, ok := <-c`. Every reject case passed `hyb check` before and
//! its emitted Go fails `go build` (go1.26.9); the accept case builds and
//! runs.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.severity == hy_diag::Severity::Error && d.code == "HYB-E1002")
}

#[test]
fn generic_literal_and_binding_widths_reject_e1002() {
    let cases = [
        r#"struct box[t any] { v t }
fn main() {
    var z int64 = 1
    b := box[int8]{v: z}
    fmt.println(b.v)
}
"#,
        r#"struct box[t any] { v t }
fn main() {
    var z int64 = 1
    var b box[int8] = box[int8]{v: z}
    fmt.println(b.v)
}
"#,
        r#"struct box[t any] { v t }
fn main() {
    var z int64 = 1
    bs := []box[int8]{box[int8]{v: z}}
    fmt.println(bs[0].v)
}
"#,
        r#"struct pair[k any, v any] { a k  b v }
fn main() {
    var z int64 = 1
    p := pair[string, int8]{a: "x", b: z}
    fmt.println(p.b)
}
"#,
        r#"fn main() {
    xs := [][]int8{{1}}
    for row in xs {
        for v in row {
            var z int64 = v
            fmt.println(z)
        }
    }
}
"#,
        r#"fn main() {
    c := make(chan int8, 1)
    c <- 1
    v, ok := <-c
    var z int64 = v
    fmt.println(z, ok)
}
"#,
    ];
    for src in cases {
        let r = check(src);
        assert!(
            has_e1002(&r),
            "expected HYB-E1002 for:
{}
{:?}",
            src,
            r.diagnostics
        );
    }
}

#[test]
fn matching_generic_literal_and_binding_widths_accept() {
    let src = r#"struct box[t any] { v t }
struct pair[k any, v any] { a k  b v }
fn main() {
    var z int8 = 1
    b := box[int8]{v: z}
    c := box[int64]{v: 2}
    p := pair[string, int8]{a: "x", b: z}
    xs := [][]int8{{1}}
    for row in xs {
        for v in row {
            var y int8 = v
            fmt.println(y)
        }
    }
    ch := make(chan int8, 1)
    ch <- 1
    w, ok := <-ch
    var q int8 = w
    fmt.println(b.v, c.v, p.b, q, ok)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected acceptance:
{:?}",
        r.diagnostics
    );
}
