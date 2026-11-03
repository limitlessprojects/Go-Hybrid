#!/usr/bin/env python3
"""Public deterministic parse-only workloads; no packages or model calls."""
from pathlib import Path
import sys
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
cases = {
    "plain-short": [f's_{i} := "row-{i:04d}-catalogue-description"' for i in range(2000)],
    "plain-long": [f's_{i} := "' + "x" * 4096 + '"' for i in range(128)],
    "plain-unicode-escapes": [f's_{i} := "世界-{i}\\n{{{{}}}}-\\x41-\\u00e9"' for i in range(1000)],
    "interpolation-control": [f's_{i} := "row-{{n}}-tail-{i}"' for i in range(2000)],
    "numeric-control": [f'n_{i} := {i} + 1 // no strings' for i in range(2000)],
}
for name, lines in cases.items():
    (out / (name + ".hy")).write_text("fn sample(n int) {\n" + "\n".join(lines) + "\n}\n")
