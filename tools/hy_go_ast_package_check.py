#!/usr/bin/env python3
"""W-011 (#186) Spike: Whole-Package Tier-0 Checking via Native Go AST & Export Data.

Erases the .hy / .go file split by analyzing multi-file Go packages (`//hy:check`,
`//hy:managed("<reason>", "<category>")`, `hy.Move(&x)`, `hy.Task(func() { ... })`)
together with sibling `.go` struct declarations and `go/types` export-data summaries.
"""

from dataclasses import dataclass, field
import re
from typing import Dict, List, Optional, Set

VALID_DIRECTIVES = {"check", "managed"}
MUTABLE_FIELD_PATTERNS = ("map[", "[]", "*", "chan ", "hy.Own[", "hy.Share[")
VALUE_PRIMITIVES = {"int", "int64", "uint64", "string", "bool", "float64", "byte", "rune"}


@dataclass
class TypeSummary:
    name: str
    is_mutable_ref: bool
    fields: Dict[str, str] = field(default_factory=dict)


@dataclass
class PackageAnalysisResult:
    verdict: str
    diagnostics: List[str] = field(default_factory=list)
    resolved_types: Dict[str, TypeSummary] = field(default_factory=dict)
    checked_files: int = 0


STRUCT_RE = re.compile(r"type\s+([A-Za-z_]\w*)\s+struct\s*\{([^}]*)\}", re.DOTALL)
DIRECTIVE_RE = re.compile(r"//hy:([A-Za-z0-9_]+)(?:\(([^)]*)\))?")
DECL_RE = re.compile(r"^([A-Za-z_]\w*)\s*:=\s*(.+)$")
MOVE_RE = re.compile(r"hy\.Move\(\s*&?([A-Za-z_]\w*)\s*\)")
TASK_RE = re.compile(r"hy\.Task\((.+)\)")
LOAN_RE = re.compile(r"^([A-Za-z_]\w*)\s*:=\s*&([A-Za-z_]\w*)(?:\[[^\]]+\])?$")


def extract_package_types(
    go_files: Dict[str, str],
    export_data: Optional[Dict[str, bool]] = None,
) -> Dict[str, TypeSummary]:
    """Resolves struct shapes across all sibling .go files + imported go/types export data."""
    summaries: Dict[str, TypeSummary] = {}
    if export_data:
        for tname, is_mut in export_data.items():
            summaries[tname] = TypeSummary(name=tname, is_mutable_ref=bool(is_mut))

    raw_structs: Dict[str, Dict[str, str]] = {}
    for src in go_files.values():
        for match in STRUCT_RE.finditer(src):
            st_name, body = match.group(1), match.group(2)
            fields: Dict[str, str] = {}
            for line in body.splitlines():
                line = line.split("//", 1)[0].strip()
                parts = line.split()
                if len(parts) >= 2:
                    fields[parts[0]] = " ".join(parts[1:])
            raw_structs[st_name] = fields

    for st_name, fields in raw_structs.items():
        is_mut = False
        for ftype in fields.values():
            if any(ftype.startswith(p) or p in ftype for p in MUTABLE_FIELD_PATTERNS):
                is_mut = True
            elif ftype in summaries and summaries[ftype].is_mutable_ref:
                is_mut = True
            elif ftype in raw_structs:
                nested = raw_structs[ftype]
                if any(any(nf.startswith(p) for p in MUTABLE_FIELD_PATTERNS) for nf in nested.values()):
                    is_mut = True
        summaries[st_name] = TypeSummary(name=st_name, is_mutable_ref=is_mut, fields=fields)
    return summaries


def _infer_expr_mutability(
    rhs: str,
    types_map: Dict[str, TypeSummary],
    diags: List[str],
) -> Optional[bool]:
    rhs = rhs.strip()
    if rhs.startswith(("make(map", "make([]", "[]", "map[", "hy.NewOwn[", "hy.Own[")):
        return True
    if rhs.startswith("hy.View["):
        return True
    type_head = rhs.split("{", 1)[0].strip().lstrip("&")
    if type_head in VALUE_PRIMITIVES:
        return False
    if type_head in types_map:
        return types_map[type_head].is_mutable_ref
    if "{" in rhs and type_head:
        diags.append(f"HYB-E3005: unresolved type '{type_head}' missing from sibling .go and export data")
        return None
    return False


