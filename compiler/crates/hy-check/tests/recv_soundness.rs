use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check_src(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("source must parse");
    check_file(&file)
}

#[test]
fn receive_from_option_wrapping_channel_emits_e2008() {
    // Family 1: `shape_of_type(option[chan int])` and `shape_of_expr(some(ch))`
    // erase `option[T]` to `T`'s `TypeShape` (`TypeShape::Chan`), so `<-opt`,
    // `<-some(ch)`, and `select { v := <-opt: }` previously passed `hyb check`
    // with 0 diagnostics even though `option[chan int]` lowers to a Go struct
    // (`invalid operation: cannot receive from non-channel`).
    for (label, src) in [
        (
            "receive from option[chan int] binding",
            r#"
import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 1
    opt := some(ch)
    v := <-opt
    fmt.println(v)
}
"#,
        ),
        (
            "receive from some(ch) call expression",
            r#"
import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 1
    v := <-some(ch)
    fmt.println(v)
}
"#,
        ),
        (
            "select receive from option[chan int] parameter",
            r#"
import "fmt"

fn drain(opt option[chan int]) {
    select {
        v := <-opt: {
            fmt.println(v)
        }
        default: {
        }
    }
}

fn main() {
    ch := make(chan int, 1)
    drain(some(ch))
}
"#,
        ),
        (
            "receive from opt.unwrap() on option[chan<- int] in type position",
            r#"
import "fmt"

fn drain(opt option[chan<- int]) {
    var xs [<-opt.unwrap()]int
    fmt.println(len(xs))
}

fn main() {
    ch := make(chan int, 1)
    var w chan<- int = ch
    drain(some(w))
}
"#,
        ),
    ] {
        let res = check_src(src);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "expected Reject for `{label}`, got diagnostics: {:?}",
            res.diagnostics
        );
        assert!(
            res.diagnostics.iter().any(|d| d.code == "HYB-E2008"),
            "expected HYB-E2008 for `{label}`, got: {:?}",
            res.diagnostics
        );
    }

    // Statement-position `<-opt.unwrap()` on `option[chan<- int]` is rejected
    // by directed-channel checking (`HYB-E3008`, or `HYB-E2008` fallback).
    let res_stmt = check_src(
        r#"
import "fmt"

fn drain(opt option[chan<- int]) {
    v := <-opt.unwrap()
    fmt.println(v)
}

fn main() {
    ch := make(chan int, 1)
    var w chan<- int = ch
    drain(some(w))
}
"#,
    );
    assert_eq!(res_stmt.verdict, Verdict::Reject, "{:?}", res_stmt.diagnostics);
    assert!(
        res_stmt
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3008" || d.code == "HYB-E2008"),
        "expected HYB-E3008/HYB-E2008, got: {:?}",
        res_stmt.diagnostics
    );

    // Legal counterpart: receiving from `opt.unwrap()` and `opt.or(ch)` when
    // `opt` holds a bidirectional `chan int` must be accepted.
    let ok = check_src(
        r#"
import "fmt"

fn main() {
    ch := make(chan int, 2)
    ch <- 7
    ch <- 8
    opt := some(ch)
    v1 := <-opt.unwrap()
    v2 := <-opt.or(ch)
    fmt.println(v1 + v2)
}
"#,
    );
    assert_eq!(
        ok.verdict,
        Verdict::Accept,
        "receiving from opt.unwrap() / opt.or(ch) on option[chan int] must accept: {:?}",
        ok.diagnostics
    );
}

