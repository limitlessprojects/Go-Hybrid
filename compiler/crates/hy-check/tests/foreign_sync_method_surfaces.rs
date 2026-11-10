//! DRAFT (FOM2/Fo, 2026-11-10 04:35 IST — NOT SUBMITTED; needs QC green on a
//! C-toolchain env or fork CI before any PR). Modeled on
//! tests/foreign_io_method_surfaces.rs: guarantees that calling phantom /
//! unsupported methods on the curated `sync` foreign surfaces rejects
//! fail-closed with HYB-E1002 (Row 13 BUILDABLE).
//!
//! Surfaces under test (declared in hy-check/src/foreign.rs, foreign_method_surface):
//! sync.mutex, sync.rwmutex, sync.wait_group, sync.once, sync.cond, sync.pool, sync.map.
//! The sync surfaces had NO dedicated surface-test file before this draft
//! (grep of tests/foreign_*surface*.rs on 2026-11-10).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

fn assert_phantom_rejected(src: &str, recv: &str) {
    let bad = check(src);
    assert_eq!(bad.verdict, Verdict::Reject, "recv {recv}");
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {recv}, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(diag.message.contains(recv), "{}", diag.message);
}

#[test]
fn phantom_mutex_methods_are_rejected() {
    assert_phantom_rejected(
        "import \"sync\"\n\
         fn lock_it(m *sync.mutex) {\n\
             m.totally_fake()\n\
         }\n",
        "sync.mutex",
    );
}

#[test]
fn phantom_rwmutex_methods_are_rejected() {
    assert_phantom_rejected(
        "import \"sync\"\n\
         fn lock_it(rw *sync.rwmutex) {\n\
             rw.bogus_lock_op()\n\
         }\n",
        "sync.rwmutex",
    );
}

#[test]
fn phantom_wait_group_methods_are_rejected() {
    assert_phantom_rejected(
        "import \"sync\"\n\
         fn wait_it(wg *sync.wait_group) {\n\
             wg.unknown_wait_call()\n\
         }\n",
        "sync.wait_group",
    );
}

#[test]
fn phantom_once_methods_are_rejected() {
    assert_phantom_rejected(
        "import \"sync\"\n\
         fn run_once(o *sync.once) {\n\
             o.do_something_else()\n\
         }\n",
        "sync.once",
    );
}

#[test]
fn phantom_cond_methods_are_rejected() {
    assert_phantom_rejected(
        "import \"sync\"\n\
         fn wait_it(c *sync.cond) {\n\
             c.bogus_signal_op()\n\
         }\n",
        "sync.cond",
    );
}

#[test]
fn phantom_pool_methods_are_rejected() {
    assert_phantom_rejected(
        "import \"sync\"\n\
         fn get_it(p *sync.pool) {\n\
             p.fetch_unknown()\n\
         }\n",
        "sync.pool",
    );
}

#[test]
fn phantom_map_methods_are_rejected() {
    assert_phantom_rejected(
        "import \"sync\"\n\
         fn load_it(m *sync.map) {\n\
             m.unknown_lookup()\n\
         }\n",
        "sync.map",
    );
}

#[test]
fn every_declared_mutex_method_is_accepted() {
    // Positive control (added 2026-11-10 06:44 IST): each declared sync.mutex
    // method name must NOT emit HYB-E1002 (over-restriction guard).
    for m in ["lock", "unlock", "try_lock"] {
        let ok = check(&format!(
            "import \"sync\"\nfn lock_it(m *sync.mutex) {{\n    m.{m}()\n}}\n"
        ));
        assert!(
            !codes(&ok).iter().any(|c| c == "HYB-E1002"),
            "declared method {m} must not be flagged HYB-E1002; got {:?}",
            codes(&ok)
        );
    }
}

#[test]
fn every_declared_map_method_is_accepted() {
    for m in [
        "load", "store", "delete", "load_or_store", "load_and_delete", "range",
        "compare_and_swap", "compare_and_delete", "swap",
    ] {
        let ok = check(&format!(
            "import \"sync\"\nfn load_it(m *sync.map) {{\n    m.{m}()\n}}\n"
        ));
        assert!(
            !codes(&ok).iter().any(|c| c == "HYB-E1002"),
            "declared method {m} must not be flagged HYB-E1002; got {:?}",
            codes(&ok)
        );
    }
}
