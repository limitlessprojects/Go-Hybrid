use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let parsed = parse_file("test.hy", src);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse failed: {:?}",
        parsed.diagnostics
    );
    check_file(&parsed.file)
}

#[test]
fn rejects_unknown_and_duplicate_fields_in_elided_struct_literals_3743() {
    let r = check(
        "package main
         struct point { x int }
         type pt_ptr = *point
         fn main() {
             ps := []point{{nope: 1}}
             dup := []point{{x: 1, x: 2}}
             ptrs := []*point{{nope: 1}}
             alias_ptrs := []pt_ptr{{nope: 1}}
             m := map[string]point{"a": {nope: 1}}
             g := [][]point{[{nope: 1}]}
             _ = ps
             _ = dup
             _ = ptrs
             _ = alias_ptrs
             _ = m
             _ = g
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e1002: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002")
        .collect();
    assert_eq!(e1002.len(), 6, "{:?}", r.diagnostics);
    assert!(
        e1002.iter().any(|d| d.message.contains("unknown field `nope` in `point` literal")),
        "{:?}",
        e1002
    );
    assert!(
        e1002.iter().any(|d| d.message.contains("duplicate field `x` in `point` literal")),
        "{:?}",
        e1002
    );
}

#[test]
fn rejects_elided_struct_literal_in_array_length_type_exprs_3743() {
    let r = check(
        "package main
         struct point { x int }
         type bad_arr = [fn() int { v := {x: 1}; return v.x }()]int
         struct holder {
             arr [fn() int { v := {x: 1}; return v.x }()]int
         }
         fn main() {
             var a [fn() int { v := {x: 1}; return v.x }()]int
             _ = a
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e1002 = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002" && d.message.contains("type-elided struct literal"))
        .count();
    assert_eq!(e1002, 3, "{:?}", r.diagnostics);
}

#[test]
fn rejects_elided_struct_literal_in_non_struct_foreign_slots_3743() {
    let r = check(
        "package main
         import "io"
         import "time"
         import "net/http"
         import "os"
         import "net"
         fn main() {
             xs := []io.reader{{}}
             ds := []time.duration{{}}
             hs := []http.handler{{}}
             ms := []os.file_mode{{}}
             cs := []net.conn{{}}
             pds := []*time.duration{{}}
             _ = xs
             _ = ds
             _ = hs
             _ = ms
             _ = cs
             _ = pds
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e1002 = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002" && d.message.contains("type-elided struct literal"))
        .count();
    assert_eq!(e1002, 6, "{:?}", r.diagnostics);
}

#[test]
fn accepts_valid_elided_struct_literals_3743() {
    let r = check(
        "package main
         import "net/http"
         struct point { x int }
         fn main() {
             ps := []point{{x: 1}, {x: 2}}
             ptrs := []*point{{x: 3}}
             m := map[string]point{"a": {x: 4}}
             g := [][]point{[{x: 5}]}
             srv := []http.server{{}}
             _ = ps
             _ = ptrs
             _ = m
             _ = g
             _ = srv
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
