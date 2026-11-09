//! Soundness and buildable validation tests for `sync/atomic` (`atomic`) calls under Row 13 (BUILDABLE):
//! 1. Non-callable package types (`atomic.bool`, `atomic.int32`, `atomic.int64`, `atomic.uint32`,
//!    `atomic.uint64`, `atomic.uintptr`, `atomic.pointer`, `atomic.value`) must be rejected
//!    when invoked as call targets (`HYB-E1002`).
//! 2. `atomic` functions require exact arity:
//!    - 1 arg: load_int32, load_int64, load_uint32, load_uint64, load_uintptr, load_pointer.
//!    - 2 args: store_int32, store_int64, store_uint32, store_uint64, store_uintptr, store_pointer,
//!             add_int32, add_int64, add_uint32, add_uint64, add_uintptr,
//!             swap_int32, swap_int64, swap_uint32, swap_uint64, swap_uintptr, swap_pointer.
//!    - 3 args: compare_and_swap_int32, compare_and_swap_int64, compare_and_swap_uint32,
//!             compare_and_swap_uint64, compare_and_swap_uintptr, compare_and_swap_pointer.
//! 3. Arity mismatches must reject fail-closed with `HYB-E1002`.
//! 4. Positive controls: legal calls with proper arity accept cleanly.
//!
//! Settlement Wallet (Base L2 USDC): 0x8366bCe3a2D379De7656D7A67015789FaF999f20

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn test_atomic_non_callable_types_reject() {
    let cases = [
        "fn main() {\n    _ = atomic.bool()\n}",
        "fn main() {\n    _ = atomic.int32()\n}",
        "fn main() {\n    _ = atomic.int64()\n}",
        "fn main() {\n    _ = atomic.uint32()\n}",
        "fn main() {\n    _ = atomic.uint64()\n}",
        "fn main() {\n    _ = atomic.uintptr()\n}",
        "fn main() {\n    _ = atomic.pointer()\n}",
        "fn main() {\n    _ = atomic.value()\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_atomic_one_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = atomic.load_int32()\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.load_int32(&x, 1)\n}",
        "fn main() {\n    _ = atomic.load_int64()\n}",
        "fn main() {\n    var x int64 = 0\n    _ = atomic.load_int64(&x, 1)\n}",
        "fn main() {\n    _ = atomic.load_uint32()\n}",
        "fn main() {\n    var x uint32 = 0\n    _ = atomic.load_uint32(&x, 1)\n}",
        "fn main() {\n    _ = atomic.load_uint64()\n}",
        "fn main() {\n    var x uint64 = 0\n    _ = atomic.load_uint64(&x, 1)\n}",
        "fn main() {\n    _ = atomic.load_uintptr()\n}",
        "fn main() {\n    var x uintptr = 0\n    _ = atomic.load_uintptr(&x, 1)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_atomic_two_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = atomic.store_int32()\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.store_int32(&x)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.store_int32(&x, 1, 2)\n}",
        "fn main() {\n    _ = atomic.add_int32()\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.add_int32(&x)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.add_int32(&x, 1, 2)\n}",
        "fn main() {\n    _ = atomic.swap_int32()\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.swap_int32(&x)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.swap_int32(&x, 1, 2)\n}",
        "fn main() {\n    _ = atomic.add_int64()\n}",
        "fn main() {\n    var x int64 = 0\n    _ = atomic.add_int64(&x)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_atomic_three_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = atomic.compare_and_swap_int32()\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.compare_and_swap_int32(&x)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.compare_and_swap_int32(&x, 0)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.compare_and_swap_int32(&x, 0, 1, 2)\n}",
        "fn main() {\n    _ = atomic.compare_and_swap_int64()\n}",
        "fn main() {\n    var x int64 = 0\n    _ = atomic.compare_and_swap_int64(&x, 0)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_atomic_positive_controls_accept() {
    let cases = [
        "fn main() {\n    var x int32 = 0\n    _ = atomic.load_int32(&x)\n}",
        "fn main() {\n    var x int64 = 0\n    _ = atomic.load_int64(&x)\n}",
        "fn main() {\n    var x int32 = 0\n    atomic.store_int32(&x, 42)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.add_int32(&x, 1)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.swap_int32(&x, 10)\n}",
        "fn main() {\n    var x int32 = 0\n    _ = atomic.compare_and_swap_int32(&x, 0, 1)\n}",
        "fn main() {\n    var x int64 = 0\n    _ = atomic.compare_and_swap_int64(&x, 0, 1)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for: {}", src);
        assert!(r.diagnostics.is_empty(), "unexpected diagnostics: {:?}", r.diagnostics);
    }
}
