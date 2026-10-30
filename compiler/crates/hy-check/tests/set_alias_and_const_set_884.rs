//! set_alias_and_const_set_884.rs — `set[T]` shares its backing map, so a
//! shallow copy must be tracked as an alias (issue #884).
//!
//! `set[T]` lowers to a struct wrapping a Go `map[T]struct{}`. Classifying it
//! as an unknown shape dropped two guarantees:
//!
//! 1. `s2 := s1` recorded no alias, so moving `s1` across a task boundary and
//!    then writing through `s2` raced with 0 diagnostics (HYB-E3003 missing).
//! 2. A `const` set mutated through a prelude method (`insert`/`remove`) did
//!    not enforce T22 deep immutability (HYB-E3006 missing).
//!
//! Controls pin that the copy-tolerant and read-only shapes stay accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_code(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
}

#[test]
fn set_copy_alias_after_move_rejects_e3003() {
    let r = check(
        "fn worker(s set[int], done chan int) {\n\
           s.insert(2)\n\
           done <- 1\n\
         }\n\
         fn main() {\n\
           done := make(chan int, 1)\n\
           s1 := set[int]{}\n\
           s2 := s1\n\
           task worker(move s1, done)\n\
           s2.insert(3)\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3003"));
}

#[test]
fn const_set_insert_rejects_e3006() {
    let r = check(
        "const cs = set[int]{}\n\
         fn main() {\n\
           cs.insert(1)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3006"));
}

#[test]
fn const_set_remove_rejects_e3006() {
    let r = check(
        "const cs = set[int]{}\n\
         fn main() {\n\
           cs.remove(1)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3006"));
}

#[test]
fn const_set_read_stays_accepted() {
    let r = check(
        "const cs = set[int]{}\n\
         fn main() {\n\
           _ = cs.contains(1)\n\
           _ = cs.len()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn set_copy_without_move_stays_accepted() {
    let r = check(
        "fn main() {\n\
           s1 := set[int]{}\n\
           s1.insert(1)\n\
           s2 := s1\n\
           s2.insert(2)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn non_const_set_param_insert_stays_accepted() {
    let r = check(
        "fn add(s set[int]) {\n\
           s.insert(1)\n\
         }\n\
         fn main() {\n\
           add(set[int]{})\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
