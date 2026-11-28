// EXPECTED-VERDICT: REJECT HYB-E3911
// Launching bare unmanaged goroutine escapes nursery lifecycle bounds
fn rogue_spawner(ctx context, w CancelWitness) TeardownResult {
    go fn() {
        // REJECT HYB-E3911: unmanaged goroutine escape inside checked nursery scope
        time.sleep(1000)
    }()
    return w.complete()
}