def check_package_go_files(
    go_files: Dict[str, str],
    export_data: Optional[Dict[str, bool]] = None,
) -> PackageAnalysisResult:
    """Runs whole-package Tier-0 checking across sibling .go files with //hy:check."""
    diags: List[str] = []
    types_map = extract_package_types(go_files, export_data)
    checked_files = 0

    for filename, src in sorted(go_files.items()):
        directives = DIRECTIVE_RE.findall(src)
        if not directives:
            continue
        for d_name, _d_args in directives:
            if d_name not in VALID_DIRECTIVES:
                diags.append(f"HYB-E3005: unknown directive '//hy:{d_name}' in {filename} (fail-closed)")
        if not any(d_name == "check" for d_name, _ in directives):
            continue

        checked_files += 1
        has_managed_permit = any(d_name == "managed" for d_name, _ in directives)
        var_mutable: Dict[str, bool] = {}
        moved_vars: Set[str] = set()
        active_loans: Dict[str, str] = {}
        in_loop = False

        for raw_line in src.splitlines():
            line = raw_line.split("//", 1)[0].strip()
            if not line:
                continue
            if line.startswith("for ") and line.endswith("{"):
                in_loop = True
                continue
            if line == "}" and in_loop:
                in_loop = False
                continue

            loan_m = LOAN_RE.match(line)
            if loan_m:
                loan_var, base_var = loan_m.group(1), loan_m.group(2)
                if base_var in moved_vars:
                    diags.append(f"HYB-E3001: cannot borrow '{base_var}' after hy.Move")
                active_loans[loan_var] = base_var
                var_mutable[loan_var] = True
                continue

            decl_m = DECL_RE.match(line)
            if decl_m and not line.startswith("for "):
                vname, rhs = decl_m.group(1), decl_m.group(2)
                if rhs in var_mutable and var_mutable[rhs]:
                    var_mutable[vname] = True
                else:
                    is_mut = _infer_expr_mutability(rhs, types_map, diags)
                    if is_mut is not None:
                        var_mutable[vname] = is_mut
                continue

            task_m = TASK_RE.search(line)
            if task_m:
                task_arg = task_m.group(1)
                moved_in_task = set(MOVE_RE.findall(task_arg))
                for mv in moved_in_task:
                    if mv in moved_vars:
                        diags.append(f"HYB-E3001: double hy.Move of '{mv}'")
                    if in_loop:
                        diags.append(f"HYB-E3003: loop-backedge hy.Move of '{mv}' inside for-loop")
                    if mv in active_loans.values():
                        diags.append(f"HYB-E3004: cannot hy.Move('{mv}') while borrowed")
                    if mv in active_loans:
                        diags.append(f"HYB-E3004: loan/view '{mv}' cannot cross hy.Task boundary")
                    moved_vars.add(mv)

                for vname, is_mut in var_mutable.items():
                    if vname in moved_in_task:
                        continue
                    if re.search(rf"\b{re.escape(vname)}\b", task_arg):
                        if vname in moved_vars:
                            diags.append(f"HYB-E3001: use of moved binding '{vname}' in hy.Task")
                        elif is_mut and not has_managed_permit:
                            diags.append(
                                f"HYB-E3002: mutable binding '{vname}' crosses hy.Task without hy.Move(&{vname})"
                            )
                continue

            for mv in MOVE_RE.findall(line):
                if mv in moved_vars:
                    diags.append(f"HYB-E3001: double hy.Move of '{mv}'")
                if in_loop:
                    diags.append(f"HYB-E3003: loop-backedge hy.Move of '{mv}'")
                moved_vars.add(mv)
                continue

            for mvar in list(moved_vars):
                if re.search(rf"\b{re.escape(mvar)}\b", line) and not MOVE_RE.search(line):
                    diags.append(f"HYB-E3001: post-move use of '{mvar}' in '{line}'")

            for base_var in set(active_loans.values()):
                if re.search(rf"\b{re.escape(base_var)}\s*\[[^\]]+\]\s*=", line):
                    diags.append(f"HYB-E3004: mutation of '{base_var}' while loan window is active")

    verdict = "ACCEPT" if not diags else "REJECT"
    return PackageAnalysisResult(
        verdict=verdict,
        diagnostics=diags,
        resolved_types=types_map,
        checked_files=checked_files,
    )
