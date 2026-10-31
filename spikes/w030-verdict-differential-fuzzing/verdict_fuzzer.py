#!/usr/bin/env python3
"""
W-030 Verdict-Differential Fuzzing Engine.
Turns the static Go-Hybrid checker corpus into an adversarial test case generator.
Employs semantics-preserving mutations, parse-clean filtering, differential verdict oracles,
and DDmin delta-debugging to discover and minimize candidate P0 soundness bugs.
"""

import sys
import os
import re
import random
from typing import List, Tuple, Optional, Dict
from dataclasses import dataclass


@dataclass
class SeedProgram:
    source: str
    expected_verdict: str  # e.g., "ACCEPT", "REJECT:HYB-E3003"
    code_file: str = "seed.hy"


class ParseCleanFilter:
    """
    Lightweight, high-throughput pre-filter ensuring mutated programs parse cleanly
    before invoking the full compiler backend.
    """
    @staticmethod
    def is_parse_clean(code: str) -> bool:
        # 1. Balanced braces, brackets, and parentheses
        stack = []
        pairs = {')': '(', '}': '{', ']': '['}
        in_string = False
        in_comment = False

        for char in code:
            if char == '"':
                in_string = not in_string
                continue
            if in_string:
                continue

            if char in "({[":
                stack.append(char)
            elif char in ")}]":
                if not stack or stack[-1] != pairs[char]:
                    return False
                stack.pop()

        if stack or in_string:
            return False

        # 2. Must contain valid function entry point
        if not re.search(r"\bfn\s+\w+\s*\(", code):
            return False

        return True


class VerdictMutator:
    """
    Applies equivalence-preserving and boundary-probing mutations to .hy seeds.
    """
    def __init__(self, seed: int = 42):
        self.rng = random.Random(seed)

    def mutate_m1_alpha_rename(self, code: str) -> str:
        """M1: Alpha-rename local variable identifiers."""
        vars_found = set(re.findall(r"\b([a-zA-Z_]\w*)\s*:=", code))
        # Exclude keywords and common builtins
        exclude = {"fn", "task", "spawn", "make", "close", "println", "int", "string", "bool"}
        candidates = [v for v in vars_found if v not in exclude]

        if not candidates:
            return code

        target = self.rng.choice(candidates)
        suffix = f"_{self.rng.randint(100, 999)}"
        new_name = f"{target}{suffix}"
        # Word boundary substitution
        return re.sub(rf"\b{re.escape(target)}\b", new_name, code)

    def mutate_m2_identity_injection(self, code: str) -> str:
        """M2: Inject no-op statements (_ = 0) and redundant grouping."""
        lines = code.splitlines()
        candidates = [i for i, l in enumerate(lines) if "{" in l and not l.strip().startswith("//")]
        if not candidates:
            return code

        idx = self.rng.choice(candidates)
        indent = "    "
        lines.insert(idx + 1, f"{indent}_ = 0")
        return "\n".join(lines)

    def mutate_m3_neutral_block(self, code: str) -> str:
        """M3: Wrap safe statement inside a neutral lexical block { ... }."""
        lines = code.splitlines()
        candidates = [i for i, l in enumerate(lines) if ":=" in l and not l.strip().startswith("//")]
        if not candidates:
            return code

        idx = self.rng.choice(candidates)
        stmt = lines[idx]
        lines[idx] = f"    {{\n        {stmt.strip()}\n    }}"
        return "\n".join(lines)

    def apply_equivalence_mutation(self, code: str) -> str:
        """Applies a random equivalence-preserving mutation."""
        mutators = [
            self.mutate_m1_alpha_rename,
            self.mutate_m2_identity_injection,
            self.mutate_m3_neutral_block
        ]
        chosen = self.rng.choice(mutators)
        mutant = chosen(code)
        return mutant if ParseCleanFilter.is_parse_clean(mutant) else code


class DeltaDebugger:
    """
    DDmin Delta-Debugging Algorithm.
    Minimizes any candidate verdict flip to the smallest reproducing subset (< 40 lines).
    """
    @staticmethod
    def minimize(code: str, oracle_fn) -> str:
        lines = code.splitlines()
        n = 2

        while len(lines) >= 2:
            chunk_size = max(1, len(lines) // n)
            subsets = []
            for i in range(0, len(lines), chunk_size):
                subsets.append(lines[i:i + chunk_size])

            reduced = False
            for subset in subsets:
                # Candidate: lines minus this subset
                candidate_lines = [l for l in lines if l not in subset]
                candidate_code = "\n".join(candidate_lines)

                # Check if candidate is parse clean and still reproduces the flip
                if ParseCleanFilter.is_parse_clean(candidate_code) and oracle_fn(candidate_code):
                    lines = candidate_lines
                    n = max(n - 1, 2)
                    reduced = True
                    break

            if not reduced:
                if n >= len(lines):
                    break
                n = min(n * 2, len(lines))

        return "\n".join(lines)


class VerdictDifferentialFuzzer:
    """
    Main orchestration engine.
    """
    def __init__(self, seed: int = 1337):
        self.seed = seed
        self.mutator = VerdictMutator(seed)

    def parse_seed_verdict(self, source: str) -> str:
        """Extracts // EXPECTED-VERDICT: <VERDICT> from source header."""
        match = re.search(r"//\s*EXPECTED-VERDICT:\s*([\w\-:]+)", source)
        if match:
            return match.group(1).upper()
        # Fallback heuristic
        if "reject" in source.lower() or "error" in source.lower():
            return "REJECT"
        return "ACCEPT"

    def fuzz_cycle(self, seed: SeedProgram, iterations: int = 100, checker_oracle=None) -> List[Tuple[str, str, str]]:
        """
        Runs N iterations of verdict-differential fuzzing on a seed program.
        Returns list of (minimized_mutant, expected_verdict, actual_verdict) for hits.
        """
        hits = []
        for i in range(iterations):
            mutant = self.mutator.apply_equivalence_mutation(seed.source)
            if not ParseCleanFilter.is_parse_clean(mutant):
                continue

            if checker_oracle:
                actual_verdict = checker_oracle(mutant)
                if actual_verdict != seed.expected_verdict:
                    # Verdict flip detected! Minimize with DDmin
                    minimized = DeltaDebugger.minimize(
                        mutant,
                        lambda c: checker_oracle(c) != seed.expected_verdict
                    )
                    hits.append((minimized, seed.expected_verdict, actual_verdict))
                    break

        return hits


if __name__ == "__main__":
    print("Verdict-Differential Fuzzer W-030 Initialized.")
