//! Regression tests for issues #747 and #748:
//! - #747: pointer-receiver method calls (`c.bump()`, `task c.bump()`, `defer c.bump()`)
//!   on `const` structs must emit `HYB-E3006` (T22).
//! - #748: parenthesized call expressions (`(delete)(m, k)`, `(clear)(m)`, `(sort.ints)(c)`,
//!   `(doc.drop)()`) and `(&c).field = v` must not bypass `process_call` or `const_write_root`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn ptr_receiver_method_mutating_const_struct_is_rejected_e3006() {
    let src = r#"
struct counter {
    n int
}

fn (c *counter) bump() {
    c.n = c.n + 1
}

fn main() {
    const c = counter{n: 0}
    c.bump()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on c.bump(), got: {:?}",
        r.diagnostics
    );
}

#[test]
fn task_and_defer_ptr_receiver_method_mutating_const_struct_emit_e3006() {
    for call_stmt in ["task c.bump()", "defer c.bump()"] {
        let src = format!(
            r#"
struct counter {{
    n int
}}

fn (c *counter) bump() {{
    c.n = c.n + 1
}}

fn main() {{
    const c = counter{{n: 0}}
    {call_stmt}
}}
"#
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            error_codes(&r).iter().any(|c| c == "HYB-E3006"),
            "expected HYB-E3006 on `{call_stmt}`, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn generic_ptr_receiver_method_mutating_const_struct_is_rejected_e3006() {
    let src = r#"
struct box[t any] {
    v t
}

fn (b *box[t]) set(x t) {
    b.v = x
}

fn main() {
    const b = box[int]{v: 0}
    b.set(42)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on generic pointer-receiver mutation of const, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn read_only_ptr_receiver_method_on_const_and_mutable_receiver_are_accepted() {
    let src = r#"
import "fmt"

struct counter {
    n int
}

fn (c *counter) bump() {
    c.n = c.n + 1
}

fn (c *counter) get() int {
    return c.n
}

fn main() {
    const c = counter{n: 7}
    v := c.get()
    m := counter{n: v}
    m.bump()
    fmt.println(m.get())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn parenthesized_delete_and_clear_in_map_slot_loan_window_reject_e3102() {
    for op in ["(delete)(m, \"a\")", "(clear)(m)"] {
        let src = format!(
            r#"
import "fmt"

struct entry {{
    n int
}}

fn main() {{
    m := map[string]entry{{"a": entry{{n: 1}}}}
    p := &m["a"]
    {op}
    p.n = 2
    fmt.println(p.n)
}}
"#
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            error_codes(&r).iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 on `{op}` during live slot loan, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn parenthesized_sort_and_addr_of_place_write_to_const_reject_e3006() {
    let src1 = r#"
import "sort"

fn main() {
    const xs = []int{3, 1, 2}
    (sort.ints)(xs)
}
"#;
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
    assert!(
        error_codes(&r1).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on (sort.ints)(xs), got: {:?}",
        r1.diagnostics
    );

    let src2 = r#"
import "sort"

fn main() {
    const xs = []int{3, 1, 2}
    (sort).ints(xs)
}
"#;
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert!(
        error_codes(&r2).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on (sort).ints(xs), got: {:?}",
        r2.diagnostics
    );

    let src3 = r#"
struct counter {
    n int
}

const c = counter{n: 0}

fn main() {
    &c.n = 1
}
"#;
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject, "{:?}", r3.diagnostics);
    assert!(
        error_codes(&r3).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on &c.n = 1, got: {:?}",
        r3.diagnostics
    );
}

#[test]
fn task_spawn_of_ptr_receiver_and_non_sendable_value_receiver_reject_e3001() {
    let ptr_recv_src = r#"
import "fmt"

struct counter {
    n int
}

fn (c *counter) bump() {
    c.n = c.n + 1
}

fn main() {
    var c = counter{n: 0}
    task c.bump()
    c.bump()
    fmt.println(c.n)
}
"#;
    let r_ptr = check(ptr_recv_src);
    assert_eq!(r_ptr.verdict, Verdict::Reject, "{:?}", r_ptr.diagnostics);
    assert!(
        error_codes(&r_ptr).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 on `task c.bump()`, got: {:?}",
        r_ptr.diagnostics
    );

    let val_recv_ok_src = r#"
import "fmt"

struct counter {
    n int
}

fn (c counter) show() {
    fmt.println(c.n)
}

fn main() {
    var c = counter{n: 5}
    task (move c).show()
}
"#;
    let r_ok = check(val_recv_ok_src);
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);

    let val_recv_map_src = r#"
struct holder {
    m map[string]int
}

fn (h holder) work() {}

fn main() {
    var h = holder{m: map[string]int{"a": 1}}
    task h.work()
}
"#;
    let r_map = check(val_recv_map_src);
    assert_eq!(r_map.verdict, Verdict::Reject, "{:?}", r_map.diagnostics);
    assert!(
        error_codes(&r_map).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 on `task h.work()` without move, got: {:?}",
        r_map.diagnostics
    );
}

#[test]
fn parenthesized_pure_pkg_call_in_helper_does_not_falsely_reject_const_arg() {
    let src = r#"
import "fmt"

const global_m = map[string]int{"a": 1}

fn inspect(m map[string]int) {
    (fmt).println(m)
}

fn main() {
    inspect(global_m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}
