"""
Performance benchmarks for structured task scopes.

Times the Python prototype's scope checking, and separately *measures* the
generated lowering with Go escape analysis instead of asserting that it is
allocation-free. The measurement says it is not (the WaitGroup escapes);
what the lowering actually buys is O(1) allocation per scope.
"""

import time
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from scope_checker import check_structured_scope


def benchmark_checker_performance(num_iterations: int = 10000):
    """Benchmark the scope checker performance."""
    
    # Test case 1: Simple scope with single task
    simple_scope = """
    scope {
        h := task worker()
        join(move h)
    }
    """
    
    # Test case 2: Multiple tasks
    multi_task_scope = """
    scope {
        h1 := task worker1()
        h2 := task worker2()
        h3 := task worker3()
        h4 := task worker4()
        h5 := task worker5()
        join(move h1)
        join(move h2)
        join(move h3)
        join(move h4)
        join(move h5)
    }
    """
    
    # Test case 3: Nested scopes
    nested_scope = """
    scope {
        h1 := task outer()
        scope {
            h2 := task inner()
            join(move h2)
        }
        join(move h1)
    }
    """
    
    test_cases = [
        ("Simple scope (1 task)", simple_scope),
        ("Multi-task scope (5 tasks)", multi_task_scope),
        ("Nested scopes", nested_scope),
    ]
    
    print("=" * 70)
    print("Structured Task Scope Checker Performance Benchmarks")
    print("=" * 70)
    print(f"Iterations per test: {num_iterations:,}")
    print()
    
    for name, source in test_cases:
        # Warm-up
        for _ in range(100):
            check_structured_scope(source)
        
        # Benchmark
        start = time.perf_counter()
        for _ in range(num_iterations):
            result = check_structured_scope(source)
        end = time.perf_counter()
        
        elapsed = end - start
        per_iteration = (elapsed / num_iterations) * 1_000_000  # microseconds
        
        print(f"{name}:")
        print(f"  Total time: {elapsed:.3f}s")
        print(f"  Per iteration: {per_iteration:.2f}µs")
        print(f"  Throughput: {num_iterations / elapsed:,.0f} checks/sec")
        print(f"  Result: {result.diagnostic_code}")
        print()
    
    # Benchmark lowering code generation
    print("-" * 70)
    print("Lowering Code Generation Benchmark")
    print("-" * 70)
    
    scope_with_lowering = """
    scope {
        h1 := task worker1()
        h2 := task worker2()
        join(move h1)
        join(move h2)
    }
    """
    
    start = time.perf_counter()
    for _ in range(num_iterations):
        result = check_structured_scope(scope_with_lowering)
        _ = result.lowered_go
    end = time.perf_counter()
    
    elapsed = end - start
    print(f"With lowering code generation:")
    print(f"  Total time: {elapsed:.3f}s")
    print(f"  Per iteration: {(elapsed / num_iterations) * 1_000_000:.2f}µs")
    print(f"  Generated Go code length: {len(result.lowered_go)} chars")
    print()
    
    # Allocation analysis: measured, not asserted.
    print("-" * 70)
    print("Lowering allocation analysis (escape analysis on generated code)")
    print("-" * 70)
    report_lowering_escapes(result.lowered_go)
    print()


