//! generic_struct_shape_soundness_issue_775.rs — regression tests for #775:
//! generic struct TypeShape erasure dropped type arguments in shape_may_share,
//! shape_holds_closure, and field_borrow_kind, causing uninstantiated field types
//! like `t` to resolve to `TypeShape::Unknown` and bypassing O5.2 (HYB-E3003) and
//! A5 (HYB-E3001) with runtime data races.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const GENERIC_BOX_SLICE_MOVE_SHARE: &str = r#"
struct box[t] {
    v t
}

fn worker(b box[[]int], done chan bool) {
    b.v[0] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    b1 := box[[]int]{v: []int{0}}
    b2 := b1
    task worker(move b1, done)
    b2.v[0] = 2
    <-done
}
"#;

const GENERIC_BOX_CLOSURE_MOVE: &str = r#"
type task_fn = fn()

struct box[t] {
    v t
}

fn runner(b box[task_fn], done chan bool) {
    (b.v)()
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    shared := 0
    f := fn() { shared = 1 }
    b := box[task_fn]{v: f}
    task runner(move b, done)
    <-done
    _ = shared
}
"#;

#[test]
fn generic_struct_copy_with_slice_field_freezes_peer_on_move() {
    let got = codes(&check(GENERIC_BOX_SLICE_MOVE_SHARE));
    assert!(
        got.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 (write through live alias of moved value), got {:?}",
        got
    );
}

#[test]
fn generic_struct_with_closure_field_rejects_task_move() {
    let got = codes(&check(GENERIC_BOX_CLOSURE_MOVE));
    assert!(
        got.iter().any(|c| c == "HYB-E3001" || c == "HYB-E3005"),
        "expected HYB-E3001/HYB-E3005 for closure in generic struct, got {:?}",
        got
    );
}
