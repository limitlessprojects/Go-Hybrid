// 03-e3401-non-commutative-reducer-rejected.hy (#2385)
// EXPECTED: REJECT HYB-E3401 (combine is order-dependent slice append: a ++ b != b ++ a)
struct OrderedLog { entries []string }
fn combine_log(acc OrderedLog, shard OrderedLog) OrderedLog {
    return OrderedLog{entries: append(acc.entries, shard.entries...)}
}
