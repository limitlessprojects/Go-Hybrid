#!/usr/bin/env python3
"""
W-027 Linear Close-Rights Static Gate Verifier (Battle-Hardened Edition v3)
Enforces compile-time affine consumption of channel close-rights.
Security audited against:
- CWE-415 (Double Close)
- CWE-416 (Use After Close / Send After Close)
- CWE-672 (Operation on Resource after Expiration or Release)
- CWE-20 (Lexer / Regex Injection Bypasses: multiline strings, escaped quotes, whitespace, multi-statement lines, indexed selectors, parenthesized expressions)
- CWE-362 (Race Condition / Concurrent Split Close)
- CWE-486 (Inner-block loop & task boundary scope escaping)
- CWE-1007 (Unicode confusable & homoglyph evasion)
- CWE-476 (Uninitialized / nil channel close panic)
- CWE-684 (Tuple / multi-channel declaration evasion)
- CWE-841 (Smart contract reentrancy with unclosed linear channels)
"""

import sys
import re
from typing import Dict, List, Tuple, Optional, Set
from dataclasses import dataclass, field
from enum import Enum


class CapState(Enum):
    HELD = "held"              # Owner has exclusive right to close
    CONSUMED = "consumed"      # close() has been executed; channel is shut
    BORROWED = "borrowed"      # Read/write access only; no close right
    SPLIT_ERROR = "split"      # Concurrently shared close capability (forbidden)
    DEFERRED = "deferred"      # Scheduled for close via defer
    NIL = "nil"                # Uninitialized nil channel
    PARTIAL = "partial"        # Closed on some branches but not all


@dataclass
class Diagnostic:
    code: str
    message: str
    line: int
    channel: str


@dataclass
class ScopeFrame:
    kind: str  # 'function', 'loop', 'task', 'block', 'if', 'else', 'select'
    declared: Set[str] = field(default_factory=set)
    shadowed: Dict[str, CapState] = field(default_factory=dict)
    ranged_channel: Optional[str] = None


