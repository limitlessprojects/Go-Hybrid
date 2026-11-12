use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn rejects_enum_named_default_or_none_with_hyb_e1002() {
    let src_default = r#"
enum default {
    idle
    active
}

fn main() {
}
"#;
    let parsed = parse_file("repro_enum_default.hy", src_default).expect("parse should succeed");
    let report = check_file(&parsed);
    assert_eq!(report.verdict, Verdict::Reject);
    assert!(
        report.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for enum named default, got: {:?}",
        report.diagnostics
    );

    let src_none = r#"
enum none {
    idle
}

fn main() {
}
"#;
    let parsed_none = parse_file("repro_enum_none.hy", src_none).expect("parse should succeed");
    let report_none = check_file(&parsed_none);
    assert_eq!(report_none.verdict, Verdict::Reject);
    assert!(
        report_none.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for enum named none, got: {:?}",
        report_none.diagnostics
    );
}

#[test]
fn rejects_enum_variant_payload_fn_param_named_default() {
    let src = r#"
enum action {
    cb(fn(default int))
}

fn main() {
}
"#;
    let parsed = parse_file("repro_enum_payload_kw.hy", src).expect("parse should succeed");
    let report = check_file(&parsed);
    assert_eq!(report.verdict, Verdict::Reject);
    assert!(
        report.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for enum payload fn param named default, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn accepts_valid_enum_declaration_control() {
    let src_ok = r#"
enum status {
    idle
    active
}

fn main() {
}
"#;
    let parsed_ok = parse_file("control_enum_ok.hy", src_ok).expect("parse should succeed");
    let report_ok = check_file(&parsed_ok);
    assert_eq!(report_ok.verdict, Verdict::Accept);
    assert!(report_ok.diagnostics.is_empty());
}
