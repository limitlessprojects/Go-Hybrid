//! Regression tests for #4212 (companion to #4135 / #4238 package call-arity family):
//! The full `compress/gzip` (`gzip`) call surface must be arity-checked at `hyb check`
//! time against Go 1.26 standard library signatures (`go doc compress/gzip`), and
//! non-callable package compression constants, error variables, and struct types
//! (`no_compression`, `best_speed`, `best_compression`, `default_compression`,
//! `huffman_only`, `err_checksum`, `err_header`, `reader`, `writer`, `header`)
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
fn gzip_one_arg_members_accept_exact_arity() {
    accepts(
        "import \"compress/gzip\"\n\
         import \"io\"\n\
         fn read_stream(r io.reader) {\n\
             zr, err := gzip.new_reader(r)\n\
             _ = zr\n\
             _ = err\n\
         }\n\
         fn main() {}\n",
    );
    accepts(
        "import \"compress/gzip\"\n\
         import \"io\"\n\
         fn write_stream(w io.writer) {\n\
             zw := gzip.new_writer(w)\n\
             _ = zw\n\
         }\n\
         fn main() {}\n",
    );
}

#[test]
fn gzip_two_arg_member_accepts_exact_arity() {
    accepts(
        "import \"compress/gzip\"\n\
         import \"io\"\n\
         fn write_level(w io.writer) {\n\
             zw, err := gzip.new_writer_level(w, gzip.best_speed)\n\
             zw2, err2 := gzip.new_writer_level(w, gzip.default_compression)\n\
             _ = zw\n\
             _ = err\n\
             _ = zw2\n\
             _ = err2\n\
         }\n\
         fn main() {}\n",
    );
}

#[test]
fn gzip_one_arg_members_reject_wrong_arity() {
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zr, err := gzip.new_reader()\n\
             _ = zr\n\
             _ = err\n\
         }\n",
    );
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zr, err := gzip.new_reader(nil, nil)\n\
             _ = zr\n\
             _ = err\n\
         }\n",
    );
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zw := gzip.new_writer()\n\
             _ = zw\n\
         }\n",
    );
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zw := gzip.new_writer(nil, nil)\n\
             _ = zw\n\
         }\n",
    );
}

#[test]
fn gzip_two_arg_member_rejects_wrong_arity() {
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zw, err := gzip.new_writer_level()\n\
             _ = zw\n\
             _ = err\n\
         }\n",
    );
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zw, err := gzip.new_writer_level(nil)\n\
             _ = zw\n\
             _ = err\n\
         }\n",
    );
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zw, err := gzip.new_writer_level(nil, 1, 2)\n\
             _ = zw\n\
             _ = err\n\
         }\n",
    );
}

#[test]
fn gzip_package_constants_errors_and_types_are_not_callable() {
    for member in [
        "no_compression",
        "best_speed",
        "best_compression",
        "default_compression",
        "huffman_only",
        "err_checksum",
        "err_header",
        "reader",
        "writer",
        "header",
    ] {
        rejects_e1002(&format!(
            "import \"compress/gzip\"\nfn main() {{\n    _ = gzip.{member}()\n}}\n"
        ));
        rejects_e1002(&format!(
            "import \"compress/gzip\"\nfn main() {{\n    _ = gzip.{member}(1)\n}}\n"
        ));
    }

    // Value-position references to compression constants and error variables accept cleanly.
    accepts(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             _ = gzip.no_compression\n\
             _ = gzip.best_speed\n\
             _ = gzip.best_compression\n\
             _ = gzip.default_compression\n\
             _ = gzip.huffman_only\n\
             _ = gzip.err_checksum\n\
             _ = gzip.err_header\n\
         }\n",
    );
}

#[test]
fn gzip_spread_is_rejected_on_non_variadic_calls() {
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             xs := []int{1}\n\
             _ = gzip.new_reader(xs...)\n\
         }\n",
    );
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             xs := []int{1}\n\
             _ = gzip.new_writer(xs...)\n\
         }\n",
    );
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             xs := []int{1, 2}\n\
             _ = gzip.new_writer_level(xs...)\n\
         }\n",
    );
}

#[test]
fn gzip_arity_error_message_is_actionable() {
    let r = check(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             zw, err := gzip.new_writer_level(nil)\n\
             _ = zw\n\
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
        msg.contains("NewWriterLevel"),
        "message must name the Go lowering: {}",
        msg
    );
}
