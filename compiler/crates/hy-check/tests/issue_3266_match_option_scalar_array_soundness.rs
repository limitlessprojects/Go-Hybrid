use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    res.diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

fn check_verdict(src: &str) -> Verdict {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast).verdict
}

#[test]
fn match_option_scalar_width_mismatch_in_var_decl_rejected() {
    let src = r#"
fn test_match_scalar_decl(opt option[int64]) {
    match opt {
        some(v) => {
            var target int32 = v
            _ = target
        }
        none => {}
    }
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning int64 option payload to int32 target in decl, got: {:?}",
        codes
    );
}

#[test]
fn match_option_scalar_width_mismatch_in_assignment_rejected() {
    let src = r#"
fn test_match_scalar_assign(opt option[int64]) {
    var target int32
    match opt {
        some(v) => {
            target = v
        }
        none => {}
    }
    _ = target
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning int64 option payload to int32 target in assign, got: {:?}",
        codes
    );
}

#[test]
fn match_option_array_len_mismatch_in_var_decl_rejected() {
    let src = r#"
fn test_match_array_decl(opt option[[3]int]) {
    match opt {
        some(v) => {
            var target [2]int = v
            _ = target
        }
        none => {}
    }
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning [3]int option payload to [2]int target in decl, got: {:?}",
        codes
    );
}

#[test]
fn match_option_array_len_mismatch_in_assignment_rejected() {
    let src = r#"
fn test_match_array_assign(opt option[[3]int]) {
    var target [2]int
    match opt {
        some(v) => {
            target = v
        }
        none => {}
    }
    _ = target
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning [3]int option payload to [2]int target in assign, got: {:?}",
        codes
    );
}

#[test]
fn match_option_float_scalar_mismatch_rejected() {
    let src = r#"
fn test_match_float_decl(opt option[float64]) {
    match opt {
        some(v) => {
            var target float32 = v
            _ = target
        }
        none => {}
    }
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning float64 option payload to float32 target, got: {:?}",
        codes
    );
}

#[test]
fn match_option_matching_types_accepted() {
    let src = r#"
fn test_matching_scalar(opt option[int64]) {
    var target int64
    match opt {
        some(v) => {
            var d int64 = v
            target = v
            _ = d
        }
        none => {}
    }
    _ = target
}

fn test_matching_array(opt option[[3]int]) {
    var target [3]int
    match opt {
        some(v) => {
            var d [3]int = v
            target = v
            _ = d
        }
        none => {}
    }
    _ = target
}

fn test_matching_float(opt option[float64]) {
    var target float64
    match opt {
        some(v) => {
            var d float64 = v
            target = v
            _ = d
        }
        none => {}
    }
    _ = target
}
"#;
    assert_eq!(check_verdict(src), Verdict::Accept);
    assert!(error_codes(src).is_empty());
}
