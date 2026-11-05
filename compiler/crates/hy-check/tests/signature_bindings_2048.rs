use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

#[test]
fn rejects_fn_type_param_colliding_with_param_issue_2048() {
    // Case 1A: Function type parameter colliding with value parameter
    let r = check(
        "fn id[t any](t int) int { return t }\n\
         fn main() {}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("`t`")),
        "expected HYB-E1002 for type parameter collision with param, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_fn_type_param_colliding_with_named_result_issue_2048() {
    // Case 1B: Function type parameter colliding with named result
    let r = check(
        "fn id[t any](x int) (t int) { return x }\n\
         fn main() {}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("`t`")),
        "expected HYB-E1002 for type parameter collision with named result, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_receiver_var_colliding_with_receiver_type_param_issue_2048() {
    // Case 1C: Method receiver variable colliding with generic receiver type argument
    let r = check(
        "struct box[t any] { v t }\n\
         fn (t box[t]) get() int { return 1 }\n\
         fn main() {}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("`t`")),
        "expected HYB-E1002 for receiver var collision with receiver type arg, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_receiver_type_param_colliding_with_param_issue_2048() {
    // Case 1D: Generic receiver type argument colliding with method value parameter
    let r = check(
        "struct box[t any] { v t }\n\
         fn (b box[t]) get(t int) int { return t }\n\
         fn main() {}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("`t`")),
        "expected HYB-E1002 for receiver type arg collision with method param, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_receiver_type_param_colliding_with_named_result_issue_2048() {
    // Case 1E: Generic receiver type argument colliding with method named result
    let r = check(
        "struct box[t any] { v t }\n\
         fn (b box[t]) get(x int) (t int) { return x }\n\
         fn main() {}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("`t`")),
        "expected HYB-E1002 for receiver type arg collision with named result, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_duplicate_receiver_type_param_issue_2048() {
    // Case 1F: Duplicate type parameter bindings in a generic method receiver
    let r = check(
        "struct pair[t any, u any] {\n\
             a t\n\
             b u\n\
         }\n\
         fn (p pair[t, t]) get() int { return 1 }\n\
         fn main() {}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("`t`")),
        "expected HYB-E1002 for duplicate receiver type param, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_unsupported_main_result_signatures_issue_2048() {
    // Single non-int/non-error result
    let r_str = check("fn main() string { return \"ok\" }\n");
    assert_eq!(r_str.verdict, Verdict::Reject, "{:?}", r_str.diagnostics);
    assert!(
        r_str.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for fn main() string, got: {:?}",
        r_str.diagnostics
    );

    // Multi-value results
    let r_multi = check("fn main() (int, error) { return 0, nil }\n");
    assert_eq!(r_multi.verdict, Verdict::Reject, "{:?}", r_multi.diagnostics);
    assert!(
        r_multi.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for fn main() (int, error), got: {:?}",
        r_multi.diagnostics
    );

    let r_multi2 = check("fn main() (int, int) { return 0, 0 }\n");
    assert_eq!(r_multi2.verdict, Verdict::Reject, "{:?}", r_multi2.diagnostics);
    assert!(
        r_multi2.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for fn main() (int, int), got: {:?}",
        r_multi2.diagnostics
    );
}

#[test]
fn accepts_valid_generic_signatures_and_main_results_issue_2048() {
    let r = check(
        "struct box[t any] { v t }\n\
         fn (b box[t]) get(x t) t { return x }\n\
         struct pair[t any, u any] {\n\
             a t\n\
             b u\n\
         }\n\
         fn (p pair[t, u]) get() int { return 1 }\n\
         fn id[t any](x t) t { return x }\n\
         fn main() int { return 0 }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);

    let r_err = check("fn main() error { return nil }\n");
    assert_eq!(r_err.verdict, Verdict::Accept, "{:?}", r_err.diagnostics);

    let r_void = check("fn main() {}\n");
    assert_eq!(r_void.verdict, Verdict::Accept, "{:?}", r_void.diagnostics);
}
