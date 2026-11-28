#!/usr/bin/env python3
"""
Executable Feasibility Verifier for Proposal #4071:
@[sync_guarded(mu)] — Externally-Synchronized Checked Obligations for Hybrid
"""

import sys
import os
import re
import json
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set, Tuple

@dataclass
class StructField:
    name: str
    type_name: str
    guarded_by: Optional[str] = None
    line: int = 0

@dataclass
class StructDef:
    name: str
    fields: Dict[str, StructField] = field(default_factory=dict)
    line: int = 0

@dataclass
class Diagnostic:
    code: str
    message: str
    line: int

class SyncGuardedVerifier:
    def __init__(self):
        self.structs: Dict[str, StructDef] = {}
        self.diagnostics: List[Diagnostic] = []
        self.var_types: Dict[str, str] = {}
        self.active_locks: Set[str] = set()

    def is_mutable_type(self, type_name: str) -> bool:
        t = type_name.strip()
        if t.startswith("map[") or t.startswith("[]") or t.startswith("*"):
            return True
        if t in self.structs:
            for f in self.structs[t].fields.values():
                if self.is_mutable_type(f.type_name) and not f.guarded_by:
                    return True
        return False

    def is_mutex_type(self, type_name: str) -> bool:
        return type_name in ("sync.mutex", "sync.rwmutex", "sync.Mutex", "sync.RWMutex")

    def parse_source(self, code: str) -> bool:
        lines = code.splitlines()
        i = 0
        current_struct = None
        pending_attr = None

        # Pass 1: Parse structs and annotations
        while i < len(lines):
            line_no = i + 1
            line = lines[i].strip()
            
            # Attribute detection: @[sync_guarded(mu)]
            attr_match = re.match(r"@\[sync_guarded\(([a-zA-Z0-9_]+)\)\]", line)
            if attr_match:
                pending_attr = attr_match.group(1)
                i += 1
                continue

            # Struct declaration
            struct_match = re.match(r"struct\s+([a-zA-Z0-9_]+)\s*\{?", line)
            if struct_match and not current_struct:
                struct_name = struct_match.group(1)
                current_struct = StructDef(name=struct_name, line=line_no)
                self.structs[struct_name] = current_struct
                i += 1
                continue

            if current_struct:
                if "}" in line:
                    # Validate struct fields
                    for fname, f in current_struct.fields.items():
                        if f.guarded_by:
                            if f.guarded_by not in current_struct.fields:
                                self.diagnostics.append(Diagnostic(
                                    code="HYB-E3002",
                                    message=f"field `{fname}` references non-existent mutex `{f.guarded_by}` in `sync_guarded`",
                                    line=f.line
                                ))
                            elif not self.is_mutex_type(current_struct.fields[f.guarded_by].type_name):
                                self.diagnostics.append(Diagnostic(
                                    code="HYB-E3002",
                                    message=f"field `{fname}` references field `{f.guarded_by}` of non-mutex type `{current_struct.fields[f.guarded_by].type_name}` in `sync_guarded`",
                                    line=f.line
                                ))
                    current_struct = None
                    pending_attr = None
                    i += 1
                    continue

                # Field line: name type
                parts = line.split()
                if len(parts) >= 2 and not line.startswith("//"):
                    fname = parts[0]
                    ftype = parts[1]
                    current_struct.fields[fname] = StructField(
                        name=fname,
                        type_name=ftype,
                        guarded_by=pending_attr,
                        line=line_no
                    )
                    pending_attr = None
            i += 1

        # Pass 2: Function checking, spawn boundary validation, and lock-region tracking
        i = 0
        while i < len(lines):
            line_no = i + 1
            line = lines[i].strip()

            # Variable type declarations: var s stats, h *holder, etc.
            var_decl = re.findall(r"(?:var\s+([a-zA-Z0-9_]+)\s+([a-zA-Z0-9_.*]+))|([a-zA-Z0-9_]+)\s*:\s*([*]?[a-zA-Z0-9_]+)", line)
            for v1, t1, v2, t2 in var_decl:
                v = v1 or v2
                t = (t1 or t2).replace("*", "")
                if v and t:
                    self.var_types[v] = t

            # Fn parameters: fn worker(h *holder, ...)
            fn_match = re.match(r"fn\s+[a-zA-Z0-9_]+\s*\(([^)]*)\)", line)
            if fn_match:
                self.active_locks.clear()
                params = fn_match.group(1).split(",")
                for p in params:
                    p = p.strip()
                    if p:
                        parts = p.split()
                        if len(parts) >= 2:
                            pname = parts[0]
                            ptype = parts[1].replace("*", "")
                            self.var_types[pname] = ptype

            # Task spawn / concurrency boundary: go worker(h, done) or task worker(h)
            spawn_match = re.match(r"(?:go|task|spawn)\s+[a-zA-Z0-9_]+\s*\(([^)]*)\)", line)
            if spawn_match:
                args = [a.strip() for a in spawn_match.group(1).split(",") if a.strip()]
                for arg in args:
                    vname = arg
                    vtype = self.var_types.get(vname, "")
                    if vtype in self.structs:
                        sdef = self.structs[vtype]
                        for f in sdef.fields.values():
                            if self.is_mutable_type(f.type_name) and not f.guarded_by:
                                self.diagnostics.append(Diagnostic(
                                    code="HYB-E3001",
                                    message=f"struct `{sdef.name}` contains unguarded mutable field `{f.name}` and cannot cross spawn sites (GUARANTEES.md Row 8)",
                                    line=line_no
                                ))

            # Lock acquisition: h.mu.lock(), h.mu.rlock()
            lock_match = re.search(r"([a-zA-Z0-9_]+)\.([a-zA-Z0-9_]+)\.(?:lock|rlock)\(\)", line)
            if lock_match:
                lock_key = f"{lock_match.group(1)}.{lock_match.group(2)}"
                self.active_locks.add(lock_key)

            # Lock release: h.mu.unlock(), h.mu.runlock()
            unlock_match = re.search(r"([a-zA-Z0-9_]+)\.([a-zA-Z0-9_]+)\.(?:unlock|runlock)\(\)", line)
            if unlock_match:
                lock_key = f"{unlock_match.group(1)}.{unlock_match.group(2)}"
                self.active_locks.discard(lock_key)

            # Field accesses: h.m["x"], h.m, etc.
            access_matches = re.finditer(r"([a-zA-Z0-9_]+)\.([a-zA-Z0-9_]+)", line)
            for acc in access_matches:
                vname = acc.group(1)
                fname = acc.group(2)
                vtype = self.var_types.get(vname, "")
                if vtype in self.structs:
                    sdef = self.structs[vtype]
                    if fname in sdef.fields:
                        field_def = sdef.fields[fname]
                        if field_def.guarded_by:
                            required_lock = f"{vname}.{field_def.guarded_by}"
                            if required_lock not in self.active_locks:
                                self.diagnostics.append(Diagnostic(
                                    code="HYB-E3003",
                                    message=f"access to guarded field `{fname}` requires lock `{field_def.guarded_by}` to be held in the current region",
                                    line=line_no
                                ))

            i += 1

        return len(self.diagnostics) == 0

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 sync_guarded_verifier.py <file.hy> [--json]")
        sys.exit(2)

    target_file = sys.argv[1]
    output_json = "--json" in sys.argv

    if not os.path.exists(target_file):
        print(f"Error: File {target_file} not found.", file=sys.stderr)
        sys.exit(2)

    with open(target_file, "r", encoding="utf-8") as f:
        content = f.read()

    verifier = SyncGuardedVerifier()
    passed = verifier.parse_source(content)

    if output_json:
        result = {
            "file": target_file,
            "verdict": "ACCEPT" if passed else "REJECT",
            "diagnostics": [
                {"code": d.code, "message": d.message, "line": d.line}
                for d in verifier.diagnostics
            ]
        }
        print(json.dumps(result, indent=2))
    else:
        if passed:
            print(f"{target_file}: OK (Verdict: ACCEPT, 0 diagnostics)")
        else:
            print(f"{target_file}: FAIL (Verdict: REJECT)")
            for d in verifier.diagnostics:
                print(f"  line {d.line}: {d.code}: {d.message}")

    sys.exit(0 if passed else 1)

if __name__ == "__main__":
    main()
