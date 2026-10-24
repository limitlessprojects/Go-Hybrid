use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn parallel_assign_self_reslice_and_sibling_snapshot_rejects_e3003() {
    // Reproducer A (both target orders): `xs, snap = xs[1:], xs` and
    // `snap, xs = xs[1:], xs[2:]` leave `xs` and `snap` sharing the same
    // backing array at runtime. Moving `xs` into a task must freeze `snap`.
    for assign in ["xs, snap = xs[1:], xs", "snap, xs = xs[1:], xs[2:]"] {
        let src = format!(
            "func worker(owned []int, done chan int) {{\n\
               owned[0] = 42\n\
               done <- 1\n\
             }}\n\
             func main() {{\n\
               xs := []int{{10, 20, 30, 40}}\n\
               var snap []int\n\
               {assign}\n\
               done := make(chan int, 1)\n\
               task worker(move xs, done)\n\
               snap[1] = 99\n\
               <-done\n\
             }}"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "assign={assign}: {:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
            "assign={assign}: expected HYB-E3003, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn parallel_define_self_reslice_and_sibling_snapshot_rejects_e3003() {
    // Inner-scope parallel short-var declaration (`xs, snap := xs[1:], xs`)
    // where `xs` shadows outer `xs` while preserving backing storage.
    for decl in ["xs, snap := xs[1:], xs", "snap, xs := xs[1:], xs[2:]"] {
        let src = format!(
            "func worker(owned []int, done chan int) {{\n\
               owned[0] = 42\n\
               done <- 1\n\
             }}\n\
             func main() {{\n\
               xs := []int{{10, 20, 30, 40}}\n\
               done := make(chan int, 1)\n\
               if true {{\n\
                 {decl}\n\
                 task worker(move xs, done)\n\
                 snap[1] = 99\n\
               }}\n\
               <-done\n\
             }}"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "decl={decl}: {:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
            "decl={decl}: expected HYB-E3003, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn parallel_assign_and_define_sibling_targets_reading_overwritten_source_reject_e3003() {
    // Reproducer B: `m, a, b = fresh, m, m` overwrites `m` with a fresh map,
    // while sibling LHS targets `a` and `b` both receive old `m` and alias each other.
    let r_assign = check(
        "func worker(owned map[string]int, done chan int) {\n\
           owned[\"k\"] = 42\n\
           done <- 1\n\
         }\n\
         func main() {\n\
           m := map[string]int{\"k\": 1}\n\
           var a map[string]int\n\
           var b map[string]int\n\
           m, a, b = map[string]int{\"fresh\": 0}, m, m\n\
           _ = m\n\
           done := make(chan int, 1)\n\
           task worker(move a, done)\n\
           b[\"k\"] = 99\n\
           <-done\n\
         }",
    );
    assert_eq!(r_assign.verdict, Verdict::Reject, "{:?}", r_assign.diagnostics);
    assert!(
        r_assign.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r_assign.diagnostics
    );

    let r_define = check(
        "func worker(owned map[string]int, done chan int) {\n\
           owned[\"k\"] = 42\n\
           done <- 1\n\
         }\n\
         func main() {\n\
           m := map[string]int{\"k\": 1}\n\
           done := make(chan int, 1)\n\
           if true {\n\
             m, a, b := 0, m, m\n\
             _ = m\n\
             task worker(move a, done)\n\
             b[\"k\"] = 99\n\
           }\n\
           <-done\n\
         }",
    );
    assert_eq!(r_define.verdict, Verdict::Reject, "{:?}", r_define.diagnostics);
    assert!(
        r_define.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r_define.diagnostics
    );
}

#[test]
fn parallel_swap_of_existing_aliases_rejects_while_independent_swap_accepts() {
    // Swapping two bindings that already share storage (`b := a; a, b = b, a`)
    // preserves their sharing link.
    let r_aliased = check(
        "func worker(owned map[string]int, done chan int) {\n\
           owned[\"k\"] = 42\n\
           done <- 1\n\
         }\n\
         func main() {\n\
           a := map[string]int{\"k\": 1}\n\
           b := a\n\
           a, b = b, a\n\
           done := make(chan int, 1)\n\
           task worker(move a, done)\n\
           b[\"k\"] = 99\n\
           <-done\n\
         }",
    );
    assert_eq!(r_aliased.verdict, Verdict::Reject, "{:?}", r_aliased.diagnostics);
    assert!(
        r_aliased.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r_aliased.diagnostics
    );

    // Positive control: swapping two independent maps does NOT link them to each other.
    let r_indep = check(
        "func worker(owned map[string]int, done chan int) {\n\
           owned[\"k\"] = 42\n\
           done <- 1\n\
         }\n\
         func main() {\n\
           a := map[string]int{\"k\": 1}\n\
           b := map[string]int{\"k\": 2}\n\
           a, b = b, a\n\
           done := make(chan int, 1)\n\
           task worker(move a, done)\n\
           b[\"k\"] = 99\n\
           <-done\n\
         }",
    );
    assert_eq!(r_indep.verdict, Verdict::Accept, "{:?}", r_indep.diagnostics);
}

#[test]
fn parallel_shadow_or_rebind_of_parameter_preserves_non_owned_origin_e3004() {
    // Reproducer C: shadowing `p` on an earlier LHS target while embedding `p`
    // into a struct literal `holder{m: p}` on a sibling LHS target must still
    // reject `move h` with HYB-E3004 (O5.1).
    let r = check(
        "type holder struct {\n\
           m map[string]int\n\
         }\n\
         func worker(owned holder, done chan int) {\n\
           owned.m[\"k\"] = 42\n\
           done <- 1\n\
         }\n\
         func leak(p map[string]int, done chan int) {\n\
           if true {\n\
             p, h := 0, holder{m: p}\n\
             _ = p\n\
             task worker(move h, done)\n\
           }\n\
         }\n\
         func main() {\n\
           m := map[string]int{\"k\": 1}\n\
           done := make(chan int, 1)\n\
           leak(m, done)\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004, got {:?}",
        r.diagnostics
    );
}

#[test]
fn parallel_shadow_or_rebind_of_const_preserves_t22_immutability_e3006() {
    // Reproducer D.1: shadowing `shared_map` while embedding it in `holder{m: shared_map}`
    // (or rebinding in a closure/helper) must still reject writes with HYB-E3006.
    let r_local = check(
        "type holder struct {\n\
           m map[string]int\n\
         }\n\
         func main() {\n\
           const shared_map = map[string]int{\"k\": 1}\n\
           if true {\n\
             shared_map, h := 0, holder{m: shared_map}\n\
             _ = shared_map\n\
             h.m[\"k\"] = 99\n\
           }\n\
         }",
    );
    assert_eq!(r_local.verdict, Verdict::Reject, "{:?}", r_local.diagnostics);
    assert!(
        r_local.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006, got {:?}",
        r_local.diagnostics
    );

    let r_closure = check(
        "const shared_map = map[string]int{\"k\": 1}\n\
         func main() {\n\
           f := func() {\n\
             c := shared_map\n\
             var m2 map[string]int\n\
             c, m2 = map[string]int{\"fresh\": 0}, c\n\
             _ = c\n\
             m2[\"k\"] = 99\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r_closure.verdict, Verdict::Reject, "{:?}", r_closure.diagnostics);
    assert!(
        r_closure.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006, got {:?}",
        r_closure.diagnostics
    );

    let r_helper = check(
        "const shared_map = map[string]int{\"k\": 1}\n\
         func mutate(p map[string]int) {\n\
           if true {\n\
             p, m2 := 0, p\n\
             _ = p\n\
             m2[\"k\"] = 99\n\
           }\n\
         }\n\
         func main() {\n\
           mutate(shared_map)\n\
         }",
    );
    assert_eq!(r_helper.verdict, Verdict::Reject, "{:?}", r_helper.diagnostics);
    assert!(
        r_helper.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006, got {:?}",
        r_helper.diagnostics
    );
}

#[test]
fn parallel_overwritten_map_preserves_live_slot_loan_e3102() {
    // Reproducer D.2: overwriting `m` on Target 0 while copying `m` into `m2`
    // on Target 1 must propagate any live `&m["a"]` slot loan onto `m2`.
    let r = check(
        "type stats struct {\n\
           count int\n\
         }\n\
         func main() {\n\
           m := map[string]stats{\"a\": {count: 1}}\n\
           s := &m[\"a\"]\n\
           var m2 map[string]stats\n\
           m, m2 = map[string]stats{\"fresh\": {count: 0}}, m\n\
           _ = m\n\
           m2[\"b\"] = stats{count: 2}\n\
           s.count = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102, got {:?}",
        r.diagnostics
    );
}

#[test]
fn multi_value_assign_overwriting_source_preserves_prior_peer_and_siblings_e3003() {
    // Defect 4: `xs, snap = int_and_slice(xs)` in an assignment where `xs`
    // previously had a peer `keep := xs` must still link `snap` to `keep`.
    let r = check(
        "func int_and_slice(xs []int) (int, []int) {\n\
           return 1, xs\n\
         }\n\
         func worker(owned []int, done chan int) {\n\
           owned[0] = 42\n\
           done <- 1\n\
         }\n\
         func main() {\n\
           xs := []int{1, 2, 3}\n\
           keep := xs\n\
           var n int\n\
           var snap []int\n\
           if true {\n\
             xs, snap := int_and_slice(xs)\n\
             n = xs\n\
             done := make(chan int, 1)\n\
             task worker(move snap, done)\n\
             keep[0] = 99\n\
             <-done\n\
           }\n\
           _ = n\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}
