//! prelude_name_collision_777.rs — regression tests for Issue #777:
//! user declarations named some/option/none/hy_pop/set/len collide with the injected prelude.
//!
//! Go keeps a single package block for top-level declarations. When Hybrid
//! injects its standard library prelude (`type option[v any] struct`,
//! `func some[v any](val v) option[v]`, `type set[t comparable] struct`,
//! `func hy_pop[t any](s *[]t) t`), user declarations with these names
//! collide and fail `go build` with `<name> redeclared in this block` or
//! capture calls made by the prelude (`len`).
//!
//! All such collisions reject fail-closed as HYB-E1002 (rule `BUILDABLE`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_777.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("snippet must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(r: &hy_check::CheckResult, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for {}", what);
}

#[test]
fn test_fn_some_repro_a_collides_with_prelude_777() {
    let src = r#"
fn some(x int) int {
    return x + 1
}

fn has(o option[int]) bool {
    return o.is_some()
}

fn main() {
    n := some(1)
    fmt.println("{n}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user fn some with option[T]");
}

#[test]
fn test_fn_some_alone_collides_with_prelude_777() {
    let src = r#"
fn some(x int) int {
    return x + 1
}

fn main() {
    n := some(1)
    fmt.println("{n}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user fn some alone");
}

#[test]
fn test_struct_option_repro_b_collides_with_prelude_777() {
    let src = r#"
struct option {
    n int
}

fn has(o option[int]) bool {
    return o.is_some()
}

fn main() {
    v := option{n: 1}
    fmt.println("{v.n}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user struct option with option[T]");
}

#[test]
fn test_struct_option_alone_collides_with_prelude_777() {
    let src = r#"
struct option {
    n int
}

fn main() {
    v := option{n: 1}
    fmt.println("{v.n}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user struct option alone");
}

#[test]
fn test_type_alias_option_collides_with_prelude_777() {
    let src = r#"
type option = int

fn main() {
    var x option = 1
    fmt.println(x)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user type alias option");
}

#[test]
fn test_enum_option_collides_with_prelude_777() {
    let src = r#"
enum option {
    A,
    B
}

fn main() {}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user enum option");
}

#[test]
fn test_const_some_collides_with_prelude_777() {
    let src = r#"
const some = 42

fn main() {
    fmt.println(some)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user const some");
}

#[test]
fn test_fn_none_collides_with_prelude_777() {
    let src = r#"
fn none() int {
    return 0
}

fn main() {
    fmt.println(none())
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user fn none");
}

#[test]
fn test_fn_hy_pop_collides_with_prelude_777() {
    let src = r#"
fn hy_pop() int {
    return 0
}

fn main() {
    fmt.println(hy_pop())
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user fn hy_pop");
}

#[test]
fn test_struct_set_collides_with_prelude_777() {
    let src = r#"
struct set {
    n int
}

fn main() {
    s := set{n: 1}
    fmt.println(s.n)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user struct set");
}

#[test]
fn test_type_set_collides_with_prelude_777() {
    let src = r#"
type set = int

fn main() {
    var s set = 5
    fmt.println(s)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user type alias set");
}

#[test]
fn test_fn_set_collides_when_set_prelude_used_777() {
    let src = r#"
fn set(x int) int {
    return x
}

fn main() {
    s := set[int]{}
    s.insert(1)
    fmt.println(set(10))
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user fn set with set prelude");
}

#[test]
fn test_fn_len_collides_when_set_prelude_used_777() {
    let src = r#"
fn len(xs []int) int {
    return 42
}

fn main() {
    s := set[int]{}
    s.insert(1)
    fmt.println("{len([]int{1})}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "user fn len capturing set prelude len");
}

#[test]
fn test_method_named_some_or_len_accepted_777() {
    let src = r#"
struct worker {
    id int
}

fn (w worker) some() int {
    return w.id
}

fn (w worker) len() int {
    return 1
}

fn (w worker) set() int {
    return 2
}

fn main() {
    w := worker{id: 10}
    fmt.println(w.some(), w.len(), w.set())
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "receiver methods named some/len/set live in method sets, not package block: {:?}",
        r.diagnostics
    );
}

#[test]
fn test_fn_set_without_set_prelude_accepted_777() {
    let src = r#"
fn set(dst []map[string]int, v map[string]int) {
    dst[0] = v
}

fn main() {
    m := [map[string]int{}]
    set(m, map[string]int{"a": 1})
    fmt.println(m)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "helper fn set without set prelude remains accepted: {:?}",
        r.diagnostics
    );
}

#[test]
fn test_valid_prelude_usage_accepted_777() {
    let src = r#"
fn maybe(x int) option[int] {
    if x > 0 {
        return some(x)
    }
    return none
}

fn main() {
    o := maybe(5)
    s := set[int]{}
    s.insert(1)
    fmt.println(o.is_some(), s.contains(1))
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "normal prelude usage must be accepted: {:?}",
        r.diagnostics
    );
}
