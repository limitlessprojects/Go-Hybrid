import time
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ssa_lowering import (
    SSALoweringEngine, HyAssign, HyBinary, HyIdent, HyLitInt, HyReturn,
    HyStructAlloc, HyLitStr, GCStackMap
)


def benchmark_direct_ssa(iterations: int = 20000):
    engine = SSALoweringEngine(write_barriers_active=True)
    stmts = [
        HyAssign("x", HyLitInt(10), is_owned=True),
        HyAssign("y", HyLitInt(20), is_owned=True),
        HyAssign("z", HyBinary("+", HyIdent("x"), HyIdent("y")), is_owned=True),
        HyStructAlloc("state", [("id", HyLitInt(1), False), ("tok", HyLitStr("ptr"), True)], is_owned=True),
        HyReturn(HyIdent("z")),
    ]

    start = time.perf_counter()
    for _ in range(iterations):
        fn = engine.lower_function("bench_fn", stmts)
    elapsed = time.perf_counter() - start
    avg_ns = (elapsed / iterations) * 1e9

    mask, bit_str = GCStackMap.generate_bitmap(fn.frame_slots)
    return avg_ns, engine.stack_allocations, engine.heap_allocations, engine.write_barriers_count, bit_str


def benchmark_text_baseline(iterations: int = 20000, with_pragmas: bool = False):
    start = time.perf_counter()
    for _ in range(iterations):
        lines = [
            "package main",
            "//go:noescape" if with_pragmas else "",
            "func bench_fn() (int64, *State) {",
            "\tx := int64(10)",
            "\ty := int64(20)",
            "\tz := x + y",
            "\tstate := &State{id: 1, tok: \"ptr\"}",
            "\t_ = x",
            "\t_ = y",
            "\treturn z, state",
            "}"
        ]
        text = "\n".join(l for l in lines if l)
        _ = text.encode("utf-8")
    elapsed = time.perf_counter() - start
    avg_ns = (elapsed / iterations) * 1e9
    return avg_ns


if __name__ == "__main__":
    n = 20000
    ssa_ns, stack_allocs, heap_allocs, wb_count, bit_str = benchmark_direct_ssa(n)
    text_std_ns = benchmark_text_baseline(n, with_pragmas=False)
    text_pragma_ns = benchmark_text_baseline(n, with_pragmas=True)

    print(f"Direct SSA Lowering:                 {ssa_ns:.2f} ns / fn")
    print(f"Standard Text Transpilation:         {text_std_ns:.2f} ns / fn")
    print(f"Pragma-Annotated Text Transpilation: {text_pragma_ns:.2f} ns / fn")
    print(f"Stack Allocated Places:              {stack_allocs}")
    print(f"Heap Allocated Escapes:              {heap_allocs}")
    print(f"Active Write Barriers Inserted:      {wb_count}")
    print(f"GC Stack Frame Bitmap:               0b{bit_str}")
