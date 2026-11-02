//! issue_3111_generic_and_slice_scalar_widths.rs — tests for HYB-E1002 rejection
//! of escaped scalar width checks through generic functions, generic struct
//! methods/fields, and slice expressions (fixes #3111).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn generic_fn_call_results_reject_e1002() {
    let cases = [
        // t11: generic call inferred int8 assigned to int64
        "fn id[t any](v t) t { return v }\nfn main() {\n    var a int8 = 1\n    var y int64 = id(a)\n    _ = y\n}\n",
        // t21: generic call explicit int8 assigned to int64
        "fn id[t any](v t) t { return v }\nfn main() {\n    var a int8 = 1\n    var y int64 = id[int8](a)\n    _ = y\n}\n",
        // t22: generic slice helper inferred int8 assigned to int64
        "fn first[t any](xs []t) t { return xs[0] }\nfn main() {\n    xs := []int8{1}\n    var y int64 = first(xs)\n    _ = y\n}\n",
        // t25: generic call inferred int32 passed to int64 parameter
        "fn id[t any](v t) t { return v }\nfn take(v int64) int64 { return v }\nfn main() {\n    var a int32 = 1\n    _ = take(id(a))\n}\n",
        // t27: untyped integer 3 infers int in Go, assigned to int64
        "fn id[t any](v t) t { return v }\nfn main() {\n    var e int64 = id(3)\n    _ = e\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn generic_struct_methods_and_fields_reject_e1002() {
    let cases = [
        // t1: generic struct method get() int8 assigned to int64
        "struct box[t any] { v t }\nfn (b box[t]) get() t { return b.v }\nfn main() {\n    b := box[int8]{v: 1}\n    var y int64 = b.get()\n    _ = y\n}\n",
        // t26: generic struct method get() int8 returned from int64 function
        "struct box[t any] { v t }\nfn (b box[t]) get() t { return b.v }\nfn widen() int64 {\n    b := box[int8]{v: 1}\n    return b.get()\n}\nfn main() { _ = widen() }\n",
        // t15: generic struct field v int8 assigned to int64
        "struct box[t any] { v t }\nfn main() {\n    b := box[int8]{v: 1}\n    var y int64 = b.v\n    _ = y\n}\n",
        // t23: generic struct field v uint16 bound to local x, assigned to int
        "struct box[t any] { v t }\nfn main() {\n    b := box[uint16]{v: 1}\n    x := b.v\n    var y int = x\n    _ = y\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn slice_expressions_reject_e1002() {
    let cases = [
        // t20: slice expression of [2]int8 produces []int8, indexing yields int8, assigned to int64
        "fn main() {\n    arr := [2]int8{1, 2}\n    s := arr[0:1]\n    var y int64 = s[0]\n    _ = y\n}\n",
        // t24: slice expression of []int8 produces []int8, indexing yields int8, assigned to int64
        "fn main() {\n    var xs []int8 = []int8{1, 2}\n    s := xs[1:]\n    var y int64 = s[0]\n    _ = y\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn valid_control_accepts() {
    let valid = r#"
struct box[t any] { v t }
fn (b box[t]) get() t { return b.v }
fn id[t any](v t) t { return v }
fn first[t any](xs []t) t { return xs[0] }
fn main() {
    b := box[int8]{v: 1}
    var a int8 = b.get()
    var c int8 = b.v
    var d int8 = id(a)
    var e int64 = id[int64](3)
    xs := []int8{1, 2}
    s := xs[1:]
    var f int8 = s[0]
    var g int8 = first(xs)
    var h int64 = int64(id(a))
    _ = a
    _ = c
    _ = d
    _ = e
    _ = f
    _ = g
    _ = h
}
"#;
    let r = check(valid);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid program must pass check, got: {:?}",
        r.diagnostics
    );
    assert_eq!(error_codes(&r).len(), 0);
}
