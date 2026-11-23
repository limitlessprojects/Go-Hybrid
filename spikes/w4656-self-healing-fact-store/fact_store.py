#!/usr/bin/env python3
"""
spikes/w4656-self-healing-fact-store/fact_store.py

Reference implementation of Milestone 1 for Wildcard Issue #4656:
"[wildcard] Carry Tier-0 decisions across edits by rechecking the facts they used"

Maintains fine-grained semantic fact dependencies per function so checker
verdicts survive source edits when recorded semantic premises remain valid.
"""

import hashlib
import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional, Set, Tuple


class Verdict(Enum):
    ACCEPT = "ACCEPT"
    REJECT = "REJECT"


@dataclass
class Diagnostic:
    code: str
    message: str
    anchor: str  # Structural AST anchor (e.g., "fn:main/stmt:1/call:foo")


@dataclass
class FunctionVerdict:
    verdict: Verdict
    diagnostics: List[Diagnostic] = field(default_factory=list)


@dataclass
class SemanticFacts:
    func_name: str
    normalized_ir_hash: str
    # Direct calls: callee_name -> (resolved_target, callee_summary_hash, return_provenance)
    callee_facts: Dict[str, Tuple[str, str, str]] = field(default_factory=dict)
    # Binding resolutions: name -> resolved_kind (e.g. "param", "local", "pkg_fn")
    binding_facts: Dict[str, str] = field(default_factory=dict)
    # Missing names that were unresolved during checking
    missing_bindings: Set[str] = field(default_factory=set)
    # Stored verdict
    verdict: Optional[FunctionVerdict] = None


class SourceSnapshot:
    """Represents a snapshot of a module's AST and symbol tables."""

    def __init__(self, functions: Dict[str, str], global_symbols: Dict[str, str], callee_summaries: Dict[str, str]):
        self.functions = functions  # func_name -> source / normalized body
        self.global_symbols = global_symbols  # symbol -> kind
        self.callee_summaries = callee_summaries  # func_name -> summary (provenance, etc.)

    def get_ir_hash(self, func_name: str) -> str:
        body = self.functions.get(func_name, "")
        # Normalize: strip comments and whitespace
        normalized = "\n".join(
            line.split("//")[0].strip()
            for line in body.splitlines()
            if line.split("//")[0].strip()
        )
        return hashlib.sha256(normalized.encode("utf-8")).hexdigest()

    def resolve_binding(self, name: str) -> Optional[str]:
        return self.global_symbols.get(name)

    def get_callee_summary(self, callee_name: str) -> Optional[Tuple[str, str, str]]:
        if callee_name in self.callee_summaries:
            summary = self.callee_summaries[callee_name]
            summary_hash = hashlib.sha256(summary.encode("utf-8")).hexdigest()
            return (callee_name, summary_hash, summary)
        return None


class SelfHealingFactStore:
    """
    Tier-0 incremental decision store. Rechecks recorded semantic facts
    before deciding whether to reuse a cached function verdict.
    """

    def __init__(self):
        self.cache: Dict[str, SemanticFacts] = {}
        self.full_check_count: int = 0
        self.reused_verdict_count: int = 0

    def check_function_clean(self, func_name: str, snapshot: SourceSnapshot) -> FunctionVerdict:
        """Runs the clean Tier-0 checker on a function and records its semantic facts."""
        self.full_check_count += 1
        body = snapshot.functions.get(func_name, "")
        ir_hash = snapshot.get_ir_hash(func_name)

        facts = SemanticFacts(func_name=func_name, normalized_ir_hash=ir_hash)

        diagnostics: List[Diagnostic] = []
        is_clean = True

        # Analyze identifiers and calls
        for line in body.splitlines():
            line = line.split("//")[0].strip()
            # Simulate unresolved reference check
            if "unresolved_" in line:
                sym = line.split("unresolved_")[1].split()[0].strip("();,")
                if snapshot.resolve_binding(sym) is None:
                    facts.missing_bindings.add(sym)
                    diagnostics.append(
                        Diagnostic(
                            code="HYB-E1001",
                            message=f"unresolved symbol `{sym}`",
                            anchor=f"fn:{func_name}/sym:{sym}",
                        )
                    )
                    is_clean = False
                else:
                    facts.binding_facts[sym] = snapshot.resolve_binding(sym)

            # Simulate callee provenance check
            if "call " in line:
                callee = line.split("call ")[1].split()[0].strip("();,")
                summary = snapshot.get_callee_summary(callee)
                if summary:
                    target, s_hash, prov = summary
                    facts.callee_facts[callee] = (target, s_hash, prov)
                    if prov == "escaping_tainted":
                        diagnostics.append(
                            Diagnostic(
                                code="HYB-E3001",
                                message=f"callee `{callee}` return provenance escapes",
                                anchor=f"fn:{func_name}/call:{callee}",
                            )
                        )
                        is_clean = False
                else:
                    facts.missing_bindings.add(callee)
                    diagnostics.append(
                        Diagnostic(
                            code="HYB-E1001",
                            message=f"unresolved callee `{callee}`",
                            anchor=f"fn:{func_name}/call:{callee}",
                        )
                    )
                    is_clean = False

        v = FunctionVerdict(
            verdict=Verdict.ACCEPT if is_clean else Verdict.REJECT,
            diagnostics=diagnostics,
        )
        facts.verdict = v
        self.cache[func_name] = facts
        return v

    def evaluate_function(self, func_name: str, snapshot: SourceSnapshot) -> Tuple[FunctionVerdict, bool]:
        """
        Evaluates a function against snapshot.
        Returns: (FunctionVerdict, reused: bool)
        """
        if func_name not in self.cache:
            return self.check_function_clean(func_name, snapshot), False

        cached = self.cache[func_name]

        # 1. Check normalized IR hash (in-body changes)
        current_ir_hash = snapshot.get_ir_hash(func_name)
        if current_ir_hash != cached.normalized_ir_hash:
            return self.check_function_clean(func_name, snapshot), False

        # 2. Check callee semantic facts (callee summaries and return provenance)
        for callee, (t, s_hash, _) in cached.callee_facts.items():
            current_summary = snapshot.get_callee_summary(callee)
            if not current_summary:
                return self.check_function_clean(func_name, snapshot), False
            _, cur_hash, _ = current_summary
            if cur_hash != s_hash:
                return self.check_function_clean(func_name, snapshot), False

        # 3. Check recorded binding facts
        for sym, expected_kind in cached.binding_facts.items():
            if snapshot.resolve_binding(sym) != expected_kind:
                return self.check_function_clean(func_name, snapshot), False

        # 4. Check previously missing bindings (if any now resolve, verdict cannot be reused)
        for sym in cached.missing_bindings:
            if snapshot.resolve_binding(sym) is not None:
                return self.check_function_clean(func_name, snapshot), False

        # All recorded premises hold -> REUSE VERDICT!
        self.reused_verdict_count += 1
        return cached.verdict, True
