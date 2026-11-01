//! Regression tests for N5 zero-value map allocation in `zeroinit.rs` and
//! `expr.rs` across `new(T)` / `&T{}` on array types and array aliases,
//! partial array literals with non-decimal or `const` lengths, and elided
//! composite literals in struct fields and behind type aliases.

use std::process::Command;

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn check_and_emit(src: &str) -> String {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    emit_go(&ast).unwrap_or_else(|e| panic!("must emit: {e:?}"))
}

fn assert_go_runs(go: &str, tag: &str, expected_stdout: &str) {
    if Command::new("go").arg("version").output().is_ok() {
        let go_file = std::env::temp_dir().join(format!("hy_n5_{tag}.go"));
        std::fs::write(&go_file, go).expect("write main.go");
        let out = Command::new("go")
            .args(["run", go_file.to_str().unwrap()])
            .output()
            .expect("go run");
        let _ = std::fs::remove_file(&go_file);
        assert!(
            out.status.success(),
            "go run failed:\n{}\n---\n{}",
            String::from_utf8_lossy(&out.stderr),
            go
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), expected_stdout);
    }
}

#[test]
fn n5_new_and_addr_of_array_types_and_aliases_emit_pointer_iife() {
    let go = check_and_emit(
        "\
struct bag {
    counts map[string]int
}

type pair = [2]bag
type maps = [2]map[string]int

fn main() {
    p1 := new(pair)
    p2 := new(maps)
    p3 := new([2]bag{})
    p4 := (new)(bag)
    p5 := &[2]bag{}
    p6 := &pair{}
    v := pair{}
    p1[0].counts[\"a\"] = 1
    p2[1][\"b\"] = 2
    p3[1].counts[\"c\"] = 3
    p4.counts[\"d\"] = 4
    p5[0].counts[\"e\"] = 5
    p6[1].counts[\"f\"] = 6
    v[0].counts[\"g\"] = 7
    fmt.println(p1[0].counts[\"a\"] + p2[1][\"b\"] + p3[1].counts[\"c\"] + p4.counts[\"d\"] + p5[0].counts[\"e\"] + p6[1].counts[\"f\"] + v[0].counts[\"g\"])
}
",
    );
    assert!(
        !go.contains("&func()"),
        "must never emit invalid `&func()...()` in Go:\n{go}"
    );
    for want in [
        "p1 := func() *pair { var hyArr pair; for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "p2 := func() *maps { var hyArr maps; for i := range hyArr { hyArr[i] = map[string]int{} }; return &hyArr }()",
        "p3 := func() *[2]bag { var hyArr [2]bag; for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "p4 := &bag{counts: map[string]int{}}",
        "p5 := func() *[2]bag { var hyArr [2]bag; for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "p6 := func() *pair { var hyArr pair; for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "v := func() (hyArr pair) { for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return hyArr }()",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
    assert_go_runs(&go, "new_and_addr_of_array", "28");
}

#[test]
fn n5_partial_array_literals_with_hex_and_const_lengths_allocate_trailing_elements() {
    let go = check_and_emit(
        "\
struct bag {
    label  string
    counts map[string]int
}

const k = 3

fn main() {
    a := [0x3]bag{bag{label: \"first\"}}
    b := [k]bag{bag{label: \"head\"}}
    a[2].counts[\"x\"] = 10
    b[2].counts[\"y\"] = 20
    fmt.println(a[2].counts[\"x\"] + b[2].counts[\"y\"])
}
",
    );
    for want in [
        "a := [0x3]bag{bag{label: \"first\", counts: map[string]int{}}, bag{counts: map[string]int{}}, bag{counts: map[string]int{}}}",
        "b := func() (hyArr [k]bag) { hyArr = [k]bag{bag{label: \"head\", counts: map[string]int{}}}; for i := 1; i < len(hyArr); i++ { hyArr[i] = bag{counts: map[string]int{}} }; return hyArr }()",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
    assert_go_runs(&go, "partial_array_literals", "30");
}

#[test]
fn n5_elided_composite_literals_in_struct_fields_and_aliases_build_and_run() {
    let go = check_and_emit(
        "\
struct bag {
    label  string
    counts map[string]int
}

type bag_ptr = *bag
type pair = [2]bag

struct shelf {
    top   bag
    ptr   bag_ptr
    slots pair
}

fn main() {
    s := shelf{
        top: {label: \"t\"},
        ptr: {label: \"p\"},
        slots: [{label: \"s0\"}],
    }
    ptrs := []bag_ptr{{label: \"elem\"}}
    grid := [][2]bag{{}}
    s.top.counts[\"a\"] = 1
    s.ptr.counts[\"b\"] = 2
    s.slots[0].counts[\"c\"] = 3
    s.slots[1].counts[\"d\"] = 4
    ptrs[0].counts[\"e\"] = 5
    grid[0][1].counts[\"f\"] = 6
    fmt.println(s.top.counts[\"a\"] + s.ptr.counts[\"b\"] + s.slots[0].counts[\"c\"] + s.slots[1].counts[\"d\"] + ptrs[0].counts[\"e\"] + grid[0][1].counts[\"f\"])
}
",
    );
    for want in [
        "top: bag{label: \"t\", counts: map[string]int{}}",
        "ptr: &bag{label: \"p\", counts: map[string]int{}}",
        "slots: pair{bag{label: \"s0\", counts: map[string]int{}}, bag{counts: map[string]int{}}}",
        "ptrs := []bag_ptr{&bag{label: \"elem\", counts: map[string]int{}}}",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }

    assert_go_runs(&go, "elided_composite_literals", "21");
}
