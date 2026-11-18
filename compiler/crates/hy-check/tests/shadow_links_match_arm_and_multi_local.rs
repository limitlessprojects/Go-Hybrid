//! Issue #966 — `bind_match_arm` and multi-value `process_local` must preserve
//! `shadow_links` and child alias edges on self-shadowed bindings so inner moves
//! after re-shadowing or rebinding cannot bypass `HYB-E3003` (O5.1 / O5.2).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_error_rule(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .and_then(|d| d.tier0_rule.clone())
}

#[test]
fn match_bind_and_multival_define_preserve_shadow_links_on_inner_move() {
    // 1. match some(m) { some(m) => { inner := m; m := 1; _ = m; task worker(move inner, done) } none => {} } m["main"] = 2 -> HYB-E3003
    let match_shadow_define = check(
        "fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           done := make(chan bool, 1)\n\
           match some(m) {\n\
             some(m) => {\n\
               inner := m\n\
               m := 1\n\
               _ = m\n\
               task worker(move inner, done)\n\
             }\n\
             none => {}\n\
           }\n\
           m[\"main\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(
        match_shadow_define.verdict,
        Verdict::Reject,
        "{:?}",
        match_shadow_define.diagnostics
    );
    assert_eq!(match_shadow_define.diagnostics[0].code, "HYB-E3003");
    assert_eq!(
        first_error_rule(&match_shadow_define).as_deref(),
        Some("A1b")
    );

    // 2. match some(m) { some(m) => { inner := m; m = map[string]int{}; task worker(move inner, done) } none => {} } m["main"] = 2 -> HYB-E3003
    let match_shadow_rebind = check(
        "fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           done := make(chan bool, 1)\n\
           match some(m) {\n\
             some(m) => {\n\
               inner := m\n\
               m = map[string]int{}\n\
               task worker(move inner, done)\n\
             }\n\
             none => {}\n\
           }\n\
           m[\"main\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(
        match_shadow_rebind.verdict,
        Verdict::Reject,
        "{:?}",
        match_shadow_rebind.diagnostics
    );
    assert_eq!(match_shadow_rebind.diagnostics[0].code, "HYB-E3003");
    assert_eq!(
        first_error_rule(&match_shadow_rebind).as_deref(),
        Some("A1b")
    );

    // 3. if true { m, ok := pass(m); _ = ok; inner := m; m = map[string]int{}; task worker(move inner, done) } m["main"] = 2 -> HYB-E3003
    let multival_rebind = check(
        "fn pass(m map[string]int) (map[string]int, bool) { return m, true }\n\
         fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           done := make(chan bool, 1)\n\
           if true {\n\
             m, ok := pass(m)\n\
             _ = ok\n\
             inner := m\n\
             m = map[string]int{}\n\
             task worker(move inner, done)\n\
           }\n\
           m[\"main\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(
        multival_rebind.verdict,
        Verdict::Reject,
        "{:?}",
        multival_rebind.diagnostics
    );
    assert_eq!(multival_rebind.diagnostics[0].code, "HYB-E3003");
    assert_eq!(first_error_rule(&multival_rebind).as_deref(), Some("A1b"));

    // 4. if true { m, ok := pass(m); _ = ok; inner := m; if true { m := 1; _ = m; task worker(move inner, done) } } m["main"] = 2 -> HYB-E3003
    let multival_nested_shadow = check(
        "fn pass(m map[string]int) (map[string]int, bool) { return m, true }\n\
         fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           done := make(chan bool, 1)\n\
           if true {\n\
             m, ok := pass(m)\n\
             _ = ok\n\
             inner := m\n\
             if true {\n\
               m := 1\n\
               _ = m\n\
               task worker(move inner, done)\n\
             }\n\
           }\n\
           m[\"main\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(
        multival_nested_shadow.verdict,
        Verdict::Reject,
        "{:?}",
        multival_nested_shadow.diagnostics
    );
    assert_eq!(multival_nested_shadow.diagnostics[0].code, "HYB-E3003");
    assert_eq!(
        first_error_rule(&multival_nested_shadow).as_deref(),
        Some("A1b")
    );

    // 5. Positive control (no post-move use of outer `m`) -> Verdict::Accept (0 diagnostics)
    let clean = check(
        "fn pass(m map[string]int) (map[string]int, bool) { return m, true }\n\
         fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           done := make(chan bool, 2)\n\
           match some(m) {\n\
             some(m) => {\n\
               inner := m\n\
               m = map[string]int{}\n\
               task worker(move inner, done)\n\
             }\n\
             none => {}\n\
           }\n\
           m2 := map[string]int{\"init\": 0}\n\
           if true {\n\
             m2, ok := pass(m2)\n\
             _ = ok\n\
             inner2 := m2\n\
             m2 = map[string]int{}\n\
             task worker(move inner2, done)\n\
           }\n\
           <-done\n\
           <-done\n\
         }",
    );
    assert_eq!(clean.verdict, Verdict::Accept, "{:?}", clean.diagnostics);
    assert!(clean.diagnostics.is_empty());
}
