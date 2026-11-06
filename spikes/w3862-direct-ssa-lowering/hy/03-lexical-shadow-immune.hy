// 03-lexical-shadow-immune.hy
// Demonstrates lexical shadow immunity in direct SSA lowering.
// Plain Go emission fails if 'fmt' or 'os' identifiers are locally defined.
// Direct SSA assigns distinct virtual registers (v3, v4) avoiding textual identifier collisions.

fn shadow_immune_kernel() -> int64 {
    val fmt: int64 = 42;
    val os: bool = true;
    val res: int64 = fmt + 10;
    return res;
}
