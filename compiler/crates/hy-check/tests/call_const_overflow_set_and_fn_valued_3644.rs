use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_call_const_overflow_set_delete_and_fn_field_3644() {
    let repro1 = r#"
fn main() {
    seen := set[u8]{}
    delete(seen, 300)
}
"#;
    let r1 = check_file(&parse_file("repro1.hy", repro1).unwrap());
    assert_eq!(r1.verdict, Verdict::Reject, "delete(seen, 300) must reject");
    assert!(
        r1.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
        "expected HYB-E6002 on delete(set[u8], 300): {:?}",
        r1.diagnostics
    );

    let repro2 = r#"
struct holder {
    cb fn(u8)
}

fn main() {
    h := holder{}
    h.cb(300)
}
"#;
    let r2 = check_file(&parse_file("repro2.hy", repro2).unwrap());
    assert_eq!(r2.verdict, Verdict::Reject, "h.cb(300) must reject");
    assert!(
        r2.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
        "expected HYB-E6002 on h.cb(300): {:?}",
        r2.diagnostics
    );

    // Negative control: in-range constants must accept
    let accept_src = r#"
struct holder {
    cb fn(u8)
}

fn main() {
    seen := set[u8]{}
    delete(seen, 50)

    h := holder{}
    h.cb(50)
}
"#;
    let ra = check_file(&parse_file("accept.hy", accept_src).unwrap());
    assert_eq!(ra.verdict, Verdict::Accept, "in-range constants must accept: {:?}", ra.diagnostics);
}
