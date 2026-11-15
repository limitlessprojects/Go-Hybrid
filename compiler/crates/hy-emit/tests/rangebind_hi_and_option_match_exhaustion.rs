use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parse succeeds");
    emit_go(&file).expect("emit succeeds")
}

#[test]
fn rangebind_preserves_outer_var_read_by_call_or_recv_in_same_name_exclusive_hi() {
    let go_call = emit(
        r#"package main

import "fmt"

fn limit(n int) int {
    return n + 1
}

fn main() {
    xs := []int{10, 20}
    for i, x in xs {
        fmt.println(i)
        for x in 0..limit(x) {
            fmt.println(x)
        }
    }
}
"#,
    );
    assert!(
        go_call.contains("for i, x := range xs {"),
        "outer `x` read by hoisted `hyHi := limit(x)` must not be blanked:\n{go_call}"
    );
    assert!(
        go_call.contains("hyHi0 := limit(x)")
            && go_call.contains("for x := 0; x < hyHi0; x++ {"),
        "expected hoisted `hyHi0 := limit(x)` before inner range loop:\n{go_call}"
    );

    let go_recv = emit(
        r#"package main

import "fmt"

fn run(xs []int, ch chan int) {
    for i, x in xs {
        fmt.println(i)
        for x in 0..(<-ch + x) {
            fmt.println(x)
        }
    }
}
"#,
    );
    assert!(
        go_recv.contains("for i, x := range xs {"),
        "outer `x` read by hoisted receive bound must not be blanked:\n{go_recv}"
    );
    assert!(
        go_recv.contains("hyHi0 := (<-ch + x)")
            && go_recv.contains("for x := 0; x < hyHi0; x++ {"),
        "expected hoisted receive upper bound before inner range loop:\n{go_recv}"
    );
}

#[test]
fn rangebind_blanked_outer_var_does_not_hoist_undefined_same_name_hi() {
    let go_blanked = emit(
        r#"package main

import "fmt"

fn main() {
    xs := []int{10, 20}
    for i, v in xs {
        fmt.println(i)
        for v in 0..v {
            fmt.println(v)
        }
    }
}
"#,
    );
    assert!(
        go_blanked.contains("for i, _ := range xs {"),
        "outer `v` should be blanked when only referenced by try/call-free same-name inner range bound:\n{go_blanked}"
    );
    assert!(
        !go_blanked.contains(":= v"),
        "blanked outer `v` must not be referenced by a hoisted `hyHi := v`:\n{go_blanked}"
    );
    assert!(
        go_blanked.contains("for v := 0; v < v; v++ {"),
        "expected direct inner range header when outer `v` is blanked:\n{go_blanked}"
    );

    let go_used = emit(
        r#"package main

import "fmt"

fn main() {
    xs := []int{10, 20}
    for i, v in xs {
        fmt.println(i, v)
        for v in 0..v {
            fmt.println(v)
        }
    }
}
"#,
    );
    assert!(
        go_used.contains("for i, v := range xs {")
            && go_used.contains("hyHi0 := v")
            && go_used.contains("for v := 0; v < hyHi0; v++ {"),
        "materialized outer `v` must still pre-evaluate `hyHi0 := v` before inner loop:\n{go_used}"
    );
}

#[test]
fn ordered_option_match_skips_unreachable_arms_and_syncs_rangebind() {
    let go = emit(
        r#"package main

import "fmt"

fn classify(opt option[int], flag bool, xs []int) int {
    for i, v in xs {
        fmt.println(i)
        match opt {
            some(x) => { fmt.println(x) }
            some(y) => { fmt.println(v, y) }
            none if flag => { fmt.println(1) }
            none => { fmt.println(2) }
            _ => { fmt.println(v) }
        }
    }
    match opt {
        some(a) => { return a }
        none if flag => { return 1 }
        none => { return 0 }
        _ => { fmt.println("dead") }
    }
}
"#,
    );
    assert!(
        go.contains("for i, _ := range xs {"),
        "outer `v` mentioned only in unreachable arms after some+none exhaustion must be blanked:\n{go}"
    );
    assert!(
        !go.contains("fmt.Println(v)"),
        "unreachable `_` arm after unguarded `some` and `none` must not be emitted:\n{go}"
    );
    assert!(
        go.contains("panic(\"unreachable\")"),
        "exhaustive ordered option match whose reachable arms all return must emit `panic(\"unreachable\")`:\n{go}"
    );
}

#[test]
fn guarded_some_option_matches_emit_unreachable_panic_when_all_arms_return() {
    let go = emit(
        r#"package main

fn from_guarded_wildcard(opt option[int]) int {
    match opt {
        some(x) if x > 0 => { return x }
        _ => { return 0 }
    }
}

fn from_multi_guarded_some(opt option[int]) int {
    match opt {
        some(x) if x > 10 => { return 2 }
        some(x) if x > 0 => { return 1 }
        some(_) => { return 0 }
        none => { return -1 }
    }
}
"#,
    );
    let panic_count = go.matches("panic(\"unreachable\")").count();
    assert_eq!(
        panic_count, 2,
        "both returning match functions using `hyMatched` fallthrough guards must end with `panic(\"unreachable\")`:\n{go}"
    );
}
