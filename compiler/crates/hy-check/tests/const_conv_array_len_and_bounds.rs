//! Regression tests for constant integer conversion wrapping in `const_int_value_at`,
//! `f32`/`f64`/alias array-length rejection, and `f32`/`f64` receiver & generic method
//! redeclaration guards in `emit_guard.rs`.

use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("test snippet must parse");
    check_file(&file)
}

fn assert_rejects_with(src: &str, code: &str) {
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected reject with {code}, got accept:\n{src}"
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == code),
        "expected diagnostic {code}, got {:?} for:\n{src}",
        res.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected accept, got {:?} for:\n{src}",
        res.diagnostics
    );
}

#[test]
fn const_int_conversion_wraps_to_negative_array_len_e6004() {
    assert_rejects_with(
        "import \"fmt\"\nconst k = 200\nfn main() {\n    var a [i8(k)]int\n    fmt.println(len(a))\n}\n",
        "HYB-E6004",
    );
    assert_rejects_with(
        "import \"fmt\"\nconst k = 40000\nfn main() {\n    var a [i16(k)]int\n    fmt.println(len(a))\n}\n",
        "HYB-E6004",
    );
    assert_rejects_with(
        "import \"fmt\"\nconst k = 200\nconst n = i8(k)\nfn main() {\n    var a [n]int\n    fmt.println(len(a))\n}\n",
        "HYB-E6004",
    );
    assert_rejects_with(
        "import \"fmt\"\ntype my_i8 = i8\nfn main() {\n    var a [my_i8(200)]int\n    fmt.println(len(a))\n}\n",
        "HYB-E6004",
    );
}

#[test]
fn const_uint_conversion_wraps_array_lit_capacity_e6004() {
    assert_rejects_with(
        "import \"fmt\"\nconst k = 256\nfn main() {\n    a := [u8(k)]int{1}\n    fmt.println(len(a))\n}\n",
        "HYB-E6004",
    );
}

#[test]
fn const_int_conversion_wraps_index_and_slice_bounds_e1002() {
    assert_rejects_with(
        "import \"fmt\"\nconst k = 200\nfn main() {\n    xs := []int{1, 2, 3, 4}\n    fmt.println(xs[i8(k)])\n}\n",
        "HYB-E1002",
    );
    assert_rejects_with(
        "import \"fmt\"\nconst k = 200\nfn main() {\n    xs := []int{1, 2, 3, 4}\n    fmt.println(xs[0:i8(k)])\n}\n",
        "HYB-E1002",
    );
    assert_rejects_with(
        "import \"fmt\"\nconst big = 257\nfn main() {\n    xs := []int{1, 2, 3, 4}\n    fmt.println(xs[2:u8(big)])\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn f32_f64_and_alias_typed_consts_and_conversions_rejected_as_array_len_e6004() {
    assert_rejects_with(
        "import \"fmt\"\nconst n f64 = 2.0\nfn main() {\n    var a [n]int\n    fmt.println(len(a))\n}\n",
        "HYB-E6004",
    );
    assert_rejects_with(
        "import \"fmt\"\nconst m f32 = 2.0\nfn main() {\n    var b [m]int\n    fmt.println(len(b))\n}\n",
        "HYB-E6004",
    );
    assert_rejects_with(
        "import \"fmt\"\ntype my_float = float64\nconst k my_float = 2\nfn main() {\n    var c [k]int\n    fmt.println(len(c))\n}\n",
        "HYB-E6004",
    );
    assert_rejects_with(
        "import \"fmt\"\nfn main() {\n    var a [f64(2.5)]int\n    fmt.println(len(a))\n}\n",
        "HYB-E6004",
    );
    assert_rejects_with(
        "import \"fmt\"\nfn main() {\n    var b [f32(2.5)]int\n    fmt.println(len(b))\n}\n",
        "HYB-E6004",
    );
}

#[test]
fn f32_f64_any_primitive_receivers_rejected_e2007() {
    assert_rejects_with(
        "fn (x f64) m() int {\n    return 0\n}\nfn main() {\n}\n",
        "HYB-E2007",
    );
    assert_rejects_with(
        "fn (x f32) m() int {\n    return 0\n}\nfn main() {\n}\n",
        "HYB-E2007",
    );
    assert_rejects_with(
        "fn (x any) m() int {\n    return 0\n}\nfn main() {\n}\n",
        "HYB-E2007",
    );
}

#[test]
fn generic_receiver_duplicate_method_with_renamed_type_param_rejected_e1003() {
    assert_rejects_with(
        "struct box[t] {\n    v t\n}\nfn (b box[t]) get() int {\n    return 0\n}\nfn (b box[u]) get() int {\n    return 1\n}\nfn main() {\n}\n",
        "HYB-E1003",
    );
}

#[test]
fn valid_wrapped_array_len_and_distinct_generic_methods_accepted() {
    assert_accepts(
        "import \"fmt\"\nconst big = 257\nfn main() {\n    a := [u8(big)]int{42}\n    fmt.println(a[0])\n}\n",
    );
    assert_accepts(
        "import \"fmt\"\nconst ten = 10\nfn main() {\n    var a [i8(ten)]int\n    fmt.println(len(a))\n}\n",
    );
    assert_accepts(
        "struct box[t] {\n    v t\n}\nfn (b box[t]) first() int {\n    return 0\n}\nfn (b box[u]) second() int {\n    return 1\n}\nfn main() {\n}\n",
    );
}
