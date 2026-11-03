use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<String>) {
    let file = parse_file("test.hy", src).expect("parse");
    let r = check_file(&file);
    (r.verdict, r.diagnostics.iter().map(|d| d.code.clone()).collect())
}

#[test]
fn rejects_composite_element_width_mismatches_across_decl_assign_call_and_return_2875() {
    let cases = [
        // q17: []i64 -> []int in var declaration
        "fn main() {\n    var a []i64\n    var b []int = a\n    fmt.println(b)\n}",
        // q18: map[string]i64 -> map[string]int in var declaration
        "fn main() {\n    var a map[string]i64\n    var b map[string]int = a\n    fmt.println(b)\n}",
        // q15: *i64 (via p := &v) -> *int in var declaration
        "fn main() {\n    var v i64 = 1\n    p := &v\n    var q *int = p\n    fmt.println(q)\n}",
        // s3: [2]i64 -> [2]int in var declaration
        "fn main() {\n    var a [2]i64\n    var b [2]int = a\n    fmt.println(b)\n}",
        // s1: []i64 -> []int in call argument
        "fn take(xs []int) int {\n    return len(xs)\n}\nfn main() {\n    var a []i64\n    fmt.println(take(a))\n}",
        // s2: []i64 -> []int in return statement
        "fn mk() []int {\n    var a []i64\n    return a\n}\nfn main() {\n    fmt.println(mk())\n}",
        // s4: []i64 -> []int in plain assignment
        "fn main() {\n    var a []i64\n    var c []int\n    c = a\n    fmt.println(c)\n}",
        // parallel assignment and map key width mismatch
        "fn main() {\n    var a []f32\n    var m map[i64]string\n    var c []f64\n    var n map[int]string\n    c, n = a, m\n    fmt.println(c, n)\n}",
        // return forwarding from function returning []i64 into []int
        "fn src() []i64 {\n    var a []i64\n    return a\n}\nfn dst() []int {\n    return src()\n}\nfn main() {\n    fmt.println(dst())\n}",
    ];
    for src in cases {
        let (verdict, codes) = check(src);
        assert_eq!(verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {codes:?} for:\n{src}");
    }
}

#[test]
fn rejects_mixed_width_typed_min_and_max_operands_2875() {
    let cases = [
        "fn main() {\n    var a i64 = 1\n    var b int = 2\n    fmt.println(min(a, b))\n}",
        "fn main() {\n    var a i64 = 1\n    var b int = 2\n    fmt.println(max(b, a))\n}",
        "fn main() {\n    var a f32 = 1.0\n    var b f64 = 2.0\n    fmt.println(max(a, b))\n}",
        "fn main() {\n    var a i64 = 1\n    var b int = 2\n    fmt.println(min(0, a, b))\n}",
    ];
    for src in cases {
        let (verdict, codes) = check(src);
        assert_eq!(verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {codes:?} for:\n{src}");
    }
}

#[test]
fn accepts_identical_and_aliased_composite_types_and_matching_minmax_2875() {
    let src = "\
type ints64 = []int64
type my_i64 = i64
fn take(xs []int64, m map[string]int64, p *int64, arr [2]int64) []i64 {
    return xs
}
fn main() {
    var a []i64
    var b []int64 = a
    var b2 ints64 = a
    var c []my_i64 = a
    var m1 map[string]i64
    var m2 map[string]int64 = m1
    var v i64 = 1
    p := &v
    var q *int64 = p
    var arr1 [2]i64
    var arr2 [2]int64 = arr1
    b = take(c, m2, q, arr2)
    var x int64 = 2
    var f1 f32 = 1.0
    var f2 float32 = 2.0
    fmt.println(b, b2, min(v, x, 3), max(f1, f2, 0.5))
}
";
    let (verdict, codes) = check(src);
    assert_eq!(verdict, Verdict::Accept, "expected accept, got {codes:?}");
}