#[test]
fn parallel_if_init_shadowing_and_closure_assign_init_emit_e2008() {
    // Family 2: In Pass B, `walk.rs` walks `if` init before `Stmt::If`, so
    // `st` already reflects `init`. Re-evaluating `if x, y := make(chan int, 1), x`
    // against post-init `st` clobbered `y` from `int` (outer `x`) to `chan int`
    // (new `x`), accepting `(<-y) > 0` on an `int`. Inside a closure,
    // `Stmt::If` with `Stmt::Assign` init (`if a = send_only; (<-a) > 0`) was
    // ignored before checking `cond`.
    let bad_parallel = check_src(
        r#"
import "fmt"

fn main() {
    x := 1
    if x, y := make(chan int, 1), x; (<-y) > 0 {
        x <- 2
        fmt.println(<-x)
    }
    fmt.println(x)
}
"#,
    );
    assert_eq!(
        bad_parallel.verdict,
        Verdict::Reject,
        "parallel if-init `if x, y := make(chan int, 1), x; (<-y) > 0` where outer x is int must reject: {:?}",
        bad_parallel.diagnostics
    );
    assert!(
        bad_parallel
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E2008"),
        "expected HYB-E2008 on `<-y`, got: {:?}",
        bad_parallel.diagnostics
    );

    let bad_closure_if_init = check_src(
        r#"
import "fmt"

fn check_in_closure(arr *[2]chan<- int) {
    f := fn() {
        if a := arr[0]; (<-a) > 0 {
            fmt.println(1)
        }
    }
    f()
}

fn main() {
    ch := make(chan int, 1)
    var a [2]chan<- int = [2]chan<- int{ch, ch}
    check_in_closure(&a)
}
"#,
    );
    assert_eq!(
        bad_closure_if_init.verdict,
        Verdict::Reject,
        "closure `if a := arr[0]; (<-a) > 0` on *[2]chan<- int must reject: {:?}",
        bad_closure_if_init.diagnostics
    );
    assert!(
        bad_closure_if_init
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E2008"),
        "expected HYB-E2008 on closure `<-a`, got: {:?}",
        bad_closure_if_init.diagnostics
    );

    // Legal counterpart: when outer `x` is a channel and `if x, y := 1, x; (<-y) > 0`,
    // `y` receives the outer channel and must be accepted.
    let ok_parallel = check_src(
        r#"
import "fmt"

fn main() {
    x := make(chan int, 1)
    x <- 5
    if x, y := 1, x; (<-y) > 0 {
        fmt.println(x)
    }
}
"#,
    );
    assert_eq!(
        ok_parallel.verdict,
        Verdict::Accept,
        "parallel if-init `if x, y := 1, x; (<-y) > 0` where outer x is chan int must accept: {:?}",
        ok_parallel.diagnostics
    );
}

#[test]
fn pointer_to_array_index_and_type_aliased_option_match_emit_e2008() {
    // Family 3: Go auto-dereferences a pointer to an array on index (`arr[0]`
    // where `arr: *[2]chan<- int` or `*[2]int`), and `match` on a type-aliased
    // `type OptSend = option[chan<- int]` binds a send-only channel in `some(c)`.
    for (label, src) in [
        (
            "receive from pointer-to-array of send-only channels",
            r#"
import "fmt"

fn read_first(arr *[2]chan<- int) {
    v := <-arr[0]
    fmt.println(v)
}

fn main() {
    ch := make(chan int, 1)
    var a [2]chan<- int = [2]chan<- int{ch, ch}
    read_first(&a)
}
"#,
        ),
        (
            "receive from pointer-to-array of ints inside managed block",
            r#"
import "fmt"

fn read_first(arr *[2]int) {
    managed("test E2008 not suppressed by managed", "other") {
        v := <-arr[0]
        fmt.println(v)
    }
}

fn main() {
    var a [2]int = [2]int{1, 2}
    read_first(&a)
}
"#,
        ),
        (
            "receive in match guard from some(c) when subject is *[2]option[chan<- int] index",
            r#"
import "fmt"

fn drain(arr *[2]option[chan<- int]) {
    match arr[0] {
        some(c) if (<-c) > 0: {
            fmt.println(1)
        }
        some(c): {
            fmt.println(0)
        }
        none: {
        }
    }
}

fn main() {
    ch := make(chan int, 1)
    var w chan<- int = ch
    var a [2]option[chan<- int] = [2]option[chan<- int]{some(w), none}
    drain(&a)
}
"#,
        ),
    ] {
        let res = check_src(src);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "expected Reject for `{label}`, got: {:?}",
            res.diagnostics
        );
        assert!(
            res.diagnostics.iter().any(|d| d.code == "HYB-E2008"),
            "expected HYB-E2008 for `{label}`, got: {:?}",
            res.diagnostics
        );
    }

    // Legal counterpart: receiving from `arr[0]` where `arr: *[2]chan int` and
    // from `some(c)` on a type-aliased `option[chan int]` must be accepted.
    let ok = check_src(
        r#"
import "fmt"

type opt_bi = option[chan int]

fn read_first(arr *[2]chan int, opt opt_bi) {
    v1 := <-arr[0]
    match opt {
        some(c): {
            v2 := <-c
            fmt.println(v1 + v2)
        }
        none: {
            fmt.println(v1)
        }
    }
}

fn main() {
    ch := make(chan int, 2)
    ch <- 3
    ch <- 4
    var a [2]chan int = [2]chan int{ch, ch}
    read_first(&a, some(ch))
}
"#,
    );
    assert_eq!(
        ok.verdict,
        Verdict::Accept,
        "receiving from *[2]chan int index and OptBi match arm must accept: {:?}",
        ok.diagnostics
    );
}
