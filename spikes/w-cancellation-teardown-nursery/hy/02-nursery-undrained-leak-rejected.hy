// EXPECTED-VERDICT: REJECT HYB-E3910
// Task selects on ctx.done() but abandons rx channel without drain join
fn leaky_worker(ctx context, in_ch rx[int], w CancelWitness) TeardownResult {
    select {
    case <-ctx.done():
        // REJECT HYB-E3910: in_ch has unconsumed teardown drain obligation
        return w.abort()
    }
}
