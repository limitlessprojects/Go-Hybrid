//! array_slice_share.rs — slicing an addressable array shares its backing
//! storage, even when the element type is a primitive.
//!
//! `shape_may_share` recurses into an array only when an *element* may
//! share (`[N]map[...]` yes, `[N]int` no). That is right for a value copy
//! (`b := a` duplicates the ints). It is wrong for a slice expression:
//! `s := a[:]`, `s := b.data[:]`, `row := g[i][:]`, and `append(a[0:1], x)`
//! all point at the array itself. Before the fix `task worker(move s)`
//! froze only `s`, `hyb check` reported 0 diagnostics, and `go run -race`
//! reported `WARNING: DATA RACE` on `s[0]` vs `a[0]`.
//!
//! A call that returns the array by value (`clone(a)[:]`) is a copy and
//! must stay unlinked — the false-positive guard.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn slice_of_primitive_array_freezes_the_array() {
    let r = check(
        "fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             s := a[:]\n\
             done := make(chan bool, 1)\n\
             task worker(move s, done)\n\
             a[0] = 2\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for the write through the array after moving its slice, got {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_of_struct_array_field_freezes_the_struct() {
    let r = check(
        "struct buf {\n\
             data [4]int\n\
         }\n\
         fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var b buf\n\
             s := b.data[:]\n\
             done := make(chan bool, 1)\n\
             task worker(move s, done)\n\
             b.data[0] = 2\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn append_of_array_window_freezes_the_array() {
    // `a[0:1]` has spare capacity (the rest of the array), so
    // `append` writes into `a`'s backing storage.
    let r = check(
        "fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [8]int\n\
             s := append(a[0:1], 2)\n\
             done := make(chan bool, 1)\n\
             task worker(move s, done)\n\
             a[0] = 3\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn two_overlapping_array_slices_cannot_both_be_moved() {
    let r = check(
        "fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             s1 := a[0:2]\n\
             s2 := a[0:2]\n\
             d1 := make(chan bool, 1)\n\
             d2 := make(chan bool, 1)\n\
             task worker(move s1, d1)\n\
             task worker(move s2, d2)\n\
             <-d1\n\
             <-d2\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn channel_send_of_array_slice_freezes_the_array() {
    let r = check(
        "fn recv(ch <-chan []int, done chan<- bool) {\n\
             s := <-ch\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             ch := make(chan []int, 1)\n\
             done := make(chan bool, 1)\n\
             task recv(ch, done)\n\
             s := a[:]\n\
             ch <- move s\n\
             a[0] = 2\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn slice_of_inner_array_freezes_the_outer_array() {
    let r = check(
        "fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var g [2][4]int\n\
             row := g[0][:]\n\
             done := make(chan bool, 1)\n\
             task worker(move row, done)\n\
             g[0][0] = 2\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn slice_of_array_param_is_not_an_owned_move() {
    let r = check(
        "fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn launch(a [4]int, done chan<- bool) {\n\
             s := a[:]\n\
             task worker(move s, done)\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             done := make(chan bool, 1)\n\
             launch(a, done)\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving a slice of a parameter array, got {:?}",
        r.diagnostics
    );
}

#[test]
fn moved_array_slice_with_no_later_use_accepts() {
    // The slice is the only live view. Moving it and not touching the
    // array again is an owned transfer.
    let r = check(
        "fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             s := a[:]\n\
             done := make(chan bool, 1)\n\
             task worker(move s, done)\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn array_value_copy_does_not_share() {
    let r = check(
        "fn worker(b [4]int, done chan<- bool) {\n\
             b[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             b := a\n\
             done := make(chan bool, 1)\n\
             task worker(move b, done)\n\
             a[0] = 2\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn element_copies_do_not_share() {
    let r = check(
        "fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             s := []int{a[0], a[1]}\n\
             done := make(chan bool, 1)\n\
             task worker(move s, done)\n\
             a[0] = 2\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn slice_of_returned_array_does_not_alias_the_argument() {
    // `clone` returns the array by value. Slicing the return views that
    // copy, not the caller's `a`.
    let r = check(
        "fn clone(a [4]int) [4]int {\n\
             return a\n\
         }\n\
         fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var a [4]int\n\
             a[0] = 7\n\
             c := clone(a)\n\
             s := c[:]\n\
             done := make(chan bool, 1)\n\
             task worker(move s, done)\n\
             a[0] = 9\n\
             <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
