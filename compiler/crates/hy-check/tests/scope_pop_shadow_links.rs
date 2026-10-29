//! scope_pop_shadow_links.rs -- regression tests for issues #811 and #797.
//!
//! Both bugs are ONE defect site: `FnState::scope_pop` (model.rs). When an
//! outer binding is shadowed inside a block and that block establishes a
//! storage-sharing edge between two *surviving outer* bindings, the edge is
//! recorded as `shadow_links[holder] += (shadowed_outer, depth)` and the
//! shadowed outer is severed from `may_share`/`aliases`. `scope_pop` then
//! discarded `shadow_links` wholesale, so the edge existed only inside the
//! block. After the block the checker believed the two outer bindings were
//! independent: `task worker(move a)` no longer froze the peer, and a write
//! through the surviving peer passed with 0 diagnostics.
//!
//! #797 covers the explicit block form (`if true { s2 = s1; s1 := 0 }`).
//! #811 covers the `for ... in` form, where the loop variables are defined
//! in a scope layer that is popped after the body.
//!
//! Every Reject assertion below was written and observed FAILING on base
//! 1f0ff97 before the fix landed (see bounties/reports/fix-811-797.md).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str, want: &[&str], what: &str) {
    let r = check(src);
    let got = codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "{}: expected Reject, got {:?} diagnostics {:?}",
        what, r.verdict, got
    );
    for w in want {
        assert!(
            got.iter().any(|c| c == w),
            "{}: expected {} among {:?}",
            what, w, got
        );
    }
}

fn assert_accept(src: &str, what: &str) {
    let r = check(src);
    let got = codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "{}: expected Accept, got diagnostics {:?}",
        what, got
    );
}

// ---------------------------------------------------------------------------
// #797 -- block scope: `s2 = s1` then shadow ONE of the two outer bindings.
// The surviving pair must still be treated as sharing storage after the
// block exits, so moving one freezes the other (O5.2 / HYB-E3003).
// ---------------------------------------------------------------------------

const R797_SHADOW_SOURCE: &str = r#"
fn worker(s []int, done chan<- bool) {
    s[0] = 1
    done <- true
}

fn main() {
    s1 := []int{0}
    var s2 []int
    done := make(chan bool, 1)
    if true {
        s2 = s1
        s1 := 0
        _ = s1
    }
    task worker(move s2, done)
    s1[0] = 2
    <-done
}
"#;

#[test]
fn scope_pop_preserves_share_when_share_target_is_shadowed() {
    assert_reject(R797_SHADOW_SOURCE, &["HYB-E3003"], "#797 shadow the share target");
}

const R797_SHADOW_DEST: &str = r#"
fn worker(s []int, done chan<- bool) {
    s[0] = 1
    done <- true
}

fn main() {
    s1 := []int{0}
    var s2 []int
    done := make(chan bool, 1)
    if true {
        s2 = s1
        s2 := 0
        _ = s2
    }
    task worker(move s2, done)
    s1[0] = 2
    <-done
}
"#;

#[test]
fn scope_pop_preserves_share_when_share_holder_is_shadowed() {
    assert_reject(R797_SHADOW_DEST, &["HYB-E3003"], "#797 shadow the share holder");
}

const R797_SHADOW_PARAM: &str = r#"
fn worker(s []int, done chan<- bool) {
    s[0] = 1
    done <- true
}

fn race(p [][]int) {
    s2 := []int{0}
    done := make(chan bool, 1)
    if true {
        p[0] = s2
        s2 := 0
        _ = s2
    }
    task worker(move s2, done)
    p[0][0] = 3
    <-done
}
"#;

#[test]
fn scope_pop_preserves_non_owned_origin_through_shadow() {
    // O5.1: s2's storage is reachable from the non-owned parameter p, so
    // moving s2 is a non-owned move (HYB-E3004), and the later write through
    // p is a use through a live alias of the moved value (E3003).
    let r = check(R797_SHADOW_PARAM);
    let got = codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "#797 shadowed param, got {:?}", got);
    assert!(
        got.iter().any(|c| c == "HYB-E3003" || c == "HYB-E3004"),
        "#797 shadowed param: expected E3003 or E3004, got {:?}",
        got
    );
}

