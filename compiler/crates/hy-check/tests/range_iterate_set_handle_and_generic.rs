//! Regression tests for `ForKind::Iterate` in `range_iter.rs` (`HYB-E2009` / `LX13` / `T9` / `O7`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("must parse");
    check_file(&ast)
}

fn error_codes(src: &str) -> Vec<String> {
    check(src)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn two_var_iterate_over_borrow_view_and_json_handle_rejects_e2009() {
    let view_escape_src = r#"
import "rust/serde_json"
fn leak(x any) { _ = x }
fn main() {
    u := 0
    _ = u
    doc := serde_json.parse(`{"users":[]}`)?
    for i, u in doc.array("users")? {
        task leak(u)
        _ = i
    }
}
"#;
    assert!(error_codes(view_escape_src).iter().any(|c| c == "HYB-E2009"));

    let handle_src = r#"
import "rust/serde_json"
fn main() {
    arr := serde_json.parse(`[]`)?
    for i, u in arr {
        _ = i
        _ = u
    }
}
"#;
    assert!(error_codes(handle_src).iter().any(|c| c == "HYB-E2009"));
}

#[test]
fn non_literal_and_aliased_set_iteration_enforces_t9_arity() {
    let reject_cases = [
        "fn walk(s set[int]) { for i, v in s { _ = i\n _ = v } }\nfn main() {}",
        "struct bag { seen set[int] }\nfn main() { b := bag{seen: set[int]{}}\n for i, v in b.seen { _ = i\n _ = v } }",
        "fn make_seen() set[int] { return set[int]{} }\nfn main() { for i, v in make_seen() { _ = i\n _ = v } }",
        "type int_set = set[int]\nfn main() { s := int_set{}\n for i, v in s { _ = i\n _ = v } }",
    ];
    for (idx, src) in reject_cases.iter().enumerate() {
        assert_eq!(error_codes(src), vec!["HYB-E2009".to_string()], "case {idx}");
    }

    let accept_src = r#"
type int_set = set[int]
struct bag { seen set[int] }
fn make_seen() set[int] { return set[int]{} }
fn walk(s set[int], b bag) {
    for v in s { _ = v }
    for v in b.seen { _ = v }
    for v in make_seen() { _ = v }
    s2 := int_set{}
    for v in s2 { _ = v }
}
fn main() {}
"#;
    let r = check(accept_src);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn send_only_channel_via_option_and_generics_rejects_e2009() {
    let cases = [
        "fn drain(opt option[chan<- int]) { for v in opt.unwrap() { _ = v } }\nfn main() {}",
        "fn drain(opt option[chan<- int], fb chan<- int) { for v in opt.or(fb) { _ = v } }\nfn main() {}",
        "struct box[t any] { ch t }\nfn drain(ch chan<- int) { b := box[chan<- int]{ch: ch}\n for v in b.ch { _ = v } }\nfn main() {}",
        "fn id[t any](x t) t { return x }\nfn drain(ch chan<- int) { for v in id(ch) { _ = v } }\nfn main() {}",
    ];
    for (idx, src) in cases.iter().enumerate() {
        assert!(error_codes(src).iter().any(|c| c == "HYB-E2009"), "case {idx}");
    }
}

#[test]
fn generic_field_and_call_iterables_classify_instantiated_type_params() {
    let reject_cases = [
        "struct box[t any] { val t }\nfn main() { b := box[int]{val: 5}\n for v in b.val { _ = v } }",
        "fn id[t any](x t) t { return x }\nfn main() { for v in id(5) { _ = v } }",
        "struct box[t any] { val t }\nfn main() { b := box[chan int]{val: make(chan int, 1)}\n for i, v in b.val { _ = i\n _ = v } }",
        "fn id[t any](x t) t { return x }\nfn main() { ch := make(chan int, 1)\n for i, v in id(ch) { _ = i\n _ = v } }",
    ];
    for (idx, src) in reject_cases.iter().enumerate() {
        assert_eq!(error_codes(src), vec!["HYB-E2009".to_string()], "case {idx}");
    }

    let accept_src = r#"
struct box[t any] { val t }
fn id[t any](x t) t { return x }
fn main() {
    b := box[[]int]{val: []int{1, 2, 3}}
    for i, v in b.val {
        _ = i
        _ = v
    }
    for i, v in id([]int{4, 5}) {
        _ = i
        _ = v
    }
}
"#;
    let r = check(accept_src);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}
