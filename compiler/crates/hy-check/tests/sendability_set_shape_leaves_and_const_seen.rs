//! Regression tests for `sendability.rs`:
//!   1. `shape_holds_unmovable_leaf`, `shape_holds_closure`, and `shape_holds_unproven_type_param`
//!      walking `set[T]` element types inside composite/struct shapes when `b.ty` is absent
//!      (e.g. `select` receive or `for` iteration over a channel inside a closure).
//!   2. Popping the `seen` recursion stack across sibling struct fields and recognizing
//!      zero-field structs (`struct empty {}`) in `shape_is_const_sendable` (Class-4 `const`
//!      sendability).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parses");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn rejects_moving_containers_of_set_with_foreign_or_unproven_elements_in_closure_select_and_for() {
    assert_eq!(
        codes(
            r#"import "fmt"
import "os"
struct holder {
    s set[*os.file]
}
fn worker(h holder) {
    fmt.println(len(h.s))
}
fn main() {
    ch := make(chan holder, 1)
    f := fn() {
        select {
            h := <-ch: task worker(move h)
            default: fmt.println(0)
        }
    }
    f()
}
"#,
        ),
        vec!["HYB-E3001"],
        "moving `holder` carrying `set[*os.file]` from a closure `select` receive must reject HYB-E3001"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
import "sync/atomic"
struct holder {
    s set[atomic.value]
}
fn worker(h holder) {
    fmt.println(len(h.s))
}
fn main() {
    ch := make(chan holder, 1)
    f := fn() {
        for h in ch {
            task worker(move h)
        }
    }
    f()
}
"#,
        ),
        vec!["HYB-E3005"],
        "moving `holder` carrying `set[atomic.value]` from a closure `for` loop must reject HYB-E3005"
    );
}

#[test]
fn accepts_const_structs_with_repeated_sibling_struct_fields_and_zero_field_structs() {
    assert!(
        codes(
            r#"import "fmt"
struct empty {}
struct point {
    x int
}
struct segment {
    a point
    b point
    tag empty
}
fn worker(s segment, e empty) {
    _ = e
    fmt.println(s.a.x + s.b.x)
}
fn main() {
    const seg = segment{a: point{x: 1}, b: point{x: 2}, tag: empty{}}
    const e = empty{}
    task worker(seg, e)
}
"#,
        )
        .is_empty(),
        "Class-4 `const` struct with sibling fields of the same struct type and zero-field structs must be sendable"
    );
}
