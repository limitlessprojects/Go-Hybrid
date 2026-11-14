use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics.iter().map(|d| d.code.to_string()).collect()
}

// ---------------------------------------------------------------------------
// issue #4457 — class 2 for `sync/atomic` is only true BEHIND `&`.
//
// An atomic crossed by value is a COPY: the receiver gets its own atomic, so
// every write it performs dies at its return. The class-2 rationale
// ("internally synchronized by construction") covers the instance, not a copy,
// and `go vet`'s copylocks fires on exactly the by-value call.
// ---------------------------------------------------------------------------

#[test]
fn spawn_value_parameter_atomic_by_value_rejects_e3001() {
    let src = r#"
import "sync/atomic"

fn bump(a atomic.int64, done chan<- int) {
    a.add(1)
    done <- 1
}

fn main() {
    var hits atomic.int64
    done := make(chan int, 1)
    task bump(hits, done)
    _ = <-done
    fmt.println("{hits.load()}")
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "{:?}", res.diagnostics);
    assert!(codes(&res).iter().any(|c| c == "HYB-E3001"), "{:?}", res.diagnostics);
    let rendered = format!("{:?}", res.diagnostics);
    assert!(
        rendered.contains("`&x`"),
        "the fix-it must name the mediated route: {rendered}"
    );
}

#[test]
fn spawn_value_parameter_atomic_pointer_accepts() {
    let src = r#"
import "sync/atomic"

fn bump(p *atomic.int64, done chan<- int) {
    p.add(1)
    done <- 1
}

fn main() {
    var hits atomic.int64
    done := make(chan int, 1)
    task bump(&hits, done)
    _ = <-done
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn spawn_pointer_parameter_still_auto_addresses_atomic_value() {
    // issue #694 contract: a bare atomic against a POINTER parameter takes the
    // mediated route automatically, so the rejection must not reach it.
    let src = r#"
import "sync/atomic"

fn bump(p *atomic.int64, done chan<- int) {
    p.add(1)
    done <- 1
}

fn main() {
    var hits atomic.int64
    done := make(chan int, 1)
    task bump(hits, done)
    _ = <-done
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn channel_send_of_bare_atomic_rejects_e3002() {
    // A channel cannot auto-address its element type, so `chan atomic.int64`
    // takes a copy regardless: same loss, send site.
    let src = r#"
import "sync/atomic"

fn main() {
    ch := make(chan atomic.int64, 1)
    var v atomic.int64
    ch <- v
    got := <-ch
    got.add(1)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "{:?}", res.diagnostics);
    assert!(codes(&res).iter().any(|c| c == "HYB-E3002"), "{:?}", res.diagnostics);
}

#[test]
fn channel_send_of_atomic_pointer_accepts() {
    let src = r#"
import "sync/atomic"

fn main() {
    ch := make(chan *atomic.int64, 1)
    var v atomic.int64
    ch <- &v
    _ = <-ch
    v.add(100)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn atomic_pointer_send_from_field_accepts() {
    // The mediated route through a struct field (`&s.hits`), the corpus-17
    // shape inside a managed struct.
    let src = r#"
import "sync/atomic"

struct counter {
    hits atomic.int64
}

fn main() {
    var c counter
    ch := make(chan *atomic.int64, 1)
    ch <- &c.hits
    _ = <-ch
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn unproven_atomic_call_return_stays_e3005() {
    // `atomic.value()` is a call return whose type does not resolve, so it
    // keeps its own fail-closed code (issue #51 / #4169); this fix must not
    // relabel it.
    let src = r#"
import "sync/atomic"

fn w(x any) { _ = x }

fn f() {
    v := atomic.value()
    task w(v)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "{:?}", res.diagnostics);
    assert!(codes(&res).iter().any(|c| c == "HYB-E3005"), "{:?}", res.diagnostics);
}