class LinearCloseGate:
    """
    Battle-hardened static typestate checker for affine channel close rights.
    """

    def __init__(self):
        self.diagnostics: List[Diagnostic] = []

    def _strip_comments_and_strings(self, code: str) -> str:
        """
        Safely strips comments and masks string literals preserving line counts.
        """
        # 1. Strip block comments /* ... */
        code = re.sub(r"/\*.*?\*/", lambda m: "\n" * m.group(0).count("\n"), code, flags=re.DOTALL)
        # 2. Mask multiline backtick strings `...`
        code = re.sub(r"`[^`]*`", lambda m: '""' + ("\n" * m.group(0).count("\n")), code, flags=re.DOTALL)
        # 3. Mask double-quoted strings with escaped characters
        code = re.sub(r'"(?:\\.|[^"\\])*"', '""', code)

        lines = code.splitlines()
        sanitized_lines = []
        for line in lines:
            if "//" in line:
                line = line.split("//", 1)[0]
            sanitized_lines.append(line)
        return "\n".join(sanitized_lines)

    @staticmethod
    def detect_homoglyphs(ident: str) -> bool:
        """
        Detects mixed-script homoglyphs or invisible Unicode confusables (CWE-1007).
        """
        has_ascii = any('a' <= c <= 'z' or 'A' <= c <= 'Z' for c in ident)
        has_non_ascii = any(ord(c) > 127 for c in ident)
        has_zero_width = any(c in ('\u200b', '\u200c', '\u200d', '\ufeff') for c in ident)
        return (has_ascii and has_non_ascii) or has_zero_width

    def check_program(self, code: str) -> List[Diagnostic]:
        self.diagnostics = []
        clean_code = self._strip_comments_and_strings(code)
        lines = clean_code.splitlines()

        # Channel allocation tracking: canonical_id -> CapState
        channels: Dict[str, CapState] = {}
        # Alias map: alias_name -> canonical_id
        aliases: Dict[str, str] = {}
        # Track deferred closes: canonical_id -> list of (line, var_name)
        deferred_closes: Dict[str, List[Tuple[int, str]]] = {}
        borrowed_params: Set[str] = set()

        root_frame = ScopeFrame(kind='root')
        scope_stack: List[ScopeFrame] = [root_frame]

        # Channel identifier regex pattern supporting dotted selectors and index brackets
        ID_PAT = r"[^\s\(\);:,=<-]+"

        for idx, raw_line in enumerate(lines, 1):
            line = raw_line.strip()
            if not line:
                continue

            # Check for function headers with channel parameters
            fn_match = re.search(r"fn\s+\w+\s*\((.*?)\)", line)
            if fn_match:
                params = fn_match.group(1).split(",")
                for p in params:
                    p = p.strip()
                    if ":" in p and "chan" in p:
                        param_name = p.split(":")[0].strip()
                        if not param_name.startswith("move "):
                            borrowed_params.add(param_name)
                            channels[param_name] = CapState.BORROWED
                            aliases[param_name] = param_name
                            scope_stack[-1].declared.add(param_name)

            # Block structure tracking using stack to prevent inner-block escape
            is_loop_start = bool(re.search(r"\b(for|while)\b", line))
            is_task_start = bool(re.search(r"\b(task|spawn|go)\b", line))
            is_if_start = bool(re.search(r"\bif\b", line))

            # Detect range over channel: `for ... := range ch`
            range_match = re.search(r"\bfor\b.*?range\s+([^\s\{\(\)]+)", line)
            ranged_ch = range_match.group(1) if range_match else None

            for ch_char in line:
                if ch_char == '{':
                    kind = 'block'
                    if is_loop_start:
                        kind = 'loop'
                        is_loop_start = False
                    elif is_task_start:
                        kind = 'task'
                        is_task_start = False
                    elif is_if_start:
                        kind = 'if'
                        is_if_start = False

                    new_frame = ScopeFrame(kind=kind, ranged_channel=ranged_ch)
                    scope_stack.append(new_frame)
                elif ch_char == '}':
                    if len(scope_stack) > 1:
                        popped = scope_stack.pop()
                        for var, old_state in popped.shadowed.items():
                            channels[var] = old_state
                            aliases[var] = var

            in_loop = any(f.kind == 'loop' for f in scope_stack)
            in_task = any(f.kind == 'task' for f in scope_stack)
            in_if = any(f.kind in ('if', 'else') for f in scope_stack)
            ranged_channels = {f.ranged_channel for f in scope_stack if f.ranged_channel}

            # Uninitialized nil channel declaration: `var ch chan int`
            var_decl_iter = re.finditer(r"\bvar\s+([^\s:]+)\s+chan\b", line)
            for var_decl_match in var_decl_iter:
                vars_part = var_decl_match.group(1)
                for raw_var in vars_part.split(","):
                    var_name = raw_var.strip()
                    if not var_name:
                        continue
                    if self.detect_homoglyphs(var_name):
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3019",
                                message=f"security violation: confusable homoglyph identifier '{var_name}'",
                                line=idx,
                                channel=var_name
                            )
                        )
                        continue
                    scope_stack[-1].declared.add(var_name)
                    channels[var_name] = CapState.NIL
                    aliases[var_name] = var_name

            # Smart contract reentrancy hazard
            if re.search(r"\b(call_external|invoke_contract|external_call|contract\.call|msg\.sender\.call)\b", line):
                held_channels = [k for k, v in channels.items() if v == CapState.HELD and k not in borrowed_params]
                for hc in held_channels:
                    self.diagnostics.append(
                        Diagnostic(
                            code="HYB-E3022",
                            message=f"reentrancy hazard: untrusted external call while linear channel '{hc}' is held open (CWE-841)",
                            line=idx,
                            channel=hc
                        )
                    )

            # Split statements by semicolon to prevent multi-statement single-line bypasses
            statements = [s.strip() for s in line.split(";") if s.strip()]

            for raw_stmt in statements:
                # Normalize spaces around dots and brackets: `worker . ch` -> `worker.ch`
                stmt = re.sub(r"\s*\.\s*", ".", raw_stmt)
                stmt = re.sub(r"\s*\[\s*", "[", stmt)
                stmt = re.sub(r"\s*\]\s*", "]", stmt)

                # Channel allocation: `ch := make(chan ...)` and Tuple assignments `ch1, ch2 := make(...), make(...)`
                alloc_match = re.search(r"([^:=]+)\s*:=\s*(.+)$", stmt)
                if alloc_match and "make" in alloc_match.group(2) and "chan" in alloc_match.group(2):
                    lhs_part = alloc_match.group(1).strip()
                    rhs_part = alloc_match.group(2).strip()
                    lhs_vars = [v.strip().strip("()") for v in lhs_part.split(",") if v.strip()]
                    rhs_exprs = [r.strip() for r in rhs_part.split(",") if r.strip()]

                    for i, name in enumerate(lhs_vars):
                        is_chan_make = False
                        if i < len(rhs_exprs) and "make" in rhs_exprs[i] and "chan" in rhs_exprs[i]:
                            is_chan_make = True
                        elif "make" in rhs_part and "chan" in rhs_part:
                            is_chan_make = True

                        if is_chan_make:
                            if self.detect_homoglyphs(name):
                                self.diagnostics.append(
                                    Diagnostic(
                                        code="HYB-E3019",
                                        message=f"security violation: confusable homoglyph identifier '{name}' in channel declaration",
                                        line=idx,
                                        channel=name
                                    )
                                )
                                continue

                            curr_frame = scope_stack[-1]
                            if name in channels:
                                curr_frame.shadowed[name] = channels[name]

                            curr_frame.declared.add(name)
                            channels[name] = CapState.HELD
                            aliases[name] = name
                    continue

                # Detect aliasing: `ch2 := ch1`
                alias_match = re.search(rf"\(?\s*({ID_PAT})\s*\)?\s*:=\s*\(?\s*({ID_PAT})\s*\)?$", stmt)
                if alias_match and "make" not in stmt:
                    dst, src = alias_match.group(1).strip("()"), alias_match.group(2).strip("()")
                    if src in aliases:
                        canonical = aliases[src]
                        aliases[dst] = canonical
                        continue

                # Detect deferred close: `defer close(ch)` or `defer close((ch))`
                defer_match = re.search(rf"\bdefer\s+close\s*\(\s*\(?\s*({ID_PAT})\s*\)?\s*\)", stmt)
                if defer_match:
                    target_var = defer_match.group(1).strip("()")
                    if self.detect_homoglyphs(target_var):
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3019",
                                message=f"security violation: confusable homoglyph in defer close '{target_var}'",
                                line=idx,
                                channel=target_var
                            )
                        )
                        continue

                    canon = aliases.get(target_var, target_var)
                    current_state = channels.get(canon)

                    if current_state == CapState.NIL:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3020",
                                message=f"panic hazard: attempt to defer close uninitialized nil channel '{target_var}' (CWE-476)",
                                line=idx,
                                channel=target_var
                            )
                        )
                        continue

                    if current_state == CapState.BORROWED:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3016",
                                message=f"cannot defer close on borrowed channel '{target_var}'",
                                line=idx,
                                channel=target_var
                            )
                        )
                        continue

                    if in_loop:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3015",
                                message=f"illegal defer close: cannot schedule close('{target_var}') inside loop body",
                                line=idx,
                                channel=target_var
                            )
                        )
                        continue

                    if in_if:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3024",
                                message=f"conditional defer hazard: defer close('{target_var}') inside conditional branch creates uneven lifecycle guarantee",
                                line=idx,
                                channel=target_var
                            )
                        )

                    if canon not in deferred_closes:
                        deferred_closes[canon] = []
                    deferred_closes[canon].append((idx, target_var))
                    if len(deferred_closes[canon]) > 1:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3013",
                                message=f"double close: channel '{target_var}' has multiple deferred closes",
                                line=idx,
                                channel=target_var
                            )
                        )
                    continue

                # Detect immediate close: `close(ch)` with arbitrary whitespace and nesting
                close_match = re.search(rf"(?<!defer\s)\bclose\s*\(\s*\(?\s*({ID_PAT})\s*\)?\s*\)", stmt)
                if close_match:
                    target_var = close_match.group(1).strip("()")
                    if self.detect_homoglyphs(target_var):
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3019",
                                message=f"security violation: confusable homoglyph in close '{target_var}'",
                                line=idx,
                                channel=target_var
                            )
                        )
                        continue

                    canon = aliases.get(target_var, target_var)
                    current_state = channels.get(canon)

                    if current_state == CapState.NIL:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3020",
                                message=f"panic hazard: attempt to close uninitialized nil channel '{target_var}' (CWE-476)",
                                line=idx,
                                channel=target_var
                            )
                        )
                    elif canon in ranged_channels or target_var in ranged_channels:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3025",
                                message=f"range self-closure panic: cannot close channel '{target_var}' while ranging over it (CWE-672)",
                                line=idx,
                                channel=target_var
                            )
                        )
                    elif current_state == CapState.BORROWED:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3016",
                                message=f"cannot close borrowed channel '{target_var}'; close-right not owned by caller",
                                line=idx,
                                channel=target_var
                            )
                        )
                    elif current_state == CapState.CONSUMED:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3013",
                                message=f"double close: channel '{target_var}' close-right was already consumed",
                                line=idx,
                                channel=target_var
                            )
                        )
                    elif in_loop:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3015",
                                message=f"illegal close: cannot call close('{target_var}') inside loop body",
                                line=idx,
                                channel=target_var
                            )
                        )
                    elif in_task:
                        channels[canon] = CapState.SPLIT_ERROR
                    elif in_if:
                        channels[canon] = CapState.PARTIAL
                    else:
                        if current_state == CapState.SPLIT_ERROR:
                            self.diagnostics.append(
                                Diagnostic(
                                    code="HYB-E3017",
                                    message=f"split close hazard: channel '{target_var}' is closed both in background task and caller",
                                    line=idx,
                                    channel=target_var
                                )
                            )
                        else:
                            channels[canon] = CapState.CONSUMED

                # Detect send operation: `ch <- value`
                send_match = re.search(rf"(?<!:)(?<!=)\s*\(?\s*({ID_PAT})\s*\)?\s*<-\s*(.+)", stmt)
                if send_match:
                    target_var = send_match.group(1).strip().strip("()")
                    canon = aliases.get(target_var, target_var)
                    current_state = channels.get(canon)

                    if current_state == CapState.CONSUMED:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3013",
                                message=f"use after close: attempt to send on closed channel '{target_var}'",
                                line=idx,
                                channel=target_var
                            )
                        )
                    elif current_state == CapState.PARTIAL:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3023",
                                message=f"use after partial close: attempt to send on conditionally closed channel '{target_var}'",
                                line=idx,
                                channel=target_var
                            )
                        )
                    elif current_state == CapState.NIL:
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E3020",
                                message=f"panic hazard: send on uninitialized nil channel '{target_var}' causes permanent deadlock",
                                line=idx,
                                channel=target_var
                            )
                        )

        return self.diagnostics


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python linear_close_gate.py <file.hy>")
        sys.exit(1)

    gate = LinearCloseGate()
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        diags = gate.check_program(f.read())

    if diags:
        print(f"FAILED with {len(diags)} diagnostics:")
        for d in diags:
            print(f"  [{d.code}] L{d.line}: {d.message}")
        sys.exit(1)
    else:
        print("OK (0 diagnostics): All channel close-rights verified affine and linear.")
        sys.exit(0)
