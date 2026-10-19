//! Regression tests for `BindingKind`, `var_struct`, `var_ty`, and `pointer_vars`
//! preservation in `hy-emit`:
//! 1. Parallel local declarations (`ch1, ch2 := make(chan int, 1), make(chan int, 1)`,
//!    `s1, s2 := set[int]{}, set[int]{}`, `u1, u2 := pool_stats{}, (pool_stats{})`)
//!    must register binding hints and struct/foreign receiver types for every bound
//!    identifier instead of dropping hints whenever `names.len() > 1` or when a
//!    struct literal is parenthesized.
//! 2. Identifier copies, moves, and address-of bindings (`ch2 := move ch1`,
//!    `s2 := s1`, `e := move c`) must propagate `BindingKind`, `var_struct`,
//!    `var_ty`, and `pointer_vars` from the source variable.
//! 3. `make((chan int), 1)`, `(make)(chan int, 1)`, `new(pool_stats)`,
//!    `new(set[int])`, and type aliases to channels/sets/structs (`type int_chan = chan int`,
//!    `type int_set = set[int]`) must classify channel/set bindings and struct/foreign
//!    receivers accurately.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn parallel_and_rebound_channel_and_set_bindings_lower_correctly() {
    let go = emit_src(
        r#"
type int_chan = chan int
type int_set = set[int]

fn main() {
    ch1, ch2 := make(chan int, 1), make((chan int), 1)
    ch3 := (make)(int_chan, 1)
    ch4 := move ch1
    for a in ch2 {
        fmt.println("{a}")
    }
    for b in ch3 {
        fmt.println("{b}")
    }
    for c in ch4 {
        fmt.println("{c}")
    }

    s1, s2 := set[int]{}, (set[int]{})
    s3 := s1
    var s4 int_set
    delete(s2, 1)
    delete(s3, 2)
    fmt.println("{len(s1)} {len(s2)} {len(s3)} {len(s4)}")
}
"#,
    );
    assert!(
        go.contains("for a := range ch2"),
        "expected single-var channel range for parallel ch2, got:\n{go}"
    );
    assert!(
        go.contains("for b := range ch3"),
        "expected single-var channel range for aliased/parenthesized make ch3, got:\n{go}"
    );
    assert!(
        go.contains("for c := range ch4"),
        "expected single-var channel range for moved ch4, got:\n{go}"
    );
    assert!(
        go.contains("s2.remove(1)"),
        "expected s2.remove(1) for parallel parenthesized set s2, got:\n{go}"
    );
    assert!(
        go.contains("s3.remove(2)"),
        "expected s3.remove(2) for rebound set s3, got:\n{go}"
    );
    assert!(
        go.contains("s1.len()") && go.contains("s2.len()") && go.contains("s3.len()") && go.contains("s4.len()"),
        "expected .len() lowering on s1, s2, s3, and aliased s4, got:\n{go}"
    );
}

#[test]
fn parallel_parenthesized_moved_and_new_struct_receivers_normalize_methods() {
    let go = emit_src(
        r#"
import "sync"

struct pool_stats {
    mu sync.mutex
}

fn main() {
    c, d := pool_stats{}, (pool_stats{})
    c.mu.lock()
    c.mu.unlock()
    d.mu.lock()
    d.mu.unlock()
    e := move c
    e.mu.lock()
    e.mu.unlock()
    p := new(pool_stats)
    p.mu.lock()
    p.mu.unlock()
}
"#,
    );
    assert!(
        go.contains("c.mu.Lock()") && go.contains("c.mu.Unlock()"),
        "expected c.mu.Lock()/Unlock() for parallel struct c, got:\n{go}"
    );
    assert!(
        go.contains("d.mu.Lock()") && go.contains("d.mu.Unlock()"),
        "expected d.mu.Lock()/Unlock() for parenthesized struct d, got:\n{go}"
    );
    assert!(
        go.contains("e.mu.Lock()") && go.contains("e.mu.Unlock()"),
        "expected e.mu.Lock()/Unlock() for moved struct e, got:\n{go}"
    );
    assert!(
        go.contains("p.mu.Lock()") && go.contains("p.mu.Unlock()"),
        "expected p.mu.Lock()/Unlock() for new(pool_stats) pointer p, got:\n{go}"
    );
}
