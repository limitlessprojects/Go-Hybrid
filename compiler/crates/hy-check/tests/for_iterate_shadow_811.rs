//! Issue #811 — `for ... in` loop variable shadowing the iterated collection (or an
//! index/key shadowing it alongside a storage-carrying element) must preserve O5.2
//! `may_share` / `shadow_links` edges (`HYB-E3003`) and T22 `const` immutability
//! (`HYB-E3006`) both during the loop and after the loop scope exits.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn for_iterate_index_shadows_collection_preserves_share_on_saved_elem_e3003() {
    // Repro 1 (`for a, b in a`): `a` (index) overwrites outer `a` before `b`
    // records may-share; saving `b` out of the loop and moving `a` must freeze
    // `saved` (`HYB-E3003`).
    let saved_then_move_outer = check(
        "fn worker(owned [][]int, done chan<- bool) {\n\
           owned[0][0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           a := [][]int{[]int{1, 2}}\n\
           saved := []int{}\n\
           done := make(chan bool, 1)\n\
           for a, b in a {\n\
             _ = a\n\
             saved = b\n\
           }\n\
           task worker(move a, done)\n\
           saved[0] = 42\n\
           <-done\n\
         }\n",
    );
    assert_eq!(
        saved_then_move_outer.verdict,
        Verdict::Reject,
        "{:?}",
        saved_then_move_outer.diagnostics
    );
    let codes = error_codes(&saved_then_move_outer);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when `saved` outlives shadowed `for a, b in a`, got {:?}",
        saved_then_move_outer.diagnostics
    );

    // Reverse post-loop direction: moving `saved` must reject both non-owned move
    // (`HYB-E3004`) and freeze outer `a` (`HYB-E3003`).
    let move_saved_then_write_outer = check(
        "fn elem_worker(owned []int, done chan<- bool) {\n\
           owned[0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           a := [][]int{[]int{1, 2}}\n\
           saved := []int{}\n\
           done := make(chan bool, 1)\n\
           for a, b in a {\n\
             _ = a\n\
             saved = b\n\
           }\n\
           task elem_worker(move saved, done)\n\
           a[0][0] = 42\n\
           <-done\n\
         }\n",
    );
    assert_eq!(
        move_saved_then_write_outer.verdict,
        Verdict::Reject,
        "{:?}",
        move_saved_then_write_outer.diagnostics
    );
    let rev_codes = error_codes(&move_saved_then_write_outer);
    assert!(
        rev_codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on outer `a` after moving `saved`, got {:?}",
        move_saved_then_write_outer.diagnostics
    );

    // Map key shadowing map with slice values (`for m, v in m`), inside a closure too.
    let map_and_closure = check(
        "fn map_worker(owned map[string][]int, done chan<- bool) {\n\
           owned[\"k\"][0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           f := fn() {\n\
             m := map[string][]int{\"k\": []int{1, 2}}\n\
             saved := []int{}\n\
             done := make(chan bool, 1)\n\
             for m, v in m {\n\
               _ = m\n\
               saved = v\n\
             }\n\
             task map_worker(move m, done)\n\
             saved[0] = 42\n\
             <-done\n\
           }\n\
           f()\n\
         }\n",
    );
    assert_eq!(
        map_and_closure.verdict,
        Verdict::Reject,
        "{:?}",
        map_and_closure.diagnostics
    );
    assert!(
        error_codes(&map_and_closure)
            .iter()
            .any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 inside closure for shadowed `for m, v in m`, got {:?}",
        map_and_closure.diagnostics
    );
}

#[test]
fn for_iterate_self_shadow_preserves_outer_alias_freeze_e3003() {
    // Repro 2 (`for xs in xs`): `xs` overwrites outer `xs` with the same name;
    // moving a pre-existing outer alias of `xs` inside the loop must freeze inner `xs`.
    let self_shadow = check(
        "fn worker(owned [][]int, done chan<- bool) {\n\
           owned[0][0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := [][]int{[]int{1, 2}}\n\
           alias := xs\n\
           done := make(chan bool, 1)\n\
           for xs in xs {\n\
             task worker(move alias, done)\n\
             xs[0] = 42\n\
           }\n\
           <-done\n\
         }\n",
    );
    assert_eq!(
        self_shadow.verdict,
        Verdict::Reject,
        "{:?}",
        self_shadow.diagnostics
    );
    assert!(
        error_codes(&self_shadow).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on inner `xs` after moving outer `alias`, got {:?}",
        self_shadow.diagnostics
    );

    // Two-var form with outer alias moved inside the loop while index shadows `a`.
    let index_shadow_alias = check(
        "fn worker(owned [][]int, done chan<- bool) {\n\
           owned[0][0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           a := [][]int{[]int{1, 2}}\n\
           alias := a\n\
           done := make(chan bool, 1)\n\
           for a, b in a {\n\
             _ = a\n\
             task worker(move alias, done)\n\
             b[0] = 42\n\
           }\n\
           <-done\n\
         }\n",
    );
    assert_eq!(
        index_shadow_alias.verdict,
        Verdict::Reject,
        "{:?}",
        index_shadow_alias.diagnostics
    );
    assert!(
        error_codes(&index_shadow_alias)
            .iter()
            .any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on `b` after moving `alias` in `for a, b in a`, got {:?}",
        index_shadow_alias.diagnostics
    );

    // Conditional rebind before moving self-shadowed element must still reject
    // because the false branch retains the element of outer `xs`.
    let cond_rebind = check(
        "fn elem_worker(owned []int, done chan<- bool) {\n\
           owned[0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := [][]int{[]int{1, 2}}\n\
           done := make(chan bool, 1)\n\
           cond := true\n\
           for xs in xs {\n\
             if cond {\n\
               xs = []int{9, 9}\n\
             }\n\
             task elem_worker(move xs, done)\n\
             break\n\
           }\n\
           xs[0][0] = 42\n\
           <-done\n\
         }\n",
    );
    assert_eq!(cond_rebind.verdict, Verdict::Reject, "{:?}", cond_rebind.diagnostics);
    assert!(
        error_codes(&cond_rebind).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on outer `xs` after conditional rebind + move, got {:?}",
        cond_rebind.diagnostics
    );
}

