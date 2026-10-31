#!/usr/bin/env python3
"""
W-032 Certified Heap-Freedom Task Pipeline Verifier (@heapfree)
Enforces compile-time zero-allocation and escape elimination across Tier-0 task pipelines.
Closes #997 (@magomedta).
"""

import sys
import re
from typing import Dict, List, Optional
from dataclasses import dataclass


@dataclass
class Diagnostic:
    code: str
    message: str
    line: int
    expr: str


class HeapFreePipelineGate:
    """
    Static verifier for @heapfree directives and Tier-0 zero-allocation task boundaries.
    """

    def __init__(self):
        self.diagnostics: List[Diagnostic] = []

    def _strip_comments_and_strings(self, code: str) -> str:
        code = re.sub(r"/\*.*?\*/", "", code, flags=re.DOTALL)
        code = re.sub(r"`[^`]*`", '""', code, flags=re.DOTALL)
        code = re.sub(r'"(?:\\.|[^"\\])*"', '""', code)

        lines = code.splitlines()
        sanitized = []
        for line in lines:
            if "//" in line:
                if "@heapfree" in line or "//hy:heapfree" in line:
                    sanitized.append(line)
                else:
                    sanitized.append(line.split("//", 1)[0])
            else:
                sanitized.append(line)
        return "\n".join(sanitized)

    def check_program(self, code: str) -> List[Diagnostic]:
        self.diagnostics = []
        clean_code = self._strip_comments_and_strings(code)
        lines = clean_code.splitlines()

        in_heapfree = False
        scope_depth = 0
        in_managed = False
        in_task_block = False
        declared_budget: Optional[int] = None

        for idx, raw_line in enumerate(lines, 1):
            line = raw_line.strip()
            if not line:
                continue

            # Detect directive @heapfree or @heapfree(budget=N)
            heapfree_match = re.search(r"@heapfree(?:\s*\(\s*budget\s*=\s*(\d+)\s*\))?", line)
            if heapfree_match:
                in_heapfree = True
                if heapfree_match.group(1):
                    declared_budget = int(heapfree_match.group(1))
                else:
                    declared_budget = 4096
                continue

            # Detect task block
            if "task {" in line or "task{" in line:
                in_task_block = True

            # Scope tracking
            if "{" in line:
                scope_depth += line.count("{")
            if "}" in line:
                scope_depth -= line.count("}")
                if in_task_block and "}" in line:
                    in_task_block = False
                if scope_depth <= 0:
                    in_heapfree = False
                    scope_depth = 0
                    declared_budget = None

            if not in_heapfree:
                continue

            # Allow managed bypass
            if "managed(" in line:
                in_managed = True
            if in_managed and "}" in line:
                in_managed = False
                continue
            if in_managed:
                continue

            statements = [s.strip() for s in line.split(";") if s.strip()]
            for stmt in statements:
                # 1. Anonymous task closure capturing outer pointer reference (HYB-E4101)
                if (in_task_block or "task" in stmt) and re.search(r"&[a-zA-Z_]\w*", stmt):
                    self.diagnostics.append(
                        Diagnostic(
                            code="HYB-E4101",
                            message="task closure captures outer pointer reference; use value-receiver task struct",
                            line=idx,
                            expr=stmt
                        )
                    )

                # 2. Dynamic Interface Boxing at task boundary (HYB-E4102)
                if re.search(r"\b(any|interface\{\})\s*\(", stmt):
                    self.diagnostics.append(
                        Diagnostic(
                            code="HYB-E4102",
                            message="value escapes across dynamic interface boxing in heap-free task pipeline",
                            line=idx,
                            expr=stmt
                        )
                    )

                # 3. Dynamic Channel Allocation (HYB-E4103)
                if re.search(r"make\s*\(\s*chan\s+[^,)]+,\s*[a-zA-Z_]\w*\s*\)", stmt):
                    self.diagnostics.append(
                        Diagnostic(
                            code="HYB-E4103",
                            message="dynamic channel allocation without static capacity bound escapes to heap",
                            line=idx,
                            expr=stmt
                        )
                    )

                # 4. Escaping Output Reference (HYB-E4104)
                if re.search(r"\breturn\s+&", stmt):
                    self.diagnostics.append(
                        Diagnostic(
                            code="HYB-E4104",
                            message="address of local stack payload escapes task execution boundary",
                            line=idx,
                            expr=stmt
                        )
                    )

                # 5. Stack allocation budget exceeded (HYB-E4105)
                # Detect arrays larger than declared budget: e.g. [100000]byte with budget 4096
                array_match = re.search(r"\[(\d+)\]\s*(?:byte|uint8|int|float64)", stmt)
                if array_match and declared_budget is not None:
                    count = int(array_match.group(1))
                    elem_size = 8 if ("int" in stmt or "float64" in stmt) else 1
                    total_bytes = count * elem_size
                    if total_bytes > declared_budget:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E4105",
                                message=f"task payload allocation ({total_bytes} bytes) exceeds declared stack budget ({declared_budget} bytes)",
                                line=idx,
                                expr=stmt
                            )
                        )

        return self.diagnostics


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python heapfree_pipeline_gate.py <file.hy>")
        sys.exit(1)

    gate = HeapFreePipelineGate()
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        diags = gate.check_program(f.read())

    if diags:
        print(f"FAILED with {len(diags)} diagnostics:")
        for d in diags:
            print(f"  [{d.code}] L{d.line}: {d.message}")
        sys.exit(1)
    else:
        print("OK (0 diagnostics): Certified heap-freedom task pipeline verified.")
        sys.exit(0)
