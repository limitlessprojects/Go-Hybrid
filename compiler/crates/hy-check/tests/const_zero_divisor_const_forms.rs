//! #758 — named/derived constant zero divisors (HYB-E6001).
//!
//! Go folds constant expressions BEFORE deciding `invalid operation:
//! division by zero`, so `10 / zero` (`const zero = 0`) and
//! `1.0 / (1.0 - 1.0)` are compile errors. `hyb check` accepted them with
//! 0 diagnostics because `const_eval_int` / `is_float_literal_zero` only
//! inspected inline literals. The fix makes the divisor predicate
//! const-map-aware (#998 pattern) and mirrors the SAME predicate onto the
//! array-length site (#999 pattern). Runtime divisors and shadowed
//! bindings stay legal (fail-open outside the constant world).

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

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn named_const_int_zero_divisor_rejects() {
    assert_code(
        "const zero = 0\n\nfn main() {\n    x := 10 / zero\n    _ = x\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "const zero = 0\n\nfn main() {\n    x := 10 % zero\n    _ = x\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "const a = 3\nconst b = a - 3\n\nfn main() {\n    x := 10\n    x /= b\n    _ = x\n}\n",
        "HYB-E6001",
    );
}

#[test]
fn folded_float_zero_divisor_rejects() {
    assert_code(
        "fn main() {\n    _ = 1.0 / (1.0 - 1.0)\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\n    _ = 1.0 / (0.0 * 5.0)\n}\n",
        "HYB-E6001",
    );
}

#[test]
fn named_const_float_zero_divisor_rejects() {
    assert_code(
        "const f_val = 1.0\nconst fz = 0.0\n\nfn main() {\n    _ = f_val / fz\n}\n",
        "HYB-E6001",
    );
}

#[test]
fn conversion_zero_divisor_rejects() {
    assert_code(
        "fn main() {\n    x := 10 / int(0)\n    _ = x\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\n    x := 10 % int64(1 - 1)\n    _ = x\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\n    _ = 1.0 / float64(0)\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\n    _ = 1.0 / f64(1 - 1)\n}\n",
        "HYB-E6001",
    );
}

#[test]
fn const_string_len_zero_divisor_rejects() {
    assert_code(
        "fn main() {\n    y := 10 / (len(\"abc\") - 3)\n    _ = y\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "const s_val = \"abc\"\n\nfn main() {\n    x := 10 % (len(s_val) - 3)\n    _ = x\n}\n",
        "HYB-E6001",
    );
}

#[test]
fn array_len_zero_divisor_rejects() {
    assert_code(
        "fn main() {\n    var a [1 / 0]int\n    _ = a\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "type arr = [1 / 0]int\n\nfn main() {\n    _ = 1\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "struct s { a [1 % 0]int }\n\nfn main() {\n    _ = 1\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "fn f(a [1 / 0]int) {\n    _ = a\n}\n\nfn main() {\n    _ = 1\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\n    _ = [1 / 0]int{}\n}\n",
        "HYB-E6001",
    );
}

#[test]
fn runtime_and_shadowed_divisors_stay_legal() {
    // runtime divisor: panics at run time, Go accepts the program
    assert_ok("fn main() {\n    var n int = 0\n    x := 10 / n\n    _ = x\n}\n");
    // a local shadowing the file const is a RUNTIME binding (issue control)
    assert_ok(
        "const zero = 0\n\nfn main() {\n    zero := 2\n    x := 10 / zero\n    _ = x\n}\n",
    );
    // non-zero constants divide fine
    assert_ok("const k = 5\n\nfn main() {\n    x := 10 / k\n    _ = x\n}\n");
    // in-range integer division of constants
    assert_ok("fn main() {\n    x := 10 / 2\n    _ = x\n}\n");
    // len of a runtime string is not a constant
    assert_ok("fn main() {\n    s := \"abc\"\n    x := 10 / (len(s) - 3)\n    _ = x\n}\n");
    // legal array lengths, incl. a zero length
    assert_ok("fn main() {\n    var a [0]int\n    var b [4 / 2]int\n    _ = a\n    _ = b\n}\n");
}