const R797_SHADOW_CONST: &str = r#"
const k = [][]int{[]int{0}}

fn link(dst [][]int, src []int) {
    dst[0] = src
}

fn main() {
    s2 := [][]int{[]int{0}}
    if true {
        link(s2, k[0])
        s2 := 0
        _ = s2
    }
    s2[0][0] = 99
}
"#;

#[test]
fn scope_pop_preserves_const_root_through_shadow() {
    // T22: s2 shares with const k; the shadow must not launder the write.
    assert_reject(R797_SHADOW_CONST, &["HYB-E3006"], "#797 shadowed const root");
}

// ---------------------------------------------------------------------------
// #811 -- `for ... in` loop-variable shadowing of the collection root.
// ---------------------------------------------------------------------------

const R811_TWO_VAR: &str = r#"
fn worker(a [][]int) {
    a[0][0] = 99
}

fn main() {
    a := [][]int{[]int{1, 2}}
    saved := []int{0}
    for a, b in a {
        _ = a
        saved = b
    }
    task worker(move a)
    saved[0] = 42
}
"#;

#[test]
fn iterate_index_var_shadowing_collection_keeps_share_edge() {
    assert_reject(R811_TWO_VAR, &["HYB-E3003"], "#811 for a, b in a");
}

const R811_SELF_SHADOW: &str = r#"
fn worker(xs [][]int) {
    xs[0][0] = 99
}

fn main() {
    xs := [][]int{[]int{1, 2}}
    alias := xs
    for xs in xs {
        task worker(move alias)
        xs[0] = 42
        break
    }
}
"#;

#[test]
fn iterate_element_var_self_shadowing_collection_keeps_share_edge() {
    assert_reject(R811_SELF_SHADOW, &["HYB-E3003"], "#811 for xs in xs");
}

const R811_CONST: &str = r#"
const table = [][]int{[]int{1, 2}}

fn main() {
    for table in table {
        table[0] = 99
    }
}
"#;

#[test]
fn iterate_var_shadowing_const_collection_rejects_const_write() {
    assert_reject(R811_CONST, &["HYB-E3006"], "#811 for table in table over const");
}

// ---------------------------------------------------------------------------
// Positive controls -- legal shadowing must stay ACCEPTED. A fix that
// rejects everything is not a fix.
// ---------------------------------------------------------------------------

#[test]
fn positive_plain_shadow_of_container_is_accepted() {
    assert_accept(
        r#"
fn main() {
    x := []int{0}
    if true {
        x := []int{1}
        x[0] = 5
    }
    x[0] = 7
}
"#,
        "legal shadow: independent inner slice",
    );
}

#[test]
fn positive_shadow_with_share_but_no_move_is_accepted() {
    assert_accept(
        r#"
fn main() {
    s1 := []int{0}
    var s2 []int
    if true {
        s2 = s1
        s1 := 0
        _ = s1
    }
    s1[0] = 2
    s2[0] = 3
}
"#,
        "legal: shared pair, no move, no const",
    );
}

#[test]
fn positive_shadow_then_move_one_never_use_other_is_accepted() {
    assert_accept(
        r#"
fn worker(s []int) {
    s[0] = 1
}

fn main() {
    s1 := []int{0}
    var s2 []int
    if true {
        s2 = s1
        s1 := 0
        _ = s1
    }
    task worker(move s2)
}
"#,
        "legal: move one peer, never touch the other",
    );
}

#[test]
fn positive_iterate_shadow_without_escaping_alias_is_accepted() {
    assert_accept(
        r#"
fn main() {
    xs := [][]int{[]int{1, 2}}
    n := 0
    for xs in xs {
        n = n + xs[0]
    }
}
"#,
        "legal: for xs in xs with no escaping share",
    );
}

#[test]
fn positive_iterate_two_var_no_shadow_is_accepted() {
    assert_accept(
        r#"
fn main() {
    a := [][]int{[]int{1, 2}}
    saved := 0
    for i, b in a {
        saved = i + b[0]
    }
    _ = saved
}
"#,
        "legal: for i, b in a no shadow",
    );
}
