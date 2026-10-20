use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Type-alias struct literals (`type holder_alias = holder; h := holder_alias{...}`)
// ---------------------------------------------------------------------------

#[test]
fn type_alias_struct_lit_shallow_copy_freezes_on_move_e3003() {
    let r = check(
        "struct holder { m map[string]int }\n\
         type holder_alias = holder\n\
         fn worker(h holder_alias, done chan<- bool) {\n\
             h.m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             m1 := map[string]int{\"k\": 0}\n\
             h := holder_alias{m: m1}\n\
             h2 := h\n\
             task worker(move h, done)\n\
             h2.m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for post-move write through shallow copy of type-aliased struct literal, got {:?}",
        r.diagnostics
    );
}

#[test]
fn type_alias_struct_lit_embedding_param_rejects_move_e3004() {
    let r = check(
        "struct holder { m map[string]int }\n\
         type holder_alias = holder\n\
         fn worker(h holder_alias, done chan<- bool) {\n\
             h.m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn bad(m map[string]int, done chan<- bool) {\n\
             h := holder_alias{m: m}\n\
             h2 := h\n\
             task worker(move h2, done)\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             m := map[string]int{\"k\": 0}\n\
             bad(m, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 when moving a type-aliased struct copy that shares parameter storage, got {:?}",
        r.diagnostics
    );
}

#[test]
fn type_alias_struct_lit_holding_closure_rejects_move_e3001() {
    let r = check(
        "struct holder { f fn() }\n\
         type holder_alias = holder\n\
         fn worker(h holder_alias, done chan<- bool) {\n\
             h.f()\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             m := map[string]int{\"k\": 0}\n\
             h := holder_alias{f: fn() { m[\"k\"] = 1 }}\n\
             task worker(move h, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when moving a type-aliased struct holding a closure across tasks, got {:?}",
        r.diagnostics
    );
}

#[test]
fn type_alias_struct_lit_with_nil_field_rejects_n1_e4001() {
    let r = check(
        "struct holder { m map[string]int }\n\
         type holder_alias = holder\n\
         fn main() {\n\
             h := holder_alias{m: nil}\n\
             _ = h\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for nil field inside type-aliased struct literal, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 2. Closure alias / composite-literal / call-return / post-move capture
// ---------------------------------------------------------------------------

#[test]
fn closure_copy_and_rebind_freeze_when_captured_binding_moves_e3003() {
    let r = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
             m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             if true {\n\
                 m := map[string]int{\"k\": 0}\n\
                 f := fn() { m[\"k\"] = 2 }\n\
                 f2 := f\n\
                 task worker(move m, done)\n\
                 f2()\n\
             }\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when calling closure alias `f2` after captured `m` was moved, got {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_embedded_in_struct_or_slice_freezes_when_captured_binding_moves_e3003() {
    let r = check(
        "struct holder { f fn() }\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
             m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             if true {\n\
                 m := map[string]int{\"k\": 0}\n\
                 h := holder{f: fn() { m[\"k\"] = 2 }}\n\
                 task worker(move m, done)\n\
                 h.f()\n\
             }\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when invoking `h.f()` after captured `m` was moved, got {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_call_returning_captured_map_tracks_sharing_and_non_owned_origin() {
    let r_e3003 = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
             m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             m := map[string]int{\"k\": 0}\n\
             get_m := fn() map[string]int { return m }\n\
             m2 := get_m()\n\
             task worker(move m2, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r_e3003.verdict, Verdict::Reject, "{:?}", r_e3003.diagnostics);
    assert!(
        error_codes(&r_e3003).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when mutating `m` after moving `m2 := get_m()`, got {:?}",
        r_e3003.diagnostics
    );

    let r_e3004 = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
             m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn bad(m map[string]int, done chan<- bool) {\n\
             m2 := (fn() map[string]int { return m })()\n\
             task worker(move m2, done)\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             m := map[string]int{\"k\": 0}\n\
             bad(m, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r_e3004.verdict, Verdict::Reject, "{:?}", r_e3004.diagnostics);
    assert!(
        error_codes(&r_e3004).iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 when moving `m2` returned from a closure capturing parameter `m`, got {:?}",
        r_e3004.diagnostics
    );
}

#[test]
fn closure_defined_or_invoked_after_move_rejects_in_block_scope_e3003() {
    let r_def = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
             m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             if true {\n\
                 m := map[string]int{\"k\": 0}\n\
                 task worker(move m, done)\n\
                 f := fn() { m[\"k\"] = 2 }\n\
                 f()\n\
             }\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r_def.verdict, Verdict::Reject, "{:?}", r_def.diagnostics);
    assert!(
        error_codes(&r_def).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when defining and calling `f` after `move m` inside a block scope, got {:?}",
        r_def.diagnostics
    );

    let r_arg = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
             m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn run(cb fn()) { cb() }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             if true {\n\
                 m := map[string]int{\"k\": 0}\n\
                 task worker(move m, done)\n\
                 run(fn() { m[\"k\"] = 2 })\n\
             }\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r_arg.verdict, Verdict::Reject, "{:?}", r_arg.diagnostics);
    assert!(
        error_codes(&r_arg).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when passing closure capturing moved `m` to call inside a block scope, got {:?}",
        r_arg.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 3. Positive controls (must stay Accept with 0 diagnostics)
// ---------------------------------------------------------------------------

#[test]
fn positive_controls_type_alias_move_and_fresh_closure_accept() {
    let r = check(
        "struct holder { m map[string]int }\n\
         type holder_alias = holder\n\
         fn worker(h holder_alias, done chan<- bool) {\n\
             h.m[\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             h := holder_alias{m: map[string]int{\"k\": 0}}\n\
             f_shadow := fn(h holder_alias) { h.m[\"fresh\"] = 1 }\n\
             task worker(move h, done)\n\
             h_fresh := holder_alias{m: map[string]int{\"fresh\": 0}}\n\
             f_shadow(h_fresh)\n\
             f := fn() { h_fresh.m[\"fresh\"] = 2 }\n\
             f2 := f\n\
             f2()\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}