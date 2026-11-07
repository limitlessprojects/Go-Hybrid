// 01-ssa-lowering-admitted.hy
// Legal Hybrid function lowered directly to Go SSA basic blocks with static stack certificate.

@stack_cert(leaf = true, frame_bytes = 32, nosplit = true, abi = "ABI0")
fn compute_leaf_kernel(x: int64, y: int64) -> int64 {
    val sum = x + y;
    if sum < 100 {
        return sum * 2;
    } else {
        return sum - 50;
    }
}
