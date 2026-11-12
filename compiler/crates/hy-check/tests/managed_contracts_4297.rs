//! Conformance & falsifier suite for verifiable `managed` synchronization contracts (#4297).
//!
//! Covers M1 empirical corpus findings (`02` and `18`), M2 `mutex` and `atomic`
//! contract verification, field-level access-set bounds, interprocedural `task`
//! propagation, alias/boundary escape prevention, post-block outliving-task
//! enforcement, and `--require-managed-contracts` perimeter shrinkage.

use hy_check::{check_file, check_file_with_options, CheckOptions, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> CheckResult {
    let ast = parse_file("contract_test.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn check_src_strict(src: &str) -> CheckResult {
    let ast = parse_file("contract_test.hy", src).expect("snippet must parse");
    check_file_with_options(&ast, &CheckOptions { require_managed_contracts: true })
}

fn assert_accept_strict(src: &str) -> CheckResult {
    let res = check_src_strict(src);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got: {:?}", res.diagnostics);
    res
}

fn assert_reject_e0005(src: &str) -> CheckResult {
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected Reject, got Accept");
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E0005"), "expected HYB-E0005, got: {:?}", res.diagnostics);
    res
}

#[test]
fn m1_corpus_18_contracted_verifies() {
    let res = assert_accept_strict(r#"
import "sync"
import "fmt"
const n_workers = 4
const seed_urls = ["https://example.com/a", "https://example.com/b"]
struct pool_stats {
    mu        sync.mutex
    processed int
    failures  int
}
fn fetch(url string) bool { return len(url) % 3 != 0 }
fn worker(id int, jobs <-chan string, stats *pool_stats, done chan<- bool) {
    _ = id
    for url in jobs {
        ok := fetch(url)
        stats.mu.lock()
        stats.processed++
        if !ok { stats.failures++ }
        stats.mu.unlock()
    }
    done <- true
}
fn main() {
    managed("pool stats under the pool mutex (lock stays)", "mutex", "CORPUS-18", guard: stats.mu, protects: stats) {
        jobs := make(chan string, len(seed_urls))
        stats := pool_stats{}
        done := make(chan bool, n_workers)
        for i in 0..n_workers { task worker(i, jobs, &stats, done) }
        for _, url in seed_urls { jobs <- url }
        close(jobs)
        for _ in 0..n_workers { <-done }
        stats.mu.lock()
        p, f := stats.processed, stats.failures
        stats.mu.unlock()
        fmt.println("processed={p} failures={f}")
    }
}
"#);
    assert_eq!((res.managed.managed_blocks, res.managed.verified_blocks, res.managed.unverified_blocks), (1, 1, 0));
    let c = res.managed.blocks[0].contract.as_ref().expect("contract audit recorded");
    assert!(c.verified);
    assert_eq!(c.guard.as_deref(), Some("stats.mu"));
    assert_eq!(c.protects, vec!["stats".to_string()]);
}

#[test]
fn m1_corpus_02_uncontracted_rejected_under_require_managed_contracts() {
    let corpus02 = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../../../tests/checker-corpus/02-shared-map-managed.hy"
    )).expect("read corpus 02");
    let ast = parse_file("02-shared-map-managed.hy", &corpus02).expect("parse corpus 02");
    let legacy = check_file(&ast);
    assert_eq!(legacy.verdict, Verdict::Accept);
    assert_eq!((legacy.managed.verified_blocks, legacy.managed.unverified_blocks), (0, 1));
    let strict = check_file_with_options(&ast, &CheckOptions { require_managed_contracts: true });
    assert_eq!(strict.verdict, Verdict::Reject);
    assert!(strict.diagnostics.iter().any(|d| d.code == "HYB-E0005"));
}

#[test]
fn m1_corpus_02_fake_mutex_not_passed_to_worker_rejects() {
    assert_reject_e0005(r#"
import "sync"
fn tally(counts map[string]int, done chan<- bool) {
    counts["hybrid"]++
    done <- true
}
fn main() {
    managed("fake mutex never passed to worker", "mutex", guard: mu, protects: counts) {
        var mu sync.mutex
        counts := map[string]int{}
        done := make(chan bool, 1)
        task tally(counts, done)
        <-done
        mu.lock()
        _ = counts["hybrid"]
        mu.unlock()
    }
}
"#);
}

#[test]
fn m1_corpus_02_synchronised_with_real_mutex_verifies() {
    let res = assert_accept_strict(r#"
import "sync"
import "fmt"
fn tally(shard []string, counts map[string]int, mu *sync.mutex, done chan<- bool) {
    for w in shard {
        mu.lock()
        counts[w]++
        mu.unlock()
    }
    done <- true
}
fn main() {
    managed("shards tallied under runner mutex", "mutex", "CORPUS-02-SYNC", guard: mu, protects: counts) {
        var mu sync.mutex
        counts := map[string]int{}
        done := make(chan bool, 2)
        for _ in 0..2 {
            chunk := []string{"hybrid", " fox"}
            task tally(move chunk, counts, &mu, done)
        }
        for _ in 0..2 { <-done }
        mu.lock()
        total := len(counts)
        n := counts["hybrid"]
        mu.unlock()
        fmt.println("{total} {n}")
    }
}
"#);
    assert_eq!(res.managed.verified_blocks, 1);
}

#[test]
fn m2_mutex_reading_field_not_covered_by_contract_rejects() {
    let res = assert_reject_e0005(r#"
import "sync"
struct pool_stats {
    mu        sync.mutex
    guarded   int
    unguarded int
}
fn worker(stats *pool_stats, done chan<- bool) {
    stats.mu.lock()
    stats.guarded++
    stats.mu.unlock()
    done <- true
}
fn main() {
    managed("only guarded is covered", "mutex", guard: stats.mu, protects: stats.guarded) {
        stats := pool_stats{}
        done := make(chan bool, 1)
        task worker(&stats, done)
        <-done
        stats.mu.lock()
        leak := stats.unguarded
        stats.mu.unlock()
        _ = leak
    }
}
"#);
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E0005" && d.message.contains("stats.unguarded")));
}

#[test]
fn m2_mutex_unlocked_read_in_main_rejects() {
    assert_reject_e0005(r#"
import "sync"
struct pool_stats {
    mu sync.mutex
    processed int
}
fn worker(stats *pool_stats, done chan<- bool) {
    stats.mu.lock()
    stats.processed++
    stats.mu.unlock()
    done <- true
}
fn main() {
    managed("unlocked read in main", "mutex", guard: stats.mu, protects: stats) {
        stats := pool_stats{}
        done := make(chan bool, 1)
        task worker(&stats, done)
        <-done
        p := stats.processed
        _ = p
    }
}
"#);
}

#[test]
fn m2_mutex_unlocked_write_in_spawned_worker_rejects() {
    assert_reject_e0005(r#"
import "sync"
struct pool_stats {
    mu sync.mutex
    processed int
}
fn worker(stats *pool_stats, done chan<- bool) {
    stats.mu.lock()
    stats.mu.unlock()
    stats.processed++
    done <- true
}
fn main() {
    managed("worker writes after unlock", "mutex", guard: stats.mu, protects: stats) {
        stats := pool_stats{}
        done := make(chan bool, 1)
        task worker(&stats, done)
        <-done
    }
}
"#);
}

#[test]
fn m2_mutex_wrong_lock_held_rejects() {
    assert_reject_e0005(r#"
import "sync"
fn worker(counts map[string]int, mu_a *sync.mutex, done chan<- bool) {
    mu_a.lock()
    counts["ok"] = 1
    mu_a.unlock()
    done <- true
}
fn main() {
    managed("locks wrong mutex", "mutex", guard: mu_a, protects: counts) {
        var mu_a sync.mutex
        var mu_b sync.mutex
        counts := map[string]int{}
        done := make(chan bool, 1)
        task worker(counts, &mu_a, done)
        <-done
        mu_b.lock()
        v := counts["ok"]
        mu_b.unlock()
        _ = v
    }
}
"#);
}

#[test]
fn m2_mutex_passed_by_value_to_task_rejects() {
    assert_reject_e0005(r#"
import "sync"
fn worker(counts map[string]int, mu sync.mutex, done chan<- bool) {
    mu.lock()
    counts["k"] = 1
    mu.unlock()
    done <- true
}
fn main() {
    managed("mutex copied by value", "mutex", guard: mu, protects: counts) {
        var mu sync.mutex
        counts := map[string]int{}
        done := make(chan bool, 1)
        task worker(counts, mu, done)
        <-done
    }
}
"#);
}

#[test]
fn m2_rw_mutex_rlock_permits_read_and_rejects_write() {
    assert_accept_strict(r#"
import "sync"
fn writer(counts map[string]int, rw *sync.rw_mutex, done chan<- bool) {
    rw.lock()
    counts["k"] = 42
    rw.unlock()
    done <- true
}
fn main() {
    managed("rw_mutex read lock", "mutex", guard: rw, protects: counts) {
        var rw sync.rw_mutex
        counts := map[string]int{}
        done := make(chan bool, 1)
        task writer(counts, &rw, done)
        <-done
        rw.rlock()
        v := counts["k"]
        rw.runlock()
        _ = v
    }
}
"#);
    assert_reject_e0005(r#"
import "sync"
fn main() {
    managed("rw_mutex write under rlock", "mutex", guard: rw, protects: counts) {
        var rw sync.rw_mutex
        counts := map[string]int{}
        rw.rlock()
        counts["k"] = 99
        rw.runlock()
    }
}
"#);
}

#[test]
fn m2_mutex_unbalanced_lock_across_branches_or_exit_rejects() {
    assert_reject_e0005(r#"
import "sync"
fn main() {
    managed("missing unlock on one branch", "mutex", guard: mu, protects: counts) {
        var mu sync.mutex
        counts := map[string]int{}
        flag := true
        if flag {
            mu.lock()
            counts["a"] = 1
        }
    }
}
"#);
}

#[test]
fn m2_mutex_defer_unlock_in_worker_verifies_and_defer_in_loop_rejects() {
    assert_accept_strict(r#"
import "sync"
fn worker(counts map[string]int, mu *sync.mutex, done chan<- bool) {
    mu.lock()
    defer mu.unlock()
    counts["k"]++
    done <- true
}
fn main() {
    managed("defer unlock in worker", "mutex", guard: mu, protects: counts) {
        var mu sync.mutex
        counts := map[string]int{}
        done := make(chan bool, 1)
        task worker(counts, &mu, done)
        <-done
    }
}
"#);
    assert_reject_e0005(r#"
import "sync"
fn main() {
    managed("defer unlock in loop", "mutex", guard: mu, protects: counts) {
        var mu sync.mutex
        counts := map[string]int{}
        for i in 0..3 {
            mu.lock()
            defer mu.unlock()
            counts["k"] = i
        }
    }
}
"#);
}

#[test]
fn m2_atomic_pkg_ops_verify() {
    let res = assert_accept_strict(r#"
import "sync/atomic"
import "fmt"
fn bump(counter *int64, done chan<- bool) {
    atomic.add_int64(counter, 1)
    done <- true
}
fn main() {
    managed("int64 updated atomically", "atomic", protects: counter) {
        var counter int64 = 0
        done := make(chan bool, 2)
        for _ in 0..2 { task bump(&counter, done) }
        for _ in 0..2 { <-done }
        v := atomic.load_int64(&counter)
        fmt.println("{v}")
    }
}
"#);
    assert_eq!(res.managed.verified_blocks, 1);
}

#[test]
fn m2_atomic_pointer_method_ops_verify() {
    let res = assert_accept_strict(r#"
import "sync/atomic"
fn reader(ptr *atomic.pointer[int], done chan<- bool) {
    _ = ptr.load()
    done <- true
}
fn main() {
    managed("atomic.pointer mediated via load/store", "atomic", protects: ptr) {
        var ptr atomic.pointer[int]
        v := 7
        ptr.store(&v)
        done := make(chan bool, 1)
        task reader(&ptr, done)
        <-done
    }
}
"#);
    assert_eq!(res.managed.verified_blocks, 1);
}

#[test]
fn m2_atomic_raw_read_or_write_rejects() {
    assert_reject_e0005(r#"
import "sync/atomic"
fn bad_worker(counter *int64, done chan<- bool) {
    atomic.add_int64(counter, 1)
    done <- true
}
fn main() {
    managed("raw increment mixed with atomic", "atomic", protects: counter) {
        var counter int64 = 0
        done := make(chan bool, 1)
        task bad_worker(&counter, done)
        <-done
        counter++
    }
}
"#);
    assert_reject_e0005(r#"
import "sync/atomic"
import "fmt"
fn worker(counter *int64, done chan<- bool) {
    atomic.add_int64(counter, 1)
    done <- true
}
fn main() {
    managed("raw non-atomic read", "atomic", protects: counter) {
        var counter int64 = 0
        done := make(chan bool, 1)
        task worker(&counter, done)
        <-done
        v := counter
        fmt.println("{v}")
    }
}
"#);
}

#[test]
fn m2_uncontracted_second_shared_root_inside_contracted_block_rejects() {
    assert_reject_e0005(r#"
import "sync"
fn sneaky(unprotected map[string]int, done chan<- bool) {
    unprotected["race"] = 1
    done <- true
}
fn main() {
    managed("dummy contract hiding unprotected map", "mutex", guard: mu, protects: safe_map) {
        var mu sync.mutex
        safe_map := map[string]int{}
        unprotected := map[string]int{}
        mu.lock()
        safe_map["ok"] = 1
        mu.unlock()
        done := make(chan bool, 1)
        task sneaky(unprotected, done)
        <-done
    }
}
"#);
}

#[test]
fn m2_alias_laundering_of_protected_root_rejects() {
    assert_reject_e0005(r#"
import "sync"
fn main() {
    managed("alias escapes lock", "mutex", guard: mu, protects: counts) {
        var mu sync.mutex
        counts := map[string]int{}
        mu.lock()
        alias := counts
        mu.unlock()
        alias["race"] = 1
    }
}
"#);
}

#[test]
fn m2_outliving_task_unlocked_post_block_access_rejects() {
    assert_reject_e0005(r#"
import "sync"
fn worker(counts map[string]int, mu *sync.mutex, done chan<- bool) {
    mu.lock()
    counts["k"]++
    mu.unlock()
    done <- true
}
fn main() {
    var mu sync.mutex
    counts := map[string]int{}
    done := make(chan bool, 1)
    managed("worker may outlive block", "mutex", guard: mu, protects: counts) {
        task worker(counts, &mu, done)
    }
    counts["post_block_race"] = 1
    <-done
}
"#);
}

#[test]
fn m2_interop_and_finalizer_perimeter_category_rules() {
    let ok = assert_accept_strict(r#"
fn main() {
    managed("cgo pointer handoff", "interop") {
        x := 1
        _ = x
    }
    managed("runtime finalizer registration", "finalizer") {
        y := 2
        _ = y
    }
}
"#);
    assert_eq!((ok.managed.verified_blocks, ok.managed.unverified_blocks), (0, 2));
    assert_reject_e0005(r#"
fn main() {
    x := 1
    managed("bogus interop contract", "interop", protects: x) {
        _ = x
    }
}
"#);
}

#[test]
fn m2_hy_diag_explain_e0005_exists() {
    let page = hy_diag::Registry::global().explain("HYB-E0005").expect("HYB-E0005 reference page must exist");
    assert!(page.contains("HYB-E0005") && page.contains("contract"));
}
