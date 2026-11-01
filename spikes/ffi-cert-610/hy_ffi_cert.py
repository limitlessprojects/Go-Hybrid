#!/usr/bin/env python3
"""hy-ffi-cert: Proof-carrying FFI boundary contract generator & verifier (#610)."""
from __future__ import annotations
import hashlib, json, re, struct
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

SCHEMA = "hy.ffi.cert/v1"
EXPORT_PREFIXES = ("hy_", "hyrt_")
GUARD_SMALL, GUARD_DEFAULT = 1024, 4096
CLASS_ORDER = {"SMALL": 0, "DEFAULT": 1, "DEEP": 2}
ROUTE_CLASS = {"S": "SMALL", "": "DEFAULT", "Deep": "DEEP", "Raw": "RAW"}

E_SIGNATURE, E_STACK, E_RETAIN = "HYB-E7001", "HYB-E7002", "HYB-E7003"
E_CALLBACK, E_BLOCKING, E_HASH = "HYB-E7004", "HYB-E7005", "HYB-E7006"

RETAIN_PRIMITIVES = ("Box::into_raw", "Box::leak", "mem::forget", "ManuallyDrop", "ptr::write", "static mut", "into_raw(")
CALLBACK_PRIMITIVES = ('extern "C" fn(', "fn()", "dyn Fn", "as *const ()")
BLOCKING_PRIMITIVES = ("Mutex", "RwLock", "thread::sleep", "std::fs", "std::net", "stdin", "std::process", "Command::new")

EXTERN_RE = re.compile(r'extern\s+([A-Za-z0-9_*]+)\s+(hy\w+)\s*\(([^)]*)\);', re.MULTILINE)
ADDR_RE = re.compile(r"static\s+void\s*\*\s*a_(\w+)\s*\(\s*void\s*\)\s*(?:__attribute__\s*\(\([^)]*\)\)\s*)?\{\s*return\s*\(void\s*\*\s*\)\s*&\s*(hy_\w+|hyrt_\w+)\s*;")
FNVAR_RE = re.compile(r"(Fn\w+)\s*=\s*unsafe\.Pointer\(\s*C\.a_(\w+)\(\)\s*\)")
CROSS_HEAD_RE = re.compile(r'hybridge\.(Cross\w*)\(\s*(Fn\w+)')
FN_HEAD_RE = re.compile(r'pub\s+extern\s+"C"\s+fn\s+(\w+)\s*\(([^)]*)\)(?:\s*->\s*(\w+))?')

def uleb128(buf: bytes, pos: int) -> Tuple[int, int]:
    val, shift, idx = 0, 0, pos
    while idx < len(buf):
        b = buf[idx]; idx += 1; val |= (b & 0x7F) << shift
        if not (b & 0x80): break
        shift += 7
    return val, idx - pos

def canonical(obj: Any) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")

def digest(obj: Any) -> str:
    return hashlib.sha256(canonical(obj)).hexdigest()

