// EXPECTED-VERDICT: ACCEPT
// Nursery task with complete cancellation teardown and channel drain join
fn worker(ctx context, in_ch rx[int], out_ch tx[int], w CancelWitness) TeardownResult {
    select {
    case v := <-in_ch:
        out_ch <- (v * 2)
        return w.complete()
    case <-ctx.done():
        for _ in in_ch.drain() {}
        return w.abort()
    }
}
