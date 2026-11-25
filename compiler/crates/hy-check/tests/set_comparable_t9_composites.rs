//! Regression tests for #2497 (#2469 follow-up): SPEC §2.3 T9 requires `set[T]`
//! element types to be comparable across fixed-size arrays `[N]E`, type aliases,
//! instantiated generic structs `box[E]`, and nested `[N]set[E]` / `map[K]set[E]`
//! folded generic type arguments.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_reject_e1002(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected rejection with HYB-E1002, but accepted: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 diagnostic code, got: {:?}",
        res.diagnostics
    );
}

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected acceptance, but rejected with: {:?}",
        res.diagnostics
    );
}

#[test]
fn set_fixed_array_of_incomparable_rejects() {
    let src = "fn main() {
    var s set[[2][]int]
    _ = s.len()
}
";
    assert_reject_e1002(src);
}

#[test]
fn set_type_alias_to_incomparable_rejects() {
    let src = "type tags = []string
type tag_alias = tags

fn main() {
    var s set[tag_alias]
    _ = s.len()
}
";
    assert_reject_e1002(src);
}

#[test]
fn set_generic_struct_with_incomparable_type_arg_rejects() {
    let src = "struct pair[a, b] {
    first a
    second b
}

fn main() {
    var s set[pair[int, map[string]int]]
    _ = s.len()
}
";
    assert_reject_e1002(src);
}

#[test]
fn nested_array_and_map_folded_type_args_check_inner_set_rejects() {
    let src = "struct box[t] {
    v t
}

fn main() {
    var b1 box[[2]set[[]int]]
    var b2 box[map[string]set[map[int]int]]
    _ = b1
    _ = b2
}
";
    assert_reject_e1002(src);
}

#[test]
fn set_comparable_arrays_aliases_and_generic_structs_accept() {
    let src = "type user_id = int

struct box[t] {
    v t
}

struct chan_box[t] {
    ch chan t
}

fn count_all[t](s set[t]) int {
    return s.len()
}

fn main() {
    var s1 set[[2]int]
    var s2 set[user_id]
    var s3 set[box[[2]user_id]]
    var s4 set[*[]int]
    var s5 set[chan_box[[]int]]
    _ = count_all(s1)
    _ = s2.len()
    _ = s3.len()
    _ = s4.len()
    _ = s5.len()
}
";
    assert_accept(src);
}
