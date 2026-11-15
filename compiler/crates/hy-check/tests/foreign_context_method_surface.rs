//! Tests for curated foreign method surfaces of `context.Context` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invoking phantom/unsupported methods on `context.context` or `*context.context`
//! rejects fail-closed with HYB-E1002, while standard Go methods (`deadline`, `done`, `err`, `value`)
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
fn phantom_context_context_rejected() {
    let bad = check(
        "fn handle(ctx context.context) {\n\
             ctx.cancel()\n\
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
    assert!(msg.contains("cancel"), "message: {msg}");
    assert!(msg.contains("context.context"), "message: {msg}");
}

#[test]
fn phantom_context_pointer_receiver_rejected() {
    let bad = check(
        "fn run(ctx *context.context) {\n\
             ctx.invalid_phantom_method()\n\
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
    assert!(msg.contains("invalid_phantom_method"), "message: {msg}");
    assert!(msg.contains("context.context"), "message: {msg}");
}

#[test]
fn phantom_context_struct_field_rejected() {
    let bad = check(
        "struct worker {\n\
             ctx context.context\n\
         }\n\
         fn main() {\n\
             var w worker\n\
             w.ctx.unsupported_call()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        all_codes
    );
    let msg = &bad.diagnostics[0].message;
    assert!(msg.contains("unsupported_call"), "message: {msg}");
    assert!(msg.contains("context.context"), "message: {msg}");
}

#[test]
fn real_context_methods_accept() {
    let good = check(
        "fn check_ctx(ctx context.context) {\n\
             _ = ctx.deadline()\n\
             _ = ctx.done()\n\
             _ = ctx.err()\n\
             _ = ctx.value(0)\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
    assert!(good.diagnostics.is_empty(), "expected 0 diagnostics, got {:?}", good.diagnostics);
}
