//! issue_3516_generic_type_param_builtins.rs — regression tests for issue #3516 (Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check` must emit Go code
//! that successfully compiles under `go build`.
//!
//! `hy-check` failed to validate operand types when built-in functions (`clear`, `close`, `delete`,
//! `cap`, `len`, `append`, `copy`, `min`, `max`) were called on arguments whose type is a bare generic
//! type parameter (`t any` / `t comparable`). `hyb check` emitted 0 diagnostics (Verdict::Accept),
//! but the emitted Go program fails `go build` with fatal type-checking errors.

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
fn generic_type_parameter_builtins_reject_e2005() {
    let cases = [
        (
            "clear on bare type parameter t",
            "package main\nfn test_clear[t any](x t) {\n    clear(x)\n}\nfn main() {}\n",
        ),
        (
            "clear on local var with inferred type param t",
            "package main\nfn test_clear_local[t any](x t) {\n    y := x\n    clear(y)\n}\nfn main() {}\n",
        ),
        (
            "clear on parenthesized type param (x)",
            "package main\nfn test_clear_paren[t any](x t) {\n    clear((x))\n}\nfn main() {}\n",
        ),
        (
            "close on bare type parameter t",
            "package main\nfn test_close[t any](ch t) {\n    close(ch)\n}\nfn main() {}\n",
        ),
        (
            "delete on bare type parameter t",
            "package main\nfn test_delete[t any](m t) {\n    delete(m, \"k\")\n}\nfn main() {}\n",
        ),
        (
            "cap on bare type parameter t",
            "package main\nfn test_cap[t any](c t) int {\n    return cap(c)\n}\nfn main() {}\n",
        ),
        (
            "len on bare type parameter t",
            "package main\nfn test_len[t any](x t) int {\n    return len(x)\n}\nfn main() {}\n",
        ),
        (
            "append on bare type parameter t",
            "package main\nfn test_append[t any](s t) {\n    _ = append(s, 1)\n}\nfn main() {}\n",
        ),
        (
            "copy destination bare type parameter t",
            "package main\nfn test_copy[t any](dst t, src []int) {\n    _ = copy(dst, src)\n}\nfn main() {}\n",
        ),
        (
            "copy source bare type parameter t",
            "package main\nfn test_copy_src[t any](dst []int, src t) {\n    _ = copy(dst, src)\n}\nfn main() {}\n",
        ),
        (
            "min on bare type parameter t",
            "package main\nfn test_min[t any](a t, b t) {\n    _ = min(a, b)\n}\nfn main() {}\n",
        ),
        (
            "max on bare type parameter t",
            "package main\nfn test_max[t any](a t, b t) {\n    _ = max(a, b)\n}\nfn main() {}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "{name}: expected Verdict::Reject, got Accept. Diagnostics: {:?}",
            r.diagnostics
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E2005".to_string()),
            "{name}: expected HYB-E2005, got {:?}",
            codes
        );
    }
}

#[test]
fn concrete_generic_container_builtins_accepted() {
    let cases = [
        (
            "clear on []t slice",
            "package main\nfn test_clear_slice[t any](x []t) {\n    clear(x)\n}\nfn main() {}\n",
        ),
        (
            "clear on map[k]v",
            "package main\nfn test_clear_map[k comparable, v any](x map[k]v) {\n    clear(x)\n}\nfn main() {}\n",
        ),
        (
            "close on chan t",
            "package main\nfn test_close_chan[t any](ch chan t) {\n    close(ch)\n}\nfn main() {}\n",
        ),
        (
            "delete on map[k]v",
            "package main\nfn test_delete_map[k comparable, v any](m map[k]v, k_val k) {\n    delete(m, k_val)\n}\nfn main() {}\n",
        ),
        (
            "cap on []t slice",
            "package main\nfn test_cap_slice[t any](c []t) int {\n    return cap(c)\n}\nfn main() {}\n",
        ),
        (
            "len on []t slice",
            "package main\nfn test_len_slice[t any](x []t) int {\n    return len(x)\n}\nfn main() {}\n",
        ),
        (
            "len on map[k]v",
            "package main\nfn test_len_map[k comparable, v any](x map[k]v) int {\n    return len(x)\n}\nfn main() {}\n",
        ),
        (
            "len on chan t",
            "package main\nfn test_len_chan[t any](ch chan t) int {\n    return len(ch)\n}\nfn main() {}\n",
        ),
        (
            "append on []t slice",
            "package main\nfn test_append_slice[t any](s []t, val t) []t {\n    return append(s, val)\n}\nfn main() {}\n",
        ),
        (
            "copy on []t slices",
            "package main\nfn test_copy_slice[t any](dst []t, src []t) int {\n    return copy(dst, src)\n}\nfn main() {}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "{name}: expected Verdict::Accept, got Reject. Diagnostics: {:?}",
            r.diagnostics
        );
        let codes = error_codes(&r);
        assert!(
            codes.is_empty(),
            "{name}: expected no errors, got {:?}",
            codes
        );
    }
}
