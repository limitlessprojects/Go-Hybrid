//! copy_may_share.rs — regression for issue #178: builtin `copy(dst, src)`
//! was never modelled by the checker. For slices whose *elements* are
//! reference types (`[]map[...]`, …), `copy` duplicates element references,
//! so `dst[i]` and `src[i]` alias. A later `move dst` must freeze `src`
//! (HYB-E3003 / O5.2). Before the fix, `process_call` resolved
//! `Resolved::Fn { name: "copy" }` with no arm, so the racing program
//! passed with 0 diagnostics while `go run -race` reported concurrent map
//! writes. By-value elements (`[]int`) share nothing and must still accept.

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

/// Issue #178 racing reproducer: `copy` of `[]map[string]int` shares the
/// map element; moving `dst` must freeze `src`.
const COPY_MAP_ELEM_RACE: &str = "\
fn worker(d []map[string]int, done chan<- bool) {
    for i in 0..100000 {
        d[0][\"k\"] = i
    }
    done <- true
}

fn main() {
    src := []map[string]int{map[string]int{\"k\": 0}}
    dst := make([]map[string]int, 1)
    copy(dst, src)
    done := make(chan bool, 1)
    task worker(move dst, done)
    for i in 0..100000 {
        src[0][\"k\"] = i
    }
    <-done
}
";

#[test]
fn copy_of_map_elem_slice_freezes_peer_e3003() {
    let r = check(COPY_MAP_ELEM_RACE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for post-move write through copy peer, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

/// Precision control: `copy` of `[]int` copies by-value elements — no
/// may-share edge — so moving `dst` must not freeze `src`.
const COPY_INT_SLICE_SAFE: &str = "\
fn worker(d []int, done chan<- bool) {
    for i in 0..1000 {
        d[0] = i
    }
    done <- true
}

fn main() {
    src := []int{0}
    dst := make([]int, 1)
    copy(dst, src)
    done := make(chan bool, 1)
    task worker(move dst, done)
    for i in 0..1000 {
        src[0] = i
    }
    <-done
}
";

#[test]
fn copy_of_int_slice_stays_accepted() {
    let r = check(COPY_INT_SLICE_SAFE);
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E3003"),
        "[]int copy must not freeze the peer (by-value elements), got {:?}",
        cs
    );
    assert!(
        cs.is_empty(),
        "expected 0 error diagnostics for safe []int copy, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
