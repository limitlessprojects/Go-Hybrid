import time
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ssa_lowering import SSALoweringEngine, HyAssign, HyBinary, HyIdent, HyLitInt, HyReturn


def benchmark_direct_ssa(iterations: int = 10000):
    engine = SSALoweringEngine()
    stmts = [
        HyAssign("x", HyLitInt(10), is_owned=True),
        HyAssign("y", HyLitInt(20), is_owned=True),
        HyAssign("z", HyBinary("+", HyIdent("x"), HyIdent("y")), is_owned=True),
        HyReturn(HyIdent("z")),
    ]

    start = time.perf_counter()
    for _ in range(iterations):
        _ = engine.lower_function("bench_fn", stmts)
    elapsed = time.perf_counter() - start
    avg_ns = (elapsed / iterations) * 1e9
    return avg_ns, engine.stack_allocations, engine.heap_allocations


def benchmark_simulated_text_transpilation(iterations: int = 10000):
    start = time.perf_counter()
    for _ in range(iterations):
        lines = [
            "package main",
            "func bench_fn() int64 {",
            "\tx := int64(10)",
            "\ty := int64(20)",
            "\tz := x + y",
            "\t_ = x",
            "\t_ = y",
            "\treturn z",
            "}"
        ]
        text = "\n".join(lines)
        _ = text.encode("utf-8")
    elapsed = time.perf_counter() - start
    avg_ns = (elapsed / iterations) * 1e9
    return avg_ns


if __name__ == "__main__":
    n = 20000
    ssa_ns, stack_allocs, heap_allocs = benchmark_direct_ssa(n)
    text_ns = benchmark_simulated_text_transpilation(n)

    print(f"Direct SSA Lowering:           {ssa_ns:.2f} ns / fn")
    print(f"Text Transpilation:            {text_ns:.2f} ns / fn")
    print(f"Stack Allocated Places:        {stack_allocs}")
    print(f"Heap Allocated Escapes:        {heap_allocs}")
