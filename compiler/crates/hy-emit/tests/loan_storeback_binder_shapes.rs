//! loan_storeback_binder_shapes.rs — a live O8 map-slot loan's store-back must
//! be flushed BEFORE the spawn that moves the container, for every binder shape
//! that can shadow the loan binding.
//!
//! This class has regressed twice already: fixtures 62/63 in `tests/checker-corpus`
//! are registered ACCEPT precisely because the checker's name-keyed window was
//! right while the EMITTER put the store-back after the spawn (#547, #551). The
//! checker cannot catch this class — the source is sound, only the emission
//! order is wrong — so an emitter test is the only guard.
//!
//! `stmt_must_pre_flush_loan` in `hy-emit/src/stmt.rs` enumerates the binder
//! shapes it pre-flushes (If-init, Match-arm binding, Select-arm binding, For
//! binder). An independent survey of the sharing layer listed two shapes NOT in
//! that list as uncovered: a bare-`Block`-local binder and a `select` RECV
//! binder. Both were probed against a built `hyb` at main `aa4a9d9` and both
//! emit the store-back BEFORE the `go` statement, so neither is a live bug —
//! these tests pin that, so a future change to the handoff/pre-flush logic
//! cannot silently regress the two shapes that were never written down.
//!
//! The assertion is an ORDER assertion on the emitted Go, not a presence check:
//! presence alone would pass even if the store-back moved after the spawn.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

/// Assert `before` appears before `after` in the emitted Go, and that both are
/// present at all (a missing store-back would otherwise pass an index compare
/// if both were absent).
fn assert_ordered(go: &str, before: &str, after: &str, what: &str) {
    let ib = go.find(before).unwrap_or_else(|| {
        panic!("{what}: expected `{before}` in the emitted Go:\n{go}")
    });
    let ia = go
        .find(after)
        .unwrap_or_else(|| panic!("{what}: expected `{after}` in the emitted Go:\n{go}"));
    assert!(
        ib < ia,
        "{what}: the loan store-back `{before}` must be flushed BEFORE `{after}` \
         — otherwise the spawned task and the sender both write the same map \
         (concurrent map writes). Emitted Go:\n{go}"
    );
}

const PRELUDE: &str = "\
struct service {
  replicas int
}

fn worker(m map[string]service, done chan int) {
  m[\"a\"] = service{replicas: 9}
  done <- 1
}

";

#[test]
fn storeback_precedes_spawn_with_a_bare_block_shadow_binder() {
    let mut src = String::from(PRELUDE);
    src.push_str(
        "\
fn main() {
  m := map[string]service{\"a\": service{replicas: 1}}
  n := &m[\"a\"]
  {
    n := 2
    fmt.println(\"{n}\")
  }
  n.replicas = 5
  done := make(chan int, 1)
  task worker(move m, done)
  _ = <-done
  fmt.println(\"done\")
}
",
    );
    let go = emit(&src);
    // The store-back reuses `m`; the spawn moves `m`.
    assert_ordered(&go, "m[\"a\"] = n", "go worker(m, done)", "bare-Block binder");
}

#[test]
fn storeback_precedes_spawn_with_a_select_recv_binder() {
    let mut src = String::from(PRELUDE);
    src.push_str(
        "\
fn main() {
  ch := make(chan int, 1)
  ch <- 7
  m := map[string]service{\"a\": service{replicas: 1}}
  n := &m[\"a\"]
  select {
    n := <-ch: {
      fmt.println(\"{n}\")
    }
  }
  n.replicas = 5
  done := make(chan int, 1)
  task worker(move m, done)
  _ = <-done
  fmt.println(\"done\")
}
",
    );
    let go = emit(&src);
    assert_ordered(
        &go,
        "m[\"a\"] = n",
        "go worker(m, done)",
        "select-recv binder",
    );
}

/// Fence the other direction: the loan must still be materialized at all. If a
/// future change simply DROPPED the store-back, the ordering assertions above
/// would still pass on a modified program but this one would fail — the write
/// through `n` would be lost.
#[test]
fn the_loan_write_is_still_carried_back_to_the_map() {
    let mut src = String::from(PRELUDE);
    src.push_str(
        "\
fn main() {
  m := map[string]service{\"a\": service{replicas: 1}}
  n := &m[\"a\"]
  n.replicas = 5
  fmt.println(\"{m[\"a\"].replicas}\")
}
",
    );
    let go = emit(&src);
    assert!(
        go.contains("m[\"a\"] = n"),
        "the loan's store-back must still be emitted (dropping it would silently \
         discard the write). Emitted Go:\n{go}"
    );
}
