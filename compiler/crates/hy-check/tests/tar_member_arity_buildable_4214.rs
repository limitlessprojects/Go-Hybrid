//! Regression tests for #4214 (companion to #4135 / #4238 package call-arity family):
//! The full `archive/tar` (`tar`) call surface must be arity-checked at `hyb check`
//! time against Go 1.26 standard library signatures (`go doc archive/tar`), and
//! all 28 non-callable package constants, error variables, and types (`type_reg`..
//! `type_gnu_long_link`, `err_header`..`err_insecure_path`, `format`,
//! `format_unknown`..`format_gnu`, `header`, `reader`, `writer`, `file_info_names`)
//! must reject as call targets with `HYB-E1002` (Row 13 BUILDABLE).

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
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "verdict for {:?}: {:?}",
        src,
        r.diagnostics
    );
}

#[test]
fn tar_one_arg_members_accept_exact_arity() {
    accepts(
        "import \"archive/tar\"\n\
         import \"io\"\n\
         fn read_archive(r io.reader) {\n\
             tr := tar.new_reader(r)\n\
             _ = tr\n\
         }\n\
         fn main() {}\n",
    );
    accepts(
        "import \"archive/tar\"\n\
         import \"io\"\n\
         fn write_archive(w io.writer) {\n\
             tw := tar.new_writer(w)\n\
             _ = tw\n\
         }\n\
         fn main() {}\n",
    );
}

#[test]
fn tar_two_arg_member_accepts_exact_arity() {
    accepts(
        "import \"archive/tar\"\n\
         fn make_header(fi os.file_info) error {\n\
             h, err := tar.file_info_header(fi, \"\")\n\
             h2 := tar.file_info_header(fi, \"target\")?\n\
             _ = h\n\
             _ = err\n\
             _ = h2\n\
             return nil\n\
         }\n\
         fn main() {}\n",
    );
}

#[test]
fn tar_one_arg_members_reject_wrong_arity() {
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             tr := tar.new_reader()\n\
             _ = tr\n\
         }\n",
    );
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             tr := tar.new_reader(nil, nil)\n\
             _ = tr\n\
         }\n",
    );
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             tw := tar.new_writer()\n\
             _ = tw\n\
         }\n",
    );
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             tw := tar.new_writer(nil, nil)\n\
             _ = tw\n\
         }\n",
    );
}

#[test]
fn tar_two_arg_member_rejects_wrong_arity() {
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             h, err := tar.file_info_header()\n\
             _ = h\n\
             _ = err\n\
         }\n",
    );
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             h, err := tar.file_info_header(nil)\n\
             _ = h\n\
             _ = err\n\
         }\n",
    );
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             h, err := tar.file_info_header(nil, \"\", \"extra\")\n\
             _ = h\n\
             _ = err\n\
         }\n",
    );
}

#[test]
fn tar_package_constants_errors_and_types_are_not_callable() {
    for member in [
        "type_reg",
        "type_reg_a",
        "type_link",
        "type_symlink",
        "type_char",
        "type_block",
        "type_dir",
        "type_fifo",
        "type_cont",
        "type_x_header",
        "type_x_global_header",
        "type_gnu_sparse",
        "type_gnu_long_name",
        "type_gnu_long_link",
        "err_header",
        "err_write_too_long",
        "err_field_too_long",
        "err_write_after_close",
        "err_insecure_path",
        "format",
        "format_unknown",
        "format_ustar",
        "format_pax",
        "format_gnu",
        "header",
        "reader",
        "writer",
        "file_info_names",
    ] {
        rejects_e1002(&format!(
            "import \"archive/tar\"\nfn main() {{\n    _ = tar.{member}()\n}}\n"
        ));
        rejects_e1002(&format!(
            "import \"archive/tar\"\nfn main() {{\n    _ = tar.{member}(1)\n}}\n"
        ));
    }

    // Value-position references to all type flag constants, format constants, and
    // error variables accept cleanly.
    accepts(
        "import \"archive/tar\"\n\
         fn main() {\n\
             _ = tar.type_reg\n\
             _ = tar.type_reg_a\n\
             _ = tar.type_link\n\
             _ = tar.type_symlink\n\
             _ = tar.type_char\n\
             _ = tar.type_block\n\
             _ = tar.type_dir\n\
             _ = tar.type_fifo\n\
             _ = tar.type_cont\n\
             _ = tar.type_x_header\n\
             _ = tar.type_x_global_header\n\
             _ = tar.type_gnu_sparse\n\
             _ = tar.type_gnu_long_name\n\
             _ = tar.type_gnu_long_link\n\
             _ = tar.err_header\n\
             _ = tar.err_write_too_long\n\
             _ = tar.err_field_too_long\n\
             _ = tar.err_write_after_close\n\
             _ = tar.err_insecure_path\n\
             _ = tar.format_unknown\n\
             _ = tar.format_ustar\n\
             _ = tar.format_pax\n\
             _ = tar.format_gnu\n\
         }\n",
    );
}

#[test]
fn tar_spread_is_rejected_on_non_variadic_calls() {
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             xs := []int{1}\n\
             _ = tar.new_reader(xs...)\n\
         }\n",
    );
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             xs := []int{1}\n\
             _ = tar.new_writer(xs...)\n\
         }\n",
    );
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             xs := []string{\"a\", \"b\"}\n\
             _, _ = tar.file_info_header(xs...)\n\
         }\n",
    );
}

#[test]
fn tar_arity_error_message_is_actionable() {
    let r = check(
        "import \"archive/tar\"\n\
         fn main() {\n\
             h, err := tar.file_info_header(nil)\n\
             _ = h\n\
             _ = err\n\
         }\n",
    );
    let msg = format!("{:?}", r.diagnostics);
    assert!(
        msg.contains("takes exactly 2 arguments"),
        "message must state expected arity: {}",
        msg
    );
    assert!(
        msg.contains("got 1"),
        "message must state actual count: {}",
        msg
    );
    assert!(
        msg.contains("FileInfoHeader"),
        "message must name the Go lowering: {}",
        msg
    );
}