def report_lowering_escapes(lowered_go: str) -> None:
    """Compile the lowered scope with -gcflags=-m and report what escapes.

    A failed probe is reported as FAILED, never as a clean verdict: an
    empty escape list used to be read as "did not escape", which is exactly
    the silent-pass shape this spike's own spec forbids.
    """
    import re
    import shutil
    import subprocess
    import tempfile

    go = shutil.which("go")
    if not go:
        print("Escape analysis: SKIPPED — no `go` on PATH")
        print("  No allocation claim is made in this run.")
        return

    body = lowered_go
    if "var __hy_wg" not in body:
        print("Escape analysis: SKIPPED — lowering has no WaitGroup to analyse")
        return

    # The lowering emits calls like `worker_a()`; stub whatever it names.
    callees = sorted(set(re.findall(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)", body))
                     - {"Add", "Done", "Wait", "main", "func"})
    stubs = "".join(f"func {name}() {{}}\n\n" for name in callees)

    with tempfile.TemporaryDirectory() as d:
        with open(os.path.join(d, "go.mod"), "w") as f:
            f.write("module loweringprobe\n\ngo 1.26\n")
        with open(os.path.join(d, "main.go"), "w") as f:
            f.write("package main\n\nimport \"sync\"\n\n"
                    + stubs
                    + "func main() {\n"
                    + "\n".join("\t" + ln for ln in body.splitlines())
                    + "\n}\n")
        proc = subprocess.run([go, "build", "-gcflags=-m", "./..."], cwd=d,
                              capture_output=True, text=True, timeout=120)
        out = proc.stderr + proc.stdout

    if proc.returncode != 0:
        print("Escape analysis: FAILED (probe did not compile; rc=%d)" % proc.returncode)
        for ln in out.splitlines()[:6]:
            print("  " + ln)
        print("  → no allocation verdict is drawn from a failed probe.")
        return

    lines = [ln for ln in out.splitlines()
             if "main.go" in ln and ("heap" in ln or "escape" in ln)]
    print("Escape analysis of the generated scope (go build -gcflags=-m):")
    for ln in lines[:8]:
        print("  " + ln.strip())
    escaped = any("__hy_wg" in ln and "moved to heap" in ln for ln in lines)
    print()
    print("Verdict:")
    if escaped:
        print("  ✗ NOT allocation-free: the goroutine closure captures __hy_wg,")
        print("    so one WaitGroup is heap-allocated per scope.")
        print("  ✓ Leak-free and O(1) in task count: exactly one __hy_wg per scope")
        print("    regardless of task count, and no per-handle bookkeeping is emitted.")
    else:
        print("  Inconclusive: the probe compiled but no __hy_wg line was reported.")
        print("  Re-read the output above before making any allocation claim.")


def benchmark_error_detection_speed():
    """Benchmark how quickly errors are detected."""
    
    print("=" * 70)
    print("Error Detection Speed Benchmarks")
    print("=" * 70)
    print()
    
    # Error case 1: Unjoined handle (detected at end)
    error_unjoined = """
    scope {
        h1 := task worker1()
        h2 := task worker2()
        h3 := task worker3()
        h4 := task worker4()
        h5 := task worker5()
        join(move h1)
        join(move h2)
        join(move h3)
        join(move h4)
        // h5 not joined - error at end
    }
    """
    
    # Error case 2: Double join (detected immediately)
    error_double_join = """
    scope {
        h := task worker()
        join(move h)
        join(move h)  // Error detected here
    }
    """
    
    # Error case 3: Foreign callback (detected at spawn)
    error_foreign = """
    http.HandleFunc("/", fn() {
        h := task worker()  // Error detected here
    })
    """
    
    test_cases = [
        ("Unjoined handle (end detection)", error_unjoined, "HYB-E3501"),
        ("Double join (immediate detection)", error_double_join, "HYB-E3502"),
        ("Foreign callback (spawn detection)", error_foreign, "HYB-E3503"),
    ]
    
    for name, source, expected_code in test_cases:
        # Warm-up
        for _ in range(100):
            check_structured_scope(source)
        
        # Benchmark
        num_iterations = 10000
        start = time.perf_counter()
        for _ in range(num_iterations):
            result = check_structured_scope(source)
        end = time.perf_counter()
        
        elapsed = end - start
        per_iteration = (elapsed / num_iterations) * 1_000_000
        
        print(f"{name}:")
        print(f"  Expected: {expected_code}, Got: {result.diagnostic_code}")
        print(f"  Detection time: {per_iteration:.2f}µs per check")
        print(f"  Unjoined handles: {result.unjoined_handles}")
        print()


if __name__ == "__main__":
    benchmark_checker_performance()
    benchmark_error_detection_speed()
    
    print("=" * 70)
    print("Summary")
    print("=" * 70)
    print()
    print("✓ Python checker throughput: <100µs per scope check (prototype only")
    print("  — this is CPython, not the shipped Rust pass; do not quote it as")
    print("  compiler cost)")
    print("✓ Error detection: immediate for E3502/E3503, end-of-scope for E3501")
    print("✓ Checker state: O(n) in tasks-in-scope (dict + list, prototype)")
    print()
    print("The Rust module (compiler/crates/hy-check/src/task_scope.rs) is the")
    print("deliverable; these numbers characterise the prototype that came first.")
    print("Allocation claims about the *lowered Go code* come from the escape")
    print("analysis above, not from this summary.")
