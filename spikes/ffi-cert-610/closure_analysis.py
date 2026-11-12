#!/usr/bin/env python3
"""Why the fail-closed policy escalates: where the call closure leaves the
artifact-provable set.  Run from the repository root after `hy_ffi_cert.py build`:

    python3 spikes/ffi-cert-610/closure_analysis.py > spikes/ffi-cert-610/evidence/closure-analysis.txt
"""

from __future__ import annotations

import collections
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hy_ffi_cert as h  # noqa: E402


def main() -> int:
    root = h.repo_root()
    artifact = h.default_artifact()
    tmp = Path(__import__("tempfile").mkdtemp(prefix="hyffi-an-"))
    graph, frames, owner = {}, {}, {}
    for name, body in h.archive_members(artifact):
        if body[:4] != b"\x7fELF":
            continue
        p = tmp / (name.replace("/", "_") or f"m{len(owner)}.o")
        p.write_bytes(body)
        try:
            elf = h.Elf(body)
        except ValueError:
            continue
        frames.update(h.stack_records(elf))
        for fn, node in h.call_graph(p).items():
            owner.setdefault(fn, name)
            if fn in graph:
                graph[fn]["calls"] += node["calls"]
                graph[fn]["tail"] += node["tail"]
                graph[fn]["got"] += node["got"]
                graph[fn]["indirect"] |= node["indirect"]
            else:
                graph[fn] = node

    print(f"functions in the object-code call graph : {len(graph)}")
    print(f"functions with a .stack_sizes record    : {len(frames)}")
    missing = [fn for fn in graph if fn not in frames]
    by_member = collections.Counter(owner[fn].split(".")[0] for fn in missing)
    print("\nfunctions WITHOUT a stack record, by archive member:")
    for name, count in by_member.most_common(10):
        print(f"  {count:5d}  {name}")

    def find_cycle(start):
        stack, onstack = [], set()

        def dfs(fn):
            if fn in onstack:
                return stack[stack.index(fn):] + [fn]
            onstack.add(fn)
            stack.append(fn)
            node = graph.get(fn)
            if node:
                for c in node["calls"] + node["tail"] + node["got"]:
                    if c in graph:
                        cyc = dfs(c)
                        if cyc:
                            return cyc
            onstack.discard(fn)
            stack.pop()
            return None

        return dfs(start)

    print("\nshortest cycle reachable from hy_json_new_object (proof that the "
          "alloc-error path is recursive in the artifact):")
    for step in find_cycle("hy_json_new_object") or []:
        print("  " + step[:120])

    print("\nper-export forcing facts:")
    for sym in sorted({s for s in graph if s.startswith(h.EXPORT_PREFIXES)}):
        facts = h.closure_facts(graph, frames, sym, mode="linked")
        print(f"  {sym:30s} bytes={facts['bytes']:6d} recursion={facts['recursive']!s:5s} "
              f"indirect={facts['indirect']!s:5s} missing={len(set(facts['missing'])):3d} "
              f"external={len(set(facts['external'])):3d}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
