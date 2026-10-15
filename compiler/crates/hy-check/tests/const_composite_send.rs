//! const_composite_send.rs — regression tests for issue #188:
//! const composites (maps, slices, structs with references) are runtime-mutable
//! in Go runtime (lowered to package vars / heap objects). They must NOT be
//! classified as Class 4 sendable across tasks or channels without `move`/`managed`.

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

// ---------------------------------------------------------------------------
// Rejects — const composites crossing task or channel boundaries (issue #188)
// ---------------------------------------------------------------------------

#[test]
fn file_const_map_task_spawn_is_e3001() {
    // Exact repro from issue #188: const shared_map passed to task worker
    let r = check(
        "const shared_map = map[string]int{\"k\": 0}\n\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done1 := make(chan bool, 1)\n\
             task worker(shared_map, done1)\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for passing const map across task spawn, got {:?}",
        cs
    );
}

#[test]
fn file_const_map_chan_send_is_e3002() {
    // Variant 1: const map sent over channel
    let r = check(
        "const shared_map = map[string]int{\"k\": 0}\n\n\
         fn main() {\n\
             ch := make(chan map[string]int, 1)\n\
             ch <- shared_map\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 for sending const map over channel, got {:?}",
        cs
    );
}

#[test]
fn fn_local_const_map_task_spawn_is_e3001() {
    // Variant 2: function-local const map passed to task
    let r = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             const local_map = map[string]int{\"k\": 0}\n\
             done := make(chan bool, 1)\n\
             task worker(local_map, done)\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for passing fn-local const map across task spawn, got {:?}",
        cs
    );
}

#[test]
fn file_const_slice_task_spawn_is_e3001() {
    // Variant 3: const slice passed to task
    let r = check(
        "const shared_slice = []int{0}\n\n\
         fn worker(s []int, done chan<- bool) {\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             task worker(shared_slice, done)\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for passing const slice across task spawn, got {:?}",
        cs
    );
}

// ---------------------------------------------------------------------------
// Positive controls — primitives and immutable scalar projections stay legal
// ---------------------------------------------------------------------------

#[test]
fn primitive_const_task_spawn_accepted() {
    let r = check(
        "const budget = 3\n\
         const msg = \"hello\"\n\n\
         fn worker(b int, s string, done chan<- bool) {\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             task worker(budget, msg, done)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn primitive_const_chan_send_accepted() {
    let r = check(
        "const budget = 42\n\n\
         fn main() {\n\
             ch := make(chan int, 1)\n\
             ch <- budget\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn const_map_scalar_index_read_accepted() {
    let r = check(
        "const shared_map = map[string]int{\"k\": 42}\n\n\
         fn worker(val int, done chan<- bool) {\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             task worker(shared_map[\"k\"], done)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
