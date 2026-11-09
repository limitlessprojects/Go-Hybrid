//! Tests for curated foreign method surface of `flag.FlagSet` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invoking phantom/unsupported methods on `flag.flag_set` or `*flag.flag_set`
//! rejects fail-closed with HYB-E1002, while standard Go methods (`parse`, `args`, `n_arg`, `set`)
//! accept cleanly with 0 diagnostics.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_flag_set_rejected() {
    let bad = check(
        "fn handle(fs flag.flag_set) {\n\
             fs.totally_fake_flag_method()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        all_codes
    );
    let msg = &bad.diagnostics[0].message;
    assert!(msg.contains("totally_fake_flag_method"), "message: {msg}");
    assert!(msg.contains("flag.flag_set"), "message: {msg}");
}

#[test]
fn phantom_flag_set_pointer_receiver_rejected() {
    let bad = check(
        "fn run(fs *flag.flag_set) {\n\
             fs.bogus_flag_call()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        all_codes
    );
    let msg = &bad.diagnostics[0].message;
    assert!(msg.contains("bogus_flag_call"), "message: {msg}");
    assert!(msg.contains("flag.flag_set"), "message: {msg}");
}

#[test]
fn real_flag_set_methods_accept() {
    let good = check(
        "fn setup(fs *flag.flag_set) {\n\
             _ = fs.parse([]string{})\n\
             _ = fs.parsed()\n\
             _ = fs.args()\n\
             _ = fs.n_arg()\n\
             _ = fs.name()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
    assert!(good.diagnostics.is_empty(), "expected 0 diagnostics, got {:?}", good.diagnostics);
}
