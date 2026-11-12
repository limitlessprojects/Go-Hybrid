// 02-linear-early-try-leak-rejected.hy
// EXPECTED-VERDICT: REJECT HYB-E3701 (unconsumed linear obligation on early `?` return and else-branch)

@[linear_must_consume]
fn compress_leaked_on_early_return(flag bool) (int, error) {
    enc := flate2.new_gzip(6)
    n := fallible_step()?
    if flag {
        _ := enc.finish()?
    }
    return n, nil
}
