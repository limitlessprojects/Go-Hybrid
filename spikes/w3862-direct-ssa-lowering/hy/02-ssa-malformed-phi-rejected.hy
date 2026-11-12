// 02-ssa-malformed-phi-rejected.hy
// Malformed SSA representation: phi node in join block does not match predecessor count.
// Expected diagnostic: HYB-E3862-SSA-MALFORMED

fn malformed_phi_kernel(cond: bool, a: int64, b: int64) -> int64 {
    // Basic blocks: b_entry -> (b_then, b_else) -> b_join
    // b_join has 2 predecessors, but lower SSA constructs phi with only 1 argument.
    @ssa_malformed_phi(arity = 1, expected_preds = 2)
    val res = if cond { a } else { b };
    return res;
}
