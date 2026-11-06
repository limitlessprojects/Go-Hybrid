use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckReport {
    let parsed = parse_file("test.hy", src).expect("parse should succeed");
    check_file("test.hy", &parsed)
}

#[test]
fn rejects_exclusive_range_with_call_in_untyped_hi_and_fixed_width_lo() {
    let src = r#"
package main

import "fmt"

fn main() {
    for i in u8(0)..(10 + len("")) {
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
fn rejects_exclusive_range_with_shadowed_var_in_untyped_hi_and_fixed_width_lo() {
    let src = r#"
package main

import "fmt"

fn main() {
    k := 1
    for k in i16(0)..len([2]int{k, 0}) {
        fmt.println("{k}")
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
fn allows_exclusive_range_with_inline_untyped_hi_and_fixed_width_lo() {
    let src = r#"
package main

import "fmt"

fn main() {
    for i in u8(0)..10 {
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