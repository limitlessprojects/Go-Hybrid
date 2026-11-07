use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("const_len.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

#[test]
fn rejects_exclusive_range_with_bare_const_len_in_untyped_hi_and_u8_lo() {
    let src = r#"
import "fmt"

const MSG = "hello world"

fn main() {
    for i in u8(0)..len(MSG) {
        fmt.println("{i}")
    }
}
"#;
    let rep = check(src);
    assert_eq!(rep.verdict, Verdict::Reject, "expected HYB-E2009 rejection");
    assert!(
        rep.diagnostics.iter().any(|d| d.code == "HYB-E2009"),
        "expected HYB-E2009 diagnostic, got: {:?}",
        rep.diagnostics
    );
}

#[test]
fn allows_exclusive_range_with_bare_const_len_and_untyped_lo() {
    let src = r#"
import "fmt"

const MSG = "hello world"

fn main() {
    for i in 0..len(MSG) {
        fmt.println("{i}")
    }
}
"#;
    let rep = check(src);
    assert_eq!(
        rep.verdict,
        Verdict::Accept,
        "unexpected diagnostics: {:?}",
        rep.diagnostics
    );
}

#[test]
fn allows_exclusive_range_with_bare_const_len_and_int_lo() {
    let src = r#"
import "fmt"

const MSG = "hello world"

fn main() {
    for i in int(0)..len(MSG) {
        fmt.println("{i}")
    }
}
"#;
    let rep = check(src);
    assert_eq!(
        rep.verdict,
        Verdict::Accept,
        "unexpected diagnostics: {:?}",
        rep.diagnostics
    );
}
