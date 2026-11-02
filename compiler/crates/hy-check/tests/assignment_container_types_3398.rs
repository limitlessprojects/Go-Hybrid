//! Regression tests for assignment type details erased by OpKind.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn assert_rejected(src: &str) {
    let result = check(src);
    assert_eq!(
        result.verdict,
        Verdict::Reject,
        "{src}\n{:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for:\n{src}\n{:#?}",
        result.diagnostics
    );
}

#[test]
fn assignments_reject_mismatched_nested_container_types() {
    for (declarations, assignment) in [
        ("var dst chan int32\n    var src chan int64", "dst = src"),
        (
            "var dst map[int32]string\n    var src map[int64]string",
            "dst = src",
        ),
        (
            "var dst map[string]int32\n    var src map[string]int64",
            "dst = src",
        ),
        ("var dst []int32\n    var src []int64", "dst = src"),
        ("var dst *int32\n    var src *int64", "dst = src"),
    ] {
        let src = format!("fn main() {{\n    {declarations}\n    {assignment}\n    _ = dst\n}}\n");
        assert_rejected(&src);
    }
}

#[test]
fn parallel_assignments_reject_mismatched_nested_container_types() {
    assert_rejected(
        "fn main() {\n\
         var chan_dst chan int32\n\
         var chan_src chan int64\n\
         var map_dst map[int32]int\n\
         var map_src map[int64]int\n\
         var map_value_dst map[string]int32\n\
         var map_value_src map[string]int64\n\
         var slice_dst []int32\n\
         var slice_src []int64\n\
         var ptr_dst *int32\n\
         var ptr_src *int64\n\
         chan_dst, map_dst, map_value_dst, slice_dst, ptr_dst = chan_src, map_src, map_value_src, slice_src, ptr_src\n\
         _ = chan_dst\n\
         _ = map_dst\n\
         _ = map_value_dst\n\
         _ = slice_dst\n\
         _ = ptr_dst\n\
         }\n",
    );
}

#[test]
fn assignable_container_aliases_and_channel_direction_stay_accepted() {
    let result = check(
        "fn main() {\n\
         var bytes []byte\n\
         var octets []uint8\n\
         bytes = octets\n\
         var bidirectional chan int\n\
         var receive_only <-chan int\n\
         receive_only = bidirectional\n\
         _ = bytes\n\
         _ = receive_only\n\
         }\n",
    );
    assert_eq!(result.verdict, Verdict::Accept, "{:#?}", result.diagnostics);
    assert!(
        !result.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "{:#?}",
        result.diagnostics
    );
}
