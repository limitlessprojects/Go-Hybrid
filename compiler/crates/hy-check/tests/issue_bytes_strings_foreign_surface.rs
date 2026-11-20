//! Tests for curated method surfaces of `bytes.buffer` and `strings.builder`.
//! Guarantees that calling phantom/unsupported methods rejects fail-closed with HYB-E1002 (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_bytes_buffer_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             buf bytes.buffer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.buf.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn pointer_bytes_buffer_phantom_method_is_rejected() {
    let bad = check(
        "struct holder {\n\
             buf *bytes.buffer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.buf.unknown_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn real_bytes_buffer_methods_accept() {
    let good = check(
        "struct holder {\n\
             buf bytes.buffer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.buf.reset()\n\
             n := h.buf.len()\n\
             s := h.buf.string()\n\
             fmt.println(\"len={n} s={s}\")\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "real methods must accept, got {:?}", good.diagnostics);
}

#[test]
fn phantom_strings_builder_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             sb strings.builder\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.sb.totally_fake_call()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn real_strings_builder_methods_accept() {
    let good = check(
        "struct holder {\n\
             sb strings.builder\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.sb.reset()\n\
             n := h.sb.len()\n\
             s := h.sb.string()\n\
             fmt.println(\"len={n} s={s}\")\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "real methods must accept, got {:?}", good.diagnostics);
}
