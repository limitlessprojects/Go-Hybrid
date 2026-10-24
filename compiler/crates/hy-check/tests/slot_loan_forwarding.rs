//! Regression tests for issue #749: return-forwarded and multi-value unpacked map-slot loans.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn return_forwarded_slot_loan_local_and_assign_extend_a3_window_e3102() {
    let local_src = r#"import "fmt"
struct entry { n int }
fn id(p *entry) *entry { return move p }
fn main() {
    m := map[string]entry{"a": entry{n: 1}}
    p := &m["a"]
    p2 := id(p)
    m["b"] = entry{n: 2}
    p2.n = 9
    fmt.println(p2.n)
}
"#;
    let r1 = check(local_src);
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
    assert!(error_codes(&r1).iter().any(|c| c == "HYB-E3102"), "got: {:?}", r1.diagnostics);

    let assign_src = r#"import "fmt"
struct entry { n int }
fn id(p *entry) *entry { return move p }
fn main() {
    m := map[string]entry{"a": entry{n: 1}}
    p := &m["a"]
    var p2 *entry
    p2 = id(p)
    m["b"] = entry{n: 2}
    p2.n = 9
    fmt.println(p2.n)
}
"#;
    let r2 = check(assign_src);
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert!(error_codes(&r2).iter().any(|c| c == "HYB-E3102"), "got: {:?}", r2.diagnostics);
}

#[test]
fn multi_value_unpacked_slot_loan_local_and_assign_extend_a3_window_e3102() {
    let local_src = r#"import "fmt"
struct entry { n int }
fn id_pair(p *entry) (*entry, bool) { return move p, true }
fn main() {
    m := map[string]entry{"a": entry{n: 1}}
    p := &m["a"]
    p2, ok := id_pair(p)
    m["b"] = entry{n: 2}
    p2.n = 9
    fmt.println(ok, p2.n)
}
"#;
    let r1 = check(local_src);
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
    assert!(error_codes(&r1).iter().any(|c| c == "HYB-E3102"), "got: {:?}", r1.diagnostics);

    let assign_src = r#"import "fmt"
struct entry { n int }
fn id_pair(p *entry) (*entry, bool) { return move p, true }
fn main() {
    m := map[string]entry{"a": entry{n: 1}}
    p := &m["a"]
    var p2 *entry
    var ok bool
    p2, ok = id_pair(p)
    m["b"] = entry{n: 2}
    p2.n = 9
    fmt.println(ok, p2.n)
}
"#;
    let r2 = check(assign_src);
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert!(error_codes(&r2).iter().any(|c| c == "HYB-E3102"), "got: {:?}", r2.diagnostics);
}

#[test]
fn addr_of_forwarded_or_unpacked_slot_loan_is_rejected_e3104() {
    for snippet in [
        "p2 := p\n    q := &p2\n    fmt.println(q)",
        "p2 := id(p)\n    q := &p2\n    fmt.println(q)",
        "p2 := id(p)\n    q := &p2.n\n    fmt.println(q)",
        "p2, ok := id_pair(p)\n    q := &p2\n    fmt.println(ok, q)",
    ] {
        let src = format!(
            r#"import "fmt"
struct entry {{ n int }}
fn id(p *entry) *entry {{ return move p }}
fn id_pair(p *entry) (*entry, bool) {{ return move p, true }}
fn main() {{
    m := map[string]entry{{"a": entry{{n: 1}}}}
    p := &m["a"]
    {snippet}
}}
"#
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(error_codes(&r).iter().any(|c| c == "HYB-E3104"), "got: {:?}", r.diagnostics);
    }
}

#[test]
fn primitive_result_of_multi_value_unpack_does_not_extend_slot_loan() {
    let src = r#"import "fmt"
struct entry { n int }
fn id_and_count(p *entry) (*entry, int) { return move p, 42 }
fn main() {
    m := map[string]entry{"a": entry{n: 1}}
    p := &m["a"]
    p2, n := id_and_count(p)
    p2.n = 9
    m["b"] = entry{n: 2}
    q := &n
    fmt.println(q)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn move_wrapped_and_chained_slot_loan_forwarding_preserve_a3_and_t24() {
    for bind_stmt in ["p2 := move p", "p2 := id(move p)", "p2 := id(id(p))"] {
        let a3_src = format!(
            r#"import "fmt"
struct entry {{ n int }}
fn id(p *entry) *entry {{ return move p }}
fn main() {{
    m := map[string]entry{{"a": entry{{n: 1}}}}
    p := &m["a"]
    {bind_stmt}
    delete(m, "a")
    p2.n = 9
    fmt.println(p2.n)
}}
"#
        );
        let r_a3 = check(&a3_src);
        assert_eq!(r_a3.verdict, Verdict::Reject, "{:?}", r_a3.diagnostics);
        assert!(error_codes(&r_a3).iter().any(|c| c == "HYB-E3102"), "got: {:?}", r_a3.diagnostics);

        let t24_src = format!(
            r#"import "fmt"
struct entry {{ n int }}
fn id(p *entry) *entry {{ return move p }}
fn main() {{
    m := map[string]entry{{"a": entry{{n: 1}}}}
    p := &m["a"]
    {bind_stmt}
    q := &p2
    fmt.println(q)
}}
"#
        );
        let r_t24 = check(&t24_src);
        assert_eq!(r_t24.verdict, Verdict::Reject, "{:?}", r_t24.diagnostics);
        assert!(error_codes(&r_t24).iter().any(|c| c == "HYB-E3104"), "got: {:?}", r_t24.diagnostics);
    }
}

#[test]
fn closure_multi_value_unpacked_slot_loan_enforces_a3_and_t24() {
    let a3_src = r#"import "fmt"
struct entry { n int }
fn id_pair(p *entry) (*entry, bool) { return move p, true }
fn main() {
    f := fn() {
        m := map[string]entry{"a": entry{n: 1}}
        p := &m["a"]
        p2, ok := id_pair(p)
        delete(m, "a")
        p2.n = 9
        fmt.println(ok, p2.n)
    }
    f()
}
"#;
    let r_a3 = check(a3_src);
    assert_eq!(r_a3.verdict, Verdict::Reject, "{:?}", r_a3.diagnostics);
    assert!(error_codes(&r_a3).iter().any(|c| c == "HYB-E3102"), "got: {:?}", r_a3.diagnostics);

    let t24_src = r#"import "fmt"
struct entry { n int }
fn id_pair(p *entry) (*entry, bool) { return move p, true }
fn main() {
    f := fn() {
        m := map[string]entry{"a": entry{n: 1}}
        p := &m["a"]
        p2, ok := id_pair(p)
        q := &p2
        fmt.println(ok, q)
    }
    f()
}
"#;
    let r_t24 = check(t24_src);
    assert_eq!(r_t24.verdict, Verdict::Reject, "{:?}", r_t24.diagnostics);
    assert!(error_codes(&r_t24).iter().any(|c| c == "HYB-E3104"), "got: {:?}", r_t24.diagnostics);
}
