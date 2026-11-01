//! type_expr_array_len_recv_and_div_zero_3089.rs — issue #3089 (Row 13 BUILDABLE).
//!
//! 1. `check_expr_recv_operands` (`recv.rs`) must walk `TypeExpr` fields on
//!    `Expr::StructLit`, `Expr::SliceLit`, `Expr::MapLit`, and `Expr::FnLit`
//!    so send-only / non-channel receives inside array-length expressions in
//!    composite and closure literal type annotations reject (`HYB-E2008`).
//! 2. `arith.rs` must walk `TypeExpr` array-length expressions in `Stmt::Local`,
//!    `Expr::StructLit`, `Expr::SliceLit`, `Expr::MapLit`, `Expr::FnLit`, and
//!    file-level declarations so constant division/modulo by zero inside array
//!    lengths rejects (`HYB-E6001`).

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
fn composite_and_fn_literal_type_expr_send_only_recv_rejects() {
    for (name, src) in [
        (
            "slice literal element array length send-only receive",
            "fn main() {\n    tx := make(chan<- int, 1)\n    _ = [][<-tx]int{}\n}\n",
        ),
        (
            "map literal key array length send-only receive",
            "fn main() {\n    tx := make(chan<- int, 1)\n    _ = map[[<-tx]int]int{}\n}\n",
        ),
        (
            "map literal value array length send-only receive",
            "fn main() {\n    tx := make(chan<- int, 1)\n    _ = map[int][<-tx]int{}\n}\n",
        ),
        (
            "fn literal param array length send-only receive",
            "fn main() {\n    tx := make(chan<- int, 1)\n    f := fn(x [<-tx]int) { _ = x }\n    _ = f\n}\n",
        ),
        (
            "fn literal result array length non-channel receive",
            "fn main() {\n    n := 1\n    f := fn() [<-n]int { var a [1]int\n return a }\n    _ = f\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}: {:?}", r.diagnostics);
        assert!(
            error_codes(&r).contains(&"HYB-E2008".to_string()),
            "{name}: expected HYB-E2008, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn type_expr_array_len_const_zero_divisor_rejects_e6001() {
    for (name, src) in [
        (
            "local var array length 1 / 0",
            "fn main() {\n    var a [1 / 0]int\n    _ = a\n}\n",
        ),
        (
            "slice literal array length 1 / 0",
            "fn main() {\n    _ = [][1 / 0]int{}\n}\n",
        ),
        (
            "map literal key array length 1 % 0",
            "fn main() {\n    _ = map[[1 % 0]int]int{}\n}\n",
        ),
        (
            "fn literal param array length 1 / 0",
            "fn main() {\n    f := fn(x [1 / 0]int) { _ = x }\n    _ = f\n}\n",
        ),
        (
            "top-level type alias array length 1 / 0",
            "type bad_arr = [1 / 0]int\nfn main() {}\n",
        ),
        (
            "top-level struct field array length 1 / 0",
            "struct holder { buf [1 / 0]int }\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}: {:?}", r.diagnostics);
        assert!(
            error_codes(&r).contains(&"HYB-E6001".to_string()),
            "{name}: expected HYB-E6001, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn valid_composite_and_local_array_lengths_stay_accepted() {
    let src = "type ok_arr = [4 / 2]int\nfn main() {\n    var a [6 / 2]int\n    _ = a\n    _ = [][4 / 2]int{}\n    _ = map[[2]int]int{}\n    f := fn(x [2]int) { _ = x }\n    _ = f\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}