class Elf:
    def __init__(self, data: bytes):
        if len(data) < 64 or data[:4] != b"\x7fELF" or data[4:6] != b"\x02\x01":
            raise ValueError("not 64-bit LE ELF")
        self.data = data
        (self.shoff,) = struct.unpack_from("<Q", data, 0x28)
        self.shentsize, self.shnum, self.shstrndx = struct.unpack_from("<HHH", data, 0x3A)
        self.sections: List[Dict[str, Any]] = []
        for i in range(self.shnum):
            off = self.shoff + i * self.shentsize
            if off + 64 > len(data): continue
            name, _, _, _, soff, size, _, info, _, _ = struct.unpack_from("<IIQQQQIIQQ", data, off)
            self.sections.append({"index": i, "name_off": name, "offset": soff, "size": size, "info": info, "name": ""})
        if self.shstrndx < len(self.sections):
            shstr = self.sections[self.shstrndx]
            for s in self.sections:
                end = self.data.find(b"\x00", shstr["offset"] + s["name_off"])
                if end != -1: s["name"] = self.data[shstr["offset"] + s["name_off"]:end].decode("utf-8", "replace")

    def sec(self, name: str) -> Optional[Dict[str, Any]]:
        return next((s for s in self.sections if s["name"] == name and s["size"] > 0), None)

    def raw(self, sec: Dict[str, Any]) -> bytes:
        return self.data[sec["offset"]:sec["offset"] + sec["size"]]

    def symtab(self) -> List[Tuple[str, int, int, int, int, int, int]]:
        sym, strt = self.sec(".symtab"), self.sec(".strtab")
        if not sym or not strt: return []
        out, base = [], strt["offset"]
        for i in range(sym["size"] // 24):
            off = sym["offset"] + i * 24
            if off + 24 > len(self.data): break
            name, info, _, shndx, val, size = struct.unpack_from("<IBBHQQ", self.data, off)
            end = self.data.find(b"\x00", base + name)
            sym_name = self.data[base + name:end].decode("utf-8", "replace") if end != -1 else ""
            out.append((sym_name, val, size, i, info >> 4, info & 0xF, shndx))
        return out

def archive_members(path: Path) -> List[Tuple[str, bytes]]:
    blob = path.read_bytes()
    if blob[:8] != b"!<arch>\n": raise ValueError(f"{path} not ar archive")
    pos, longnames, members = 8, b"", []
    while pos + 60 <= len(blob):
        hdr = blob[pos:pos + 60]
        name = hdr[:16].decode("utf-8", "replace").rstrip()
        size = int(hdr[48:58].decode("ascii", "replace").strip() or "0")
        body = blob[pos + 60:pos + 60 + size]
        pos += 60 + size + (size & 1)
        if name == "//": longnames = body; continue
        if name in ("/", "/SYM64/", "__.SYMDEF"): continue
        if name.startswith("/") and name[1:].isdigit():
            start = int(name[1:]); end = longnames.find(b"/\n", start)
            name = longnames[start:end].decode("utf-8", "replace") if end != -1 else name
        elif name.endswith("/"): name = name[:-1]
        members.append((name, body))
    return members

def stack_records(elf: Elf) -> Dict[str, int]:
    funcs_by_sec: Dict[int, List[str]] = {}
    for s in elf.symtab():
        if s[5] == 2: funcs_by_sec.setdefault(s[6], []).append(s[0])
    out = {}
    for rsec in elf.sections:
        if not rsec["name"].startswith(".rela.stack_sizes"): continue
        target = elf.sections[rsec["info"]] if rsec["info"] < len(elf.sections) else None
        if not target: continue
        body, pos = elf.raw(target), 0
        while pos + 8 < len(body):
            pos += 8; sz, n = uleb128(body, pos); pos += n
            for fn in funcs_by_sec.get(target["info"], []): out[fn] = max(out.get(fn, 0), sz)
    return out

def _split_args(args: str) -> List[str]:
    out, depth, cur = [], 0, ""
    for ch in args:
        if ch in "([{": depth += 1
        elif ch in ")]}": depth -= 1
        if ch == "," and depth == 0:
            if cur.strip(): out.append(cur.strip())
            cur = ""
        else: cur += ch
    if cur.strip(): out.append(cur.strip())
    return out

def _balanced_args(text: str, pos: int) -> str:
    depth, i = 0, pos
    while i < len(text):
        if text[i] == "(": depth += 1
        elif text[i] == ")":
            if depth == 0: return text[pos:i].lstrip(",").strip()
            depth -= 1
        i += 1
    return ""

def go_bindings(go_root: Path) -> Tuple[Dict[str, Any], Dict[str, str], List[Dict[str, Any]]]:
    externs, fnmap, sites = {}, {}, []
    for path in sorted(go_root.rglob("*.go")):
        text = path.read_text(encoding="utf-8", errors="replace")
        for m in EXTERN_RE.finditer(text):
            params = [a.strip() for a in m.group(3).split(",") if a.strip() and a.strip() != "void"]
            externs[m.group(2)] = {"ret": m.group(1).strip(), "params": params, "arity": len(params), "file": path.name}
        addr = {m.group(1): m.group(2) for m in ADDR_RE.finditer(text)}
        for m in FNVAR_RE.finditer(text): fnmap[m.group(1)] = addr.get(m.group(2), m.group(2))
        for m in CROSS_HEAD_RE.finditer(text):
            tramp, fnvar = m.group(1), m.group(2)
            args = _balanced_args(text, m.end())
            tm = re.match(r"Cross([A-Za-z]*?)([1-9])([A-Za-z]*)$", tramp)
            cls = ROUTE_CLASS.get((tm.group(3) or tm.group(1)) if tm else "", "DEFAULT")
            sites.append({"file": path.name, "trampoline": tramp, "fnvar": fnvar, "symbol": fnmap.get(fnvar, ""), "route_class": cls, "arity": len(_split_args(args))})
    return externs, fnmap, sites

def wrapper_effects(src_path: Path) -> Dict[str, Dict[str, Any]]:
    text = src_path.read_text(encoding="utf-8", errors="replace")
    effects, matches = {}, list(FN_HEAD_RE.finditer(text))
    for i, m in enumerate(matches):
        fn = m.group(1)
        chunk = text[m.start():matches[i + 1].start() if i + 1 < len(matches) else len(text)]
        params = [p.strip() for p in m.group(2).split(",") if p.strip()]
        effects[fn] = {
            "params": params, "arity": len(params), "ret": m.group(3) or "void",
            "may_retain_ptr": any(p in chunk for p in RETAIN_PRIMITIVES),
            "may_callback": any(p in chunk for p in CALLBACK_PRIMITIVES),
            "blocking": any(p in chunk for p in BLOCKING_PRIMITIVES),
        }
    return effects

def closure_facts(graph: Dict[str, List[str]], frames: Dict[str, int], root: str) -> Dict[str, Any]:
    stack_path, is_rec, indirect, missing, blocking = [], False, False, [], []
    def dfs(node: str) -> int:
        nonlocal is_rec, indirect
        if node in stack_path: is_rec = True; return 0
        if node.startswith(("*indirect", "dyn:")): indirect = True; return 0
        if any(b in node for b in BLOCKING_PRIMITIVES): blocking.append(node)
        frame = frames.get(node)
        if frame is None and not node.startswith("."): missing.append(node); frame = 0
        stack_path.append(node)
        max_c = max([dfs(nxt) for nxt in graph.get(node, [])], default=0)
        stack_path.pop()
        return (frame or 0) + max_c

    total = dfs(root)
    forcing = []
    if is_rec: forcing.append("recursion cycle")
    if indirect: forcing.append("indirect dispatch")
    if missing: forcing.append(f"{len(missing)} missing frame records")
    cls = "DEEP" if (is_rec or indirect or missing or total > GUARD_DEFAULT) else ("DEFAULT" if total > GUARD_SMALL else "SMALL")
    return {"bytes": total, "class": cls, "recursive": is_rec, "indirect_call": indirect, "missing_frames": missing[:5], "blocking_callees": blocking[:5], "forcing": forcing}

def verify(contract: Dict[str, Any], go_root: Path, artifact: Optional[Path] = None) -> Tuple[List[Dict[str, Any]], Dict[str, int]]:
    diags, (externs, _, sites), syms = [], go_bindings(go_root), contract.get("symbols", {})
    for sym, fact in syms.items():
        ext = externs.get(sym)
        if ext and ext["arity"] != fact["abi"]["arity"]:
            diags.append({"code": E_SIGNATURE, "symbol": sym, "why": f"extern arity {ext['arity']} != {fact['abi']['arity']}"})
    for s in sites:
        fact = syms.get(s["symbol"])
        if not fact: continue
        if s["arity"] != fact["abi"]["arity"]:
            diags.append({"code": E_SIGNATURE, "symbol": s["symbol"], "why": f"{s['trampoline']} arity mismatch"})
        if CLASS_ORDER.get(s["route_class"], 2) < CLASS_ORDER.get(fact["stack"]["class"], 2):
            diags.append({"code": E_STACK, "symbol": s["symbol"], "why": f"route {s['route_class']} < proven {fact['stack']['class']}"})
    for sym, fact in syms.items():
        eff, st = fact.get("effects", {}), fact.get("stack", {})
        if eff.get("may_retain_ptr"): diags.append({"code": E_RETAIN, "symbol": sym, "why": "retains pointer"})
        if st.get("indirect_call") and not eff.get("may_callback", True): diags.append({"code": E_CALLBACK, "symbol": sym, "why": "indirect call under no-callback"})
        if st.get("blocking_callees") and not eff.get("blocking", True): diags.append({"code": E_BLOCKING, "symbol": sym, "why": "blocking call under non-blocking"})
    if artifact and artifact.exists():
        actual, exp = hashlib.sha256(artifact.read_bytes()).hexdigest(), contract.get("artifact", {}).get("sha256")
        if exp and actual != exp: diags.append({"code": E_HASH, "symbol": "*", "why": f"sha256 {actual} != {exp}"})
    return diags, {"exports": len(syms), "call_sites": len(sites), "diagnostics": len(diags)}

def run_fixtures(base_contract: Dict[str, Any], go_root: Path) -> List[Dict[str, Any]]:
    res = []
    def check(name: str, code: str, mutated: Dict[str, Any], dummy: Optional[Path] = None):
        diags, _ = verify(mutated, go_root, dummy)
        res.append({"fixture": name, "expected": code, "rejected": any(d["code"] == code for d in diags)})

    m1 = json.loads(json.dumps(base_contract))
    next(iter(m1["symbols"].values()))["abi"]["arity"] += 1
    check("f1_signature_drift", E_SIGNATURE, m1)

    m2 = json.loads(json.dumps(base_contract))
    _, _, sites = go_bindings(go_root)
    sm = next((s["symbol"] for s in sites if s["route_class"] == "SMALL" and s["symbol"] in m2["symbols"]), None)
    if sm: m2["symbols"][sm]["stack"].update({"class": "DEEP", "recursive": True})
    check("f2_recursion_as_small", E_STACK, m2)

    m3 = json.loads(json.dumps(base_contract))
    next(iter(m3["symbols"].values()))["effects"]["may_retain_ptr"] = True
    check("f3_retained_borrow", E_RETAIN, m3)

    m4 = json.loads(json.dumps(base_contract))
    s4 = next(iter(m4["symbols"].values()))
    s4["stack"]["indirect_call"] = True; s4["effects"]["may_callback"] = False
    check("f4_callback_edge", E_CALLBACK, m4)

    m5 = json.loads(json.dumps(base_contract))
    s5 = next(iter(m5["symbols"].values()))
    s5["stack"]["blocking_callees"] = ["std::sync::Mutex::lock"]; s5["effects"]["blocking"] = False
    check("f5_blocking_beneath_nonblocking", E_BLOCKING, m5)

    m6 = json.loads(json.dumps(base_contract))
    m6["artifact"]["sha256"] = "0" * 64
    tmp = go_root / "_tmp_art.bin"
    tmp.write_bytes(b"invalid-bytes")
    try: check("f6_hash_mismatch", E_HASH, m6, dummy=tmp)
    finally:
        if tmp.exists(): tmp.unlink()
    return res

def main() -> int:
    import sys
    go_root = Path(__file__).resolve().parents[2] / "runtime" / "go"
    ext, _, _ = go_bindings(go_root)
    base = {"schema": SCHEMA, "artifact": {"sha256": "0" * 16}, "symbols": {s: {"abi": {"arity": e["arity"], "params": e["params"], "ret": e["ret"]}, "stack": {"bytes": 128, "class": "SMALL", "recursive": False, "indirect_call": False, "missing_frames": [], "blocking_callees": []}, "effects": {"may_retain_ptr": False, "may_callback": False, "blocking": False}} for s, e in ext.items()}}
    res = run_fixtures(base, go_root)
    print(f"Negative fixtures rejected: {sum(1 for r in res if r['rejected'])}/{len(res)}")
    for r in res:
        print(f"  {r['fixture']}: expected {r['expected']} -> {'REJECTED (fail-closed)' if r['rejected'] else 'UNCAUGHT'}")
    return 0 if all(r["rejected"] for r in res) else 1

if __name__ == "__main__":
    import sys; sys.exit(main())
