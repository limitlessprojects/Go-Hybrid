use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn exclusive_range_hoists_upper_bound_when_shadowing_loop_var_730() {
    let src = r#"package main

import "fmt"

fn main() {
    n := 3
    for n in 0..n {
        fmt.println(n)
    }
    for n in 0..n * 2 {
        fmt.println(n)
    }
    var m int64 = 4
    for m in int64(0)..m {
        fmt.println(m)
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for n, hyHi0 := 0, n; n < hyHi0; n++ {")
            && go.contains("for n, hyHi1 := 0, n * 2; n < hyHi1; n++ {")
            && go.contains("for m, hyHi2 := int64(0), m; m < hyHi2; m++ {")
            && !go.contains("for n := 0; n < n"),
        "exclusive range must hoist hi in parallel for-init before loop variable n shadows outer n:\n{go}"
    );
}

#[test]
fn exclusive_range_preserves_typed_lo_with_untyped_const_hi_and_labeled_loops_730() {
    let src_typed = r#"package main

import "fmt"

fn main() {
    var start int64 = 5
    for i in start..10 {
        fmt.println(i)
    }
}
"#;
    let file_typed = parse_file("test.hy", src_typed).expect("parse succeeds");
    let go_typed = emit_go(&file_typed).expect("emit succeeds");
    assert!(
        go_typed.contains("for i := start; i < 10; i++ {"),
        "non-shadowing range must keep direct condition so typed lo + untyped constant hi compiles:\n{go_typed}"
    );
    assert!(
        !go_typed.contains("hyHi"),
        "non-shadowing range must not allocate an untyped temp:\n{go_typed}"
    );

    let src_labeled = r#"package main

import "fmt"

fn main() {
    n := 3
    loop {
        for n in 0..n {
            fmt.println(n)
            break loop
        }
    }
    for n in 0..n {
        fmt.println(n)
    }
}
"#;
    let file_labeled = parse_file("test.hy", src_labeled).expect("parse succeeds");
    let go_labeled = emit_go(&file_labeled).expect("emit succeeds");
    assert!(
        go_labeled.contains("hyLoop:
	for {
		for n, hyHi0 := 0, n; n < hyHi0; n++ {
			fmt.Println(n)
			break hyLoop"),
        "nested shadowing range inside labeled loop must emit parallel for-init:
{go_labeled}"
    );
    assert!(
        go_labeled.contains("for n, hyHi1 := 0, n; n < hyHi1; n++ {"),
        "sibling shadowing range must also emit parallel for-init:
{go_labeled}"
    );
}
