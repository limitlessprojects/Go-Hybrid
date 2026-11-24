#!/usr/bin/env python3
"""
benchmarks.py — Multi-Scale Scaling Benchmarks
Compares AST Path-Sensitive Alias-Graph Walk vs Zero-Alias Phantom Capability Type Checking.
"""

import json
from phantom_capability_model import benchmark_graph_vs_phantom

def run_suite():
    scales = [100, 500, 1000, 2500, 5000]
    results = []
    print("=" * 70)
    print("RUNNING MULTI-SCALE SCALING BENCHMARKS")
    print(f"{'Scale (Nodes)':<15} | {'Graph Walk (s)':<18} | {'Phantom Cap (s)':<18} | {'Speedup':<10}")
    print("-" * 70)
    for n in scales:
        res = benchmark_graph_vs_phantom(num_nodes=n)
        results.append(res)
        print(f"{res['nodes']:<15} | {res['graph_walk_seconds']:<18.5f} | {res['phantom_cap_seconds']:<18.5f} | {res['speedup_factor']}x")
    print("=" * 70)

    with open("benchmark_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print("Saved benchmark results to benchmark_results.json")

if __name__ == "__main__":
    run_suite()
