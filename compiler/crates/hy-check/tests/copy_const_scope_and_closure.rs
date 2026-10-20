//! Regression tests for issue #238:
//!   1a. `copy(dst, src)` must enforce T22 `const` deep-immutability on `dst`
//!       (direct `const` slice, local `const` slice, alias/re-slice of `const`,
//!       and inside closure bodies) with `HYB-E3006`.
//!   1b. `copy(dst, src)` inside a nested block scope (`if` / `for` / `{}`)
//!       between outer reference-element slices must preserve the `may_share`
//!       edge when the block exits (`scope_pop`), rejecting post-move access
//!       with `HYB-E3003` and non-owned moves with `HYB-E3004`.
//!   1c. `copy(dst, src)` inside a closure body must record `may_share` edges
//!       in the closure boundary pass (`closures.rs`), rejecting post-move
//!       access (`HYB-E3003`) and moves of closure-local slices that copied
//!       from captured/parameter slices (`HYB-E3004`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("issue_238.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

#[test]
fn copy_into_file_level_const_slice_rejects_with_e3006() {
    let r = check(
        "const shared_slice = []int{1, 2, 3}\n\
         fn main() {\n\
           src := []int{9, 8, 7}\n\
           copy(shared_slice, src)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for copy into file-level const slice, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_into_local_const_slice_rejects_with_e3006() {
    let r = check(
        "fn main() {\n\
           const local_const = []int{1, 2, 3}\n\
           src := []int{9, 8, 7}\n\
           copy(local_const, src)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for copy into local const slice, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_into_alias_or_reslice_of_const_slice_rejects_with_e3006() {
    let r = check(
        "const shared_slice = []int{1, 2, 3}\n\
         fn main() {\n\
           alias := shared_slice\n\
           src := []int{9, 8, 7}\n\
           copy(alias[1:], src)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for copy into alias/reslice of const slice, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_into_const_slice_inside_closure_rejects_with_e3006() {
    let r = check(
        "const shared_slice = []int{1, 2, 3}\n\
         fn main() {\n\
           f := fn() {\n\
             src := []int{9, 8, 7}\n\
             copy(shared_slice, src)\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for copy into const slice inside closure, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_inside_if_block_preserves_may_share_and_freezes_on_move() {
    let r = check(
        "fn worker(xs []map[string]int) {\n\
           xs[0][\"k\"] = 1\n\
         }\n\
         fn main() {\n\
           dst := []map[string]int{map[string]int{\"k\": 0}}\n\
           src := []map[string]int{map[string]int{\"k\": 0}}\n\
           if true {\n\
             copy(dst, src)\n\
           }\n\
           task worker(move src)\n\
           dst[0][\"k\"] = 2\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when writing through dst after copy inside block and move src, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_from_param_inside_if_block_rejects_move_of_dst_with_e3004() {
    let r = check(
        "fn worker(xs []map[string]int) {\n\
           xs[0][\"k\"] = 1\n\
         }\n\
         fn run(src []map[string]int) {\n\
           dst := []map[string]int{map[string]int{\"k\": 0}}\n\
           if true {\n\
             copy(dst, src)\n\
           }\n\
           task worker(move dst)\n\
         }\n\
         fn main() {\n\
           s := []map[string]int{map[string]int{\"k\": 0}}\n\
           run(s)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving dst that copied from param src inside block, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_inside_closure_freezes_peer_on_move_with_e3003() {
    let r = check(
        "fn worker(xs []map[string]int) {\n\
           xs[0][\"k\"] = 1\n\
         }\n\
         fn main() {\n\
           f := fn() {\n\
             dst := []map[string]int{map[string]int{\"k\": 0}}\n\
             src := []map[string]int{map[string]int{\"k\": 0}}\n\
             copy(dst, src)\n\
             task worker(move src)\n\
             dst[0][\"k\"] = 2\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for post-move write after copy inside closure, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_from_captured_slice_inside_closure_rejects_move_with_e3004() {
    let r = check(
        "fn worker(xs []map[string]int) {\n\
           xs[0][\"k\"] = 1\n\
         }\n\
         fn main() {\n\
           src := []map[string]int{map[string]int{\"k\": 0}}\n\
           f := fn() {\n\
             dst := []map[string]int{map[string]int{\"k\": 0}}\n\
             copy(dst, src)\n\
             task worker(move dst)\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving closure-local dst that copied from captured src, got {:?}",
        r.diagnostics
    );
}

#[test]
fn copy_primitive_slice_inside_block_and_closure_still_accepts() {
    let r = check(
        "const shared_slice = []int{1, 2, 3}\n\
         fn worker(xs []int) {}\n\
         fn main() {\n\
           dst := []int{0, 0, 0}\n\
           if true {\n\
             copy(dst, shared_slice)\n\
           }\n\
           task worker(move dst)\n\
           f := fn() {\n\
             local_dst := []int{0, 0, 0}\n\
             local_src := []int{4, 5, 6}\n\
             copy(local_dst, local_src)\n\
             task worker(move local_src)\n\
             local_dst[0] = 99\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}
