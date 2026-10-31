//! assign_float_from_int_var_soundness.rs — regression tests for Issue #2551:
//! verification that non-constant integer variables assigned to float64 targets
//! in assignment statements, struct literals, and return statements are rejected
//! with HYB-E1002 while untyped integer constants remain cleanly accepted.

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
fn int_var_assigned_to_float_target_rejected_with_e1002() {
    // 1. Plain assignment: f = i
    let src_assign = "fn main() {\n    var f float64\n    var i int = 1\n    f = i\n    _ = f\n}\n";
    let r_assign = check(src_assign);
    assert_eq!(r_assign.verdict, Verdict::Reject);
    assert!(error_codes(&r_assign).contains(&"HYB-E1002".to_string()));

    // 2. Parallel assignment: f, s = i, \"text\"
    let src_parallel = "fn main() {\n    var f float64\n    var s string\n    var i int = 1\n    f, s = i, \"text\"\n    _ = f\n    _ = s\n}\n";
    let r_parallel = check(src_parallel);
    assert_eq!(r_parallel.verdict, Verdict::Reject);
    assert!(error_codes(&r_parallel).contains(&"HYB-E1002".to_string()));

    // 3. Struct literal field: my_config{ratio: i}
    let src_struct = "struct my_config {\n    ratio float64\n}\nfn main() {\n    var i int = 1\n    c := my_config{ratio: i}\n    _ = c\n}\n";
    let r_struct = check(src_struct);
    assert_eq!(r_struct.verdict, Verdict::Reject);
    assert!(error_codes(&r_struct).contains(&"HYB-E1002".to_string()));

    // 4. Return statement: return i in fn() float64
    let src_ret = "fn get_ratio() float64 {\n    var i int = 1\n    return i\n}\n";
    let r_ret = check(src_ret);
    assert_eq!(r_ret.verdict, Verdict::Reject);
    assert!(error_codes(&r_ret).contains(&"HYB-E1002".to_string()));
}

#[test]
fn untyped_int_constants_to_float_targets_stay_accepted() {
    // 1. Plain assignment: f = 1
    let src_assign_const = "fn main() {\n    var f float64\n    f = 1\n    _ = f\n}\n";
    let r_assign_const = check(src_assign_const);
    assert_eq!(r_assign_const.verdict, Verdict::Accept);

    // 2. Struct literal field with constant: my_config{ratio: 1}
    let src_struct_const = "struct my_config {\n    ratio float64\n}\nfn main() {\n    c := my_config{ratio: 1}\n    _ = c\n}\n";
    let r_struct_const = check(src_struct_const);
    assert_eq!(r_struct_const.verdict, Verdict::Accept);

    // 3. Return statement with constant: return 1 in fn() float64
    let src_ret_const = "fn get_ratio() float64 {\n    return 1\n}\n";
    let r_ret_const = check(src_ret_const);
    assert_eq!(r_ret_const.verdict, Verdict::Accept);
}
