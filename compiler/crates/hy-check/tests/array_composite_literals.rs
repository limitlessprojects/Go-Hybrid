//! Integration tests for issue #643: `[N]T{…}` and `[...]T{…}` array composite literals.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn accepts_all_ten_array_composite_literal_spellings() {
    let src = r#"
const n = 2
const fixed = [3]string{"a", "b", "c"}

struct pair {
    a int
    b int
}

struct holder {
    tags [3]string
}

fn echo_tags(xs [3]string) [3]string {
    return [3]string{xs[0], xs[1], xs[2]}
}

fn worker(arr [3]int, done chan [3]string) {
    fmt.println(arr[0], fixed[0])
    done <- [3]string{"x", "y", "z"}
}

fn main() {
    // 1. Fixed-length array literal
    a := [3]int{1, 2, 3}
    // 2. Inferred-length (`[...]`) array literal
    b := [...]int{1, 2, 3}
    // 3. Empty array literal (zero-initialized)
    c := [3]int{}
    // 4. Partially-initialized array literal
    d := [3]int{1}
    // 5 & 6. Constant-expression length (`[n]` and `[n + 1]`)
    e := [n]int{1, 2}
    f := [n + 1]int{1, 2, 3}
    // 7. Nested multidimensional array literal with elided inner types
    g := [2][2]int{{1, 2}, {3, 4}}
    // 8. Array of structs with type-elided inner struct literals
    h := [2]pair{{a: 1, b: 2}, {a: 3, b: 4}}
    // 9. Array literals in let, var, const, fn arg, return, struct field, map value, chan send
    let l_arr = [3]string{"a", "b", "c"}
    var v_arr [3]string = [3]string{"a", "b", "c"}
    var v_inferred [3]string = [...]string{"a", "b", "c"}
    const c_arr = [3]string{"a", "b", "c"}
    ret_arr := echo_tags([3]string{"a", "b", "c"})
    hd := holder{tags: [3]string{"a", "b", "c"}}
    mp := map[string][3]string{"k": [3]string{"a", "b", "c"}}
    done := make(chan [3]string, 1)
    task worker(move a, done)
    got := <-done
    // 10. Range iteration over an array literal
    for i, v in [3]int{10, 20, 30} {
        fmt.println(i, v)
    }
    fmt.println(
        b[0], c[0], d[0], e[0], f[2], g[1][0], h[1].a,
        l_arr[0], v_arr[1], v_inferred[2], c_arr[0],
        ret_arr[0], hd.tags[0], mp["k"][0], got[0],
        len(b), cap(b)
    )
}
"#;
    let ast = parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:?}",
        res.diagnostics
    );
}

#[test]
fn rejects_excess_elements_in_array_composite_literal() {
    for src in [
        "fn main() {\n    a := [2]int{1, 2, 3}\n    fmt.println(a[0])\n}\n",
        "const n = 1\nfn main() {\n    a := [n + 1]int{1, 2, 3}\n    fmt.println(a[0])\n}\n",
        "fn main() {\n    a := [2][2]int{{1, 2, 3}, {4, 5}}\n    fmt.println(a[0][0])\n}\n",
        "fn main() {\n    var a [3]int = [2]int{1, 2}\n    fmt.println(a[0])\n}\n",
        "fn main() {\n    var a [3]int = [...]int{1, 2}\n    fmt.println(a[0])\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            error_codes(&r).contains(&"HYB-E6004".to_string()),
            "expected HYB-E6004, got {:?} for:\n{src}",
            error_codes(&r)
        );
    }
}

#[test]
fn rejects_element_type_mismatch_in_array_composite_literal() {
    for src in [
        "fn main() {\n    a := [3]int{1, \"two\", 3}\n    fmt.println(a[0])\n}\n",
        "fn main() {\n    a := [...]bool{true, 42}\n    fmt.println(a[0])\n}\n",
        "fn main() {\n    a := [2][2]int{{1, \"bad\"}, {3, 4}}\n    fmt.println(a[0][0])\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002, got {:?} for:\n{src}",
            error_codes(&r)
        );
    }
}

#[test]
fn rejects_constant_overflow_and_nil_and_invalid_lengths_in_array_literals() {
    // Constant overflow -> HYB-E6002
    let r_ovf = check("fn main() {\n    a := [2]int8{1, 300}\n    fmt.println(a[0])\n}\n");
    assert_eq!(r_ovf.verdict, Verdict::Reject);
    assert!(error_codes(&r_ovf).contains(&"HYB-E6002".to_string()));

    // Nil in non-nilable element slot -> HYB-E4001
    let r_nil = check("fn main() {\n    a := [2]*int{nil, nil}\n    fmt.println(a)\n}\n");
    assert_eq!(r_nil.verdict, Verdict::Reject);
    assert!(error_codes(&r_nil).contains(&"HYB-E4001".to_string()));

    // Negative / non-constant array length and [...]T outside composite literal -> HYB-E6004
    for src in [
        "fn main() {\n    a := [-1]int{}\n    fmt.println(a)\n}\n",
        "fn main() {\n    n := 3\n    a := [n]int{1, 2, 3}\n    fmt.println(a[0])\n}\n",
        "fn main() {\n    var a [...]int\n    fmt.println(a)\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            error_codes(&r).contains(&"HYB-E6004".to_string()),
            "expected HYB-E6004, got {:?} for:\n{src}",
            error_codes(&r)
        );
    }
}

#[test]
fn array_literal_bindings_enforce_tier0_sendability_and_move_freeze() {
    // Mutable array binding shared without `move` -> HYB-E3001
    let shared_mut = r#"
fn worker(a [3]int) {
    fmt.println(a[0])
}
fn main() {
    a := [3]int{1, 2, 3}
    task worker(a)
}
"#;
    let r1 = check(shared_mut);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E3001".to_string()));

    // Mutable array binding moved then read -> HYB-E3003
    let use_after_move = r#"
fn worker(a [3]int) {
    fmt.println(a[0])
}
fn main() {
    a := [3]int{1, 2, 3}
    task worker(move a)
    fmt.println(a[0])
}
"#;
    let r2 = check(use_after_move);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E3003".to_string()));
}
