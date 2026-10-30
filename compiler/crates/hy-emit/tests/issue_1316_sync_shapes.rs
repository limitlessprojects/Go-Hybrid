//! Regression tests for #1316: `sync.rw_mutex`, `sync.wait_group`, and
//! `sync.once` lower to `sync.RWMutex`, `sync.WaitGroup`, and `sync.Once`
//! with exact Go method names (`RLock`, `RUnlock`, `TryRLock`, `Add`, `Done`,
//! `Wait`, `Do`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

#[test]
fn sync_rw_mutex_wait_group_and_once_lower_types_and_methods_correctly() {
    let go = emit(
        r#"import "sync"

struct pool_coord {
    rw sync.rw_mutex
    wg sync.wait_group
    once sync.once
}

fn main() {
    p := pool_coord{}
    p.rw.rlock()
    p.rw.runlock()
    p.rw.r_lock()
    p.rw.r_unlock()
    p.rw.try_rlock()
    p.wg.add(1)
    p.wg.done()
    p.wg.wait()
}
"#,
    );
    assert!(
        go.contains("sync.RWMutex"),
        "expected sync.rw_mutex to lower to sync.RWMutex, got:\n{go}"
    );
    assert!(
        go.contains("sync.WaitGroup"),
        "expected sync.wait_group to lower to sync.WaitGroup, got:\n{go}"
    );
    assert!(
        go.contains("sync.Once"),
        "expected sync.once to lower to sync.Once, got:\n{go}"
    );
    assert!(
        go.contains("p.rw.RLock()") && go.contains("p.rw.RUnlock()") && go.contains("p.rw.TryRLock()"),
        "expected rw_mutex methods to lower to RLock/RUnlock/TryRLock, got:\n{go}"
    );
    assert!(
        go.contains("p.wg.Add(1)") && go.contains("p.wg.Done()") && go.contains("p.wg.Wait()"),
        "expected wait_group methods to lower to Add/Done/Wait, got:\n{go}"
    );
}
