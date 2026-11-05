// 01-linear-branch-complete-admitted.hy
// EXPECTED-VERDICT: ACCEPT (all control-flow paths consume linear handle `enc`)

@[linear_must_consume]
fn compress_branch_complete(flag bool, out chan []byte) (int, error) {
    enc := flate2.new_gzip(6)
    if flag {
        bytes := enc.finish()?
        out <- move bytes
        return 1, nil
    } else {
        bytes := enc.finish()?
        out <- move bytes
        return 2, nil
    }
}
