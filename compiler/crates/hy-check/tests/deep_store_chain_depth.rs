//! deep_store_chain_depth.rs — the interprocedural parameter-store analysis is
//! DEPTH-INDEPENDENT, and these tests pin that.
//!
//! `loans.rs::collect_callee_param_stores` walks a callee's body to record that
//! one parameter's storage was stored into another's. The walk is bounded
//! (depth <= 4, and it needs at least two parameter slots), which reads like a
//! hole: a chain of helpers deeper than the bound would record no `may_share`
//! edge, so `move q` would freeze only `q`, and the caller's other binding
//! would stay writable while the spawned task writes the same map.
//!
//! It is not a hole. These two programs are the probe that falsified it —
//! verified against a built `hyb` at main `aa4a9d9`: the depth-5 chain and its
//! depth-2 control BOTH reject with HYB-E3003, and the diagnostic names the
//! alias explicitly ("`shared` aliases `h` — the freeze covers every live
//! alias"). The alias is therefore discovered without relying on the bounded
//! walk: `h`'s shape is a struct with a map field, and the store edge is
//! recorded from the callee's own signature/body shape rather than only from
//! the depth-limited chain.
//!
//! This cell had NO corpus or unit coverage before this file (an independent
//! survey of the sharing layer listed "interprocedural chains > 4 deep" as
//! uncovered). The value here is regression direction: if a future change
//! re-introduces a real depth bound, the depth-5 case flips to ACCEPT and this
//! test fails, where nothing would have failed before.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

/// Build the chain: `l1` performs the store, `l2..=lN` forward to it.
/// `depth` is the number of forwarding frames, so depth 1 = `main` calls `l1`
/// directly and depth 5 = `main -> l5 -> l4 -> l3 -> l2 -> l1`.
fn chain(depth: usize) -> String {
    let mut src = String::from(
        "\
struct holder {
  m map[string]int
}

fn worker(h holder, done chan int) {
  h.m[\"from_task\"] = 1
  done <- 1
}

fn l1(p map[string]int, q holder) {
  q.m = p
}

",
    );
    for n in 2..=depth {
        src.push_str(&format!(
            "fn l{n}(p map[string]int, q holder) {{\n  l{}(p, q)\n}}\n\n",
            n - 1
        ));
    }
    src.push_str(&format!(
        "\
fn main() {{
  shared := map[string]int{{\"k\": 0}}
  var h holder
  l{depth}(shared, h)
  done := make(chan int, 1)
  task worker(move h, done)
  shared[\"from_main\"] = 2
  _ = <-done
  fmt.println(\"done\")
}}
"
    ));
    src
}

/// The two-parameter chain at the bound the walk actually covers.
#[test]
fn store_chain_at_depth_2_rejects_e3003() {
    let r = check(&chain(2));
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "depth-2 store chain must reject; got {:?}",
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        codes(&r).contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003; got {:?}",
        codes(&r)
    );
}

/// One frame PAST the walk's depth bound. This is the probe that falsified the
/// "depth > 4 loses the alias edge" hypothesis — it must reject too.
#[test]
fn store_chain_at_depth_5_still_rejects_e3003() {
    let r = check(&chain(5));
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "depth-5 store chain must still reject — the freeze reaches `shared` \
         through `h`, so a deeper chain must not turn this into an accepted race; got {:?}",
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        codes(&r).contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003; got {:?}",
        codes(&r)
    );
}

/// Fence the other direction: the store is what links the two bindings. With no
/// store anywhere in the chain there is no alias, and the spawn is legal — so
/// these tests cannot be satisfied by a rule that rejects every deep chain.
#[test]
fn deep_chain_without_the_store_is_accepted() {
    let src = "\
struct holder {
  m map[string]int
}

fn worker(h holder, done chan int) {
  h.m[\"from_task\"] = 1
  done <- 1
}

fn l1(q holder) {
  q.m = map[string]int{\"fresh\": 1}
}

fn l2(q holder) {
  l1(q)
}

fn l3(q holder) {
  l2(q)
}

fn l4(q holder) {
  l3(q)
}

fn l5(q holder) {
  l4(q)
}

fn main() {
  shared := map[string]int{\"k\": 0}
  var h holder
  l5(h)
  done := make(chan int, 1)
  task worker(h, done)
  shared[\"from_main\"] = 2
  _ = <-done
  fmt.println(\"done\")
}
";
    let r = check(src);
    // `shared` is never handed to the callee, so it cannot be an alias of `h`;
    // the spawn passes `h` by value (a struct copy) and the later write to
    // `shared` is independent. This must not be rejected as a moved-alias write.
    assert!(
        !codes(&r).contains(&"HYB-E3003".to_string()),
        "a deep chain that never stores `shared` must not be reported as a \
         moved-alias write; got {:?}",
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}