#[test]
fn for_iterate_shadowing_const_collection_preserves_t22_e3006() {
    // Repro 3: single-var `for table in table` on a `const` slice-of-slices must
    // preserve T22 deep immutability (`HYB-E3006`).
    for src in [
        "const table = [][]int{[]int{1, 2}}\n\nfn main() {\n    for table in table {\n        table[0] = 99\n    }\n}\n",
        "fn main() {\n    const table = [][]int{[]int{1, 2}}\n    for table in table {\n        table[0] = 99\n    }\n}\n",
        "const table = [][]int{[]int{1, 2}}\n\nfn main() {\n    for table, row in table {\n        _ = table\n        row[0] = 99\n    }\n}\n",
        "const table = [][]int{[]int{1, 2}}\n\nfn main() {\n    saved := []int{}\n    for table, row in table {\n        _ = table\n        saved = row\n    }\n    saved[0] = 99\n}\n",
        "const table = [][]int{[]int{1, 2}}\n\nfn main() {\n    f := fn() {\n        for table in table {\n            table[0] = 99\n        }\n    }\n    f()\n}\n",
        "fn main() {\n    const table = [][]int{[]int{1, 2}}\n    f := fn() {\n        for table, row in table {\n            _ = table\n            row[0] = 99\n        }\n    }\n    f()\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{:?}\nfor:\n{}", r.diagnostics, src);
        let codes = error_codes(&r);
        assert!(
            codes.iter().any(|c| c == "HYB-E3006"),
            "expected HYB-E3006, got {:?}\nfor:\n{}",
            r.diagnostics,
            src
        );
    }
}

#[test]
fn for_iterate_shadowing_accepts_non_racing_positive_controls() {
    for src in [
        // Valid read of shadowed range var + post-loop move of outer owned slice.
        "fn worker(owned [][]int, done chan<- bool) {\n\
           owned[0][0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           a := [][]int{[]int{1, 2}}\n\
           done := make(chan bool, 1)\n\
           for a, b in a {\n\
             _ = a\n\
             _ = b[0]\n\
           }\n\
           task worker(move a, done)\n\
           <-done\n\
         }\n",
        // Valid self-shadow read + post-loop move of outer owned slice.
        "fn worker(owned [][]int, done chan<- bool) {\n\
           owned[0][0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := [][]int{[]int{1, 2}}\n\
           done := make(chan bool, 1)\n\
           for xs in xs {\n\
             _ = xs[0]\n\
           }\n\
           task worker(move xs, done)\n\
           <-done\n\
         }\n",
        // Rebinding `xs` inside the loop to a fresh slice before moving it does not
        // freeze outer `xs`.
        "fn elem_worker(owned []int, done chan<- bool) {\n\
           owned[0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := [][]int{[]int{1, 2}}\n\
           done := make(chan bool, 1)\n\
           for xs in xs {\n\
             xs = []int{9, 9}\n\
             task elem_worker(move xs, done)\n\
             break\n\
           }\n\
           xs[0][0] = 42\n\
           <-done\n\
         }\n",
        // Rebinding a two-var element (even with an outer peer of `a`) inside
        // the loop to a fresh slice before moving it does not freeze outer `a` or `p`.
        "fn elem_worker(owned []int, done chan<- bool) {\n\
           owned[0] = 7\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           a := [][]int{[]int{1, 2}}\n\
           p := a\n\
           done := make(chan bool, 1)\n\
           for a, b in a {\n\
             _ = a\n\
             b = []int{9, 9}\n\
             task elem_worker(move b, done)\n\
             break\n\
           }\n\
           p[0][0] = 42\n\
           <-done\n\
         }\n",
        // Scalar element copied from a `const` slice stays an ordinary writable local.
        "const nums = []int{1, 2, 3}\n\n\
         fn main() {\n\
           for nums in nums {\n\
             nums = 99\n\
             _ = nums\n\
           }\n\
         }\n",
    ] {
        let r = check(src);
        let errs = error_codes(&r);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected Accept for:\n{}\ngot errors: {:?} (all: {:?})",
            src,
            errs,
            r.diagnostics
        );
    }
}
