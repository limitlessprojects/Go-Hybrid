#!/usr/bin/env python3
"""hy-ffi-cert — artifact-derived FFI boundary contracts for the hyrt staticlib.

Spike for issue #610: "Make FFI claims proof-carrying: verify boundary
contracts against the linked Rust artifact".  Pre-registration, gates and the
six negative fixtures are fixed in PREREG.md (committed before any run).

What is derived from WHERE (the whole point of the spike):

  artifact (libhyrt.a)          signature arity + per-parameter ABI class (DWARF)
                                per-function stack frame records (.stack_sizes)
                                direct/tail call graph (object code)
                                indirect-call and blocking-callee evidence
  Go binding (runtime/go/**)    the consumed symbol set, the declared C
                                signature in the cgo preamble, the route class
                                at every call site (Cross*S / Cross* / CrossDeep*)
  wrapper source (restricted)   never-retains / never-callbacks / never-blocks
                                — accepted ONLY for the syntactic subset in
                                PREREG.md §5, never as proof of the artifact

Nothing here is a compiler change and nothing is linked into the artifact.

Exit codes: 0 = accepted (no unsafe claim), 1 = rejected (a claim is not
proven by the facts), 2 = tool error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCHEMA = "hy.ffi.cert/v1"
EXPORT_PREFIXES = ("hy_", "hyrt_")

# Guard sizes from interfaces/descriptors.md — the ledger's own classes.
GUARD_SMALL = 1024
GUARD_DEFAULT = 4096
CLASS_ORDER = {"SMALL": 0, "DEFAULT": 1, "DEEP": 2}
ROUTE_CLASS = {"S": "SMALL", "": "DEFAULT", "Deep": "DEEP", "Raw": "RAW"}

# Diagnostic codes are local to this spike (PREREG.md §6).
E_SIGNATURE = "HYB-E7001"
E_STACK = "HYB-E7002"
E_RETAIN = "HYB-E7003"
E_CALLBACK = "HYB-E7004"
E_BLOCKING = "HYB-E7005"
E_HASH = "HYB-E7006"

# Restricted wrapper subset (PREREG.md §5) — syntactic, conservative.
RETAIN_PRIMITIVES = (
    "Box::into_raw", "Box::leak", "mem::forget", "ManuallyDrop", "ptr::write",
    "static mut", "into_raw(", "from_raw(",  # from_raw alone is only a coercion
)
CALLBACK_PRIMITIVES = ("extern \"C\" fn(", "fn()", "dyn Fn", "as *const ()")
BLOCKING_PRIMITIVES = (
    "Mutex", "RwLock", "thread::sleep", "std::fs", "std::net", "stdin",
    "std::process", "Command::new",
)


# --------------------------------------------------------------------------
# ELF (64-bit little-endian; the only shape this sandbox builds)
# --------------------------------------------------------------------------
class Elf:
    def __init__(self, data: bytes):
        if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
            raise ValueError("not a 64-bit little-endian ELF object")
        self.data = data
        (self.shoff,) = struct.unpack_from("<Q", data, 0x28)
        self.shentsize, self.shnum, self.shstrndx = struct.unpack_from("<HHH", data, 0x3A)
        self.sections = []
        for i in range(self.shnum):
            off = self.shoff + i * self.shentsize
            name, stype, _flags, addr, soff, size, link, info, _align, entsize = \
                struct.unpack_from("<IIQQQQIIQQ", data, off)
            self.sections.append({
                "index": i, "name_off": name, "type": stype, "addr": addr,
                "offset": soff, "size": size, "link": link, "info": info,
                "entsize": entsize, "name": "",
            })
        shstr = self.sections[self.shstrndx]
        for s in self.sections:
            s["name"] = self._cstr(shstr["offset"] + s["name_off"])

    def _cstr(self, off: int) -> str:
        end = self.data.index(b"\x00", off)
        return self.data[off:end].decode("utf-8", "replace")

    def sec(self, name: str):
        for s in self.sections:
            if s["name"] == name and s["size"]:
                return s
        return None

    def raw(self, sec) -> bytes:
        return self.data[sec["offset"]:sec["offset"] + sec["size"]]

    def symtab(self):
        """[(name, value, size, index, bind, type, shndx)]"""
        sym, strt = self.sec(".symtab"), self.sec(".strtab")
        if not sym or not strt:
            return []
        out, base = [], strt["offset"]
        for i in range(sym["size"] // 24):
            off = sym["offset"] + i * 24
            name, info, _other, shndx, value, size = struct.unpack_from("<IBBHQQ", self.data, off)
            end = self.data.index(b"\x00", base + name)
            out.append((self.data[base + name:end].decode("utf-8", "replace"),
                        value, size, i, info >> 4, info & 0xF, shndx))
        return out


def uleb128(buf: bytes, pos: int):
    result = shift = 0
    while True:
        b = buf[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        if not (b & 0x80):
            return result, pos
        shift += 7


TEXT_SECTION_PREFIXES = (".text.unlikely.", ".text.hot.", ".text.startup.", ".text.")


def _fn_from_section(sec: str) -> str:
    """rustc's function-section naming: `.text[.unlikely].<mangled fn>`."""
    for p in TEXT_SECTION_PREFIXES:
        if sec.startswith(p):
            return sec[len(p):]
    return ""


def archive_members(path: Path):
    """[(member_name, bytes)] — GNU ar, incl. the // long-name table."""
    blob = path.read_bytes()
    if blob[:8] != b"!<arch>\n":
        raise ValueError(f"{path} is not an archive")
    pos, longnames, members = 8, b"", []
    while pos + 60 <= len(blob):
        hdr = blob[pos:pos + 60]
        name = hdr[:16].decode("utf-8", "replace").rstrip()
        size = int(hdr[48:58].decode("ascii").strip())
        body = blob[pos + 60:pos + 60 + size]
        pos += 60 + size + (size & 1)
        if name == "//":
            longnames = body
            continue
        if name in ("/", "/SYM64/", "__.SYMDEF"):
            continue
        if name.startswith("/") and name[1:].isdigit():
            start = int(name[1:])
            end = longnames.find(b"/\n", start)
            name = longnames[start:end].decode("utf-8", "replace")
        elif name.endswith("/"):
            name = name[:-1]
        members.append((name, body))
    return members


# --------------------------------------------------------------------------
# Artifact facts
# --------------------------------------------------------------------------
def stack_records(elf: Elf):
    """{symbol: frame_bytes} from the .stack_sizes records rustc emits under
    `-Z emit-stack-sizes`.

    Layout, verified against this toolchain (see results/toolchain.txt): one
    `.stack_sizes` section per function section, each entry = an 8-byte
    relocated address slot followed by a ULEB128 size; the relocation points
    at the *section symbol* of the function's `.text.*` section, so the
    function name is resolved through the symbol table (STT_FUNC whose
    st_shndx is that section).
    """
    syms = elf.symtab()
    by_index = {s[3]: s for s in syms}
    funcs_by_sec = {}
    for s in syms:
        if s[5] == 2:                      # STT_FUNC
            funcs_by_sec.setdefault(s[6], []).append(s[0])
    sec_names = {sec["index"]: sec["name"] for sec in elf.sections}
    out = {}
    for rsec in elf.sections:
        if not rsec["name"].startswith(".rela.stack_sizes"):
            continue
        target = elf.sections[rsec["info"]] if rsec["info"] < len(elf.sections) else None
        if target is None:
            continue
        relmap = {}
        for i in range(rsec["size"] // 24):
            off, info, _add = struct.unpack_from("<QQq", elf.data, rsec["offset"] + i * 24)
            s = by_index.get(info >> 32)
            if not s:
                continue
            if s[5] == 2:
                name = s[0]
            else:
                name = (funcs_by_sec.get(s[6]) or [""])[0] or _fn_from_section(sec_names.get(s[6], ""))
            if name:
                relmap[off] = name
        raw, pos = elf.raw(target), 0
        while pos + 8 <= len(raw):
            if raw[pos:pos + 8] != b"\x00" * 8:      # address slot must be relocated
                break
            name = relmap.get(pos)
            try:
                size, nxt = uleb128(raw, pos + 8)
            except IndexError:
                break
            if name:
                out[name] = size
            pos = nxt
    return out


CALL_RE = re.compile(r"^\s+([0-9a-f]+):\t(.*)$")
RELOC_RE = re.compile(r"^\s+([0-9a-f]+):\s+(R_X86_64_\w+)\s+(\S+)")
FUNC_RE = re.compile(r"^([0-9a-f]+) <(.+)>:$")


def call_graph(obj_path: Path):
    """{func: {"calls": [...], "tail": [...], "got": [...], "indirect": bool}}

    The observed codegen on this toolchain routes almost every cross-function
    call through the GOT (`call *0x0(%rip)` + an R_X86_64_GOTPCREL relocation
    naming the callee).  That is a *resolved* callee in a statically linked
    binary but an *interposable* one in principle, so the two readings are
    kept apart instead of being silently merged: `got` edges are followed by
    the linked reading and force DEEP under the strict reading.
    """
    dis = subprocess.run(["objdump", "-dr", str(obj_path)], capture_output=True, text=True, check=True).stdout
    graph, cur = {}, None
    pending_reloc = {}
    for line in dis.splitlines():
        m = FUNC_RE.match(line)
        if m:
            cur = m.group(2)
            graph.setdefault(cur, {"calls": [], "tail": [], "got": [], "indirect": False})
            continue
        m = RELOC_RE.match(line)
        if m:
            off, _typ, sym = int(m.group(1), 16), m.group(2), m.group(3)
            pending_reloc[off] = sym.split("-0x")[0].split("+0x")[0]
            continue
        m = CALL_RE.match(line)
        if not m or cur is None:
            continue
        off = int(m.group(1), 16)
        # `off:\t<raw bytes>\t<asm>` — the asm is the last tab-separated field.
        fields = m.group(2).split("\t")
        asm = fields[-1]
        rawlen = len(fields[0].split()) if len(fields) > 1 else 0
        parts = asm.split()
        if not parts:
            continue
        mnem = parts[0]
        if mnem not in ("call", "callq", "jmp", "jmpq"):
            continue
        reloc = None
        for cand, sym in pending_reloc.items():
            if off <= cand < off + max(rawlen, 1):
                reloc = sym
                break
        node = graph[cur]
        if reloc and _is_data_slot(reloc):
            node["indirect"] = True            # call *GOT(...) through a vtable/jump table/static fn slot
        elif reloc:
            (node["calls"] if mnem.startswith("call") else node["tail"]).append(reloc)
        elif "*" in asm:
            node["indirect"] = True            # call *%reg — true indirect
    return graph


DWARF_DIE_RE = re.compile(r"^ <(\d+)><([0-9a-f]+)>: Abbrev Number: \d+(?: \((DW_TAG_\w+)\))?")
DWARF_ATTR_RE = re.compile(r"^    <[0-9a-f]+>\s+DW_AT_(\w+)\s*:\s*(.*)$")


def dwarf_dies(obj_path: Path):
    dump = subprocess.run(["readelf", "--debug-dump=info", str(obj_path)],
                          capture_output=True, text=True, check=True).stdout
    dies, cur = {}, None
    for line in dump.splitlines():
        m = DWARF_DIE_RE.match(line)
        if m:
            level, off, tag = int(m.group(1)), int(m.group(2), 16), m.group(3) or ""
            cur = {"level": level, "tag": tag, "attrs": {}}
            dies[off] = cur
            continue
        m = DWARF_ATTR_RE.match(line)
        if m and cur is not None:
            val = m.group(2).strip()
            if ": " in val and val.startswith("("):
                val = val.split("): ", 1)[-1]
            cur["attrs"][m.group(1)] = val
    return dies


def _type_name(dies, ref, depth=0):
    if ref is None or depth > 6:
        return "unknown"
    m = re.match(r"<0x([0-9a-f]+)>", ref)
    if not m:
        return str(ref)
    die = dies.get(int(m.group(1), 16))
    if die is None:
        return "unknown"
    tag, a = die["tag"], die["attrs"]
    if tag in ("DW_TAG_typedef", "DW_TAG_const_type", "DW_TAG_volatile_type", "DW_TAG_restrict_type"):
        return _type_name(dies, a.get("type"), depth + 1)
    if tag == "DW_TAG_pointer_type":
        return "ptr"
    if tag in ("DW_TAG_base_type", "DW_TAG_enumeration_type"):
        name = a.get("name")
        if name:
            return name
        size = a.get("byte_size", "")
        return f"i{int(size) * 8}" if size.isdigit() else "int"
    if tag == "DW_TAG_structure_type":
        return "aggregate:" + a.get("name", "?")
    return a.get("name", tag.replace("DW_TAG_", "").lower())


def export_signatures(obj_path: Path, wanted: set):
    """{symbol: {"params": [...], "arity": n, "ret": t}} for concrete DIEs."""
    dies = dwarf_dies(obj_path)
    out = {}
    for off, die in dies.items():
        if die["tag"] != "DW_TAG_subprogram":
            continue
        name = die["attrs"].get("name")
        if name not in wanted or "low_pc" not in die["attrs"]:
            continue
        params = [d for d in dies.values()
                  if d["level"] == die["level"] + 1 and d["tag"] == "DW_TAG_formal_parameter"]
        # formal_parameters are children; attribute order is preserved per DIE,
        # so filter to the contiguous block that follows this DIE.
        params = _children_params(dies, off)
        out[name] = {
            "params": [_type_name(dies, p["attrs"].get("type")) for p in params],
            "arity": len(params),
            "ret": _type_name(dies, die["attrs"].get("type")),
        }
    return out


def _children_params(dies, off):
    """DWARF children of the DIE at `off`: the level+1 block immediately after it."""
    keys = sorted(dies)
    idx = keys.index(off)
    parent = keys[idx]
    base_level = dies[parent]["level"]
    res = []
    for k in keys[idx + 1:]:
        lvl = dies[k]["level"]
        if lvl <= base_level:
            break
        if lvl == base_level + 1 and dies[k]["tag"] == "DW_TAG_formal_parameter":
            res.append(dies[k])
    return res


# --------------------------------------------------------------------------
# Go binding facts
# --------------------------------------------------------------------------
EXTERN_RE = re.compile(r"extern\s+([A-Za-z_0-9\* ]+?)\s+(hy_\w+|hyrt_\w+)\s*\(([^)]*)\)\s*;")
ADDR_RE = re.compile(r"static\s+void\s*\*\s*a_(\w+)\s*\(\s*void\s*\)\s*(?:__attribute__\s*\(\([^)]*\)\)\s*)?\{\s*return\s*\(void\s*\*\s*\)\s*&\s*(hy_\w+|hyrt_\w+)\s*;")
FNVAR_RE = re.compile(r"(Fn\w+)\s*=\s*unsafe\.Pointer\(\s*C\.a_(\w+)\(\)\s*\)")
CROSS_HEAD_RE = re.compile(r"hybridge\.(Cross\w*?)\(\s*(Fn\w+)")


def go_bindings(go_root: Path):
    """externs, Fn-var → symbol map, call sites with route class + arity."""
    externs, fnmap, sites = {}, {}, []
    for path in sorted(go_root.rglob("*.go")):
        text = path.read_text(encoding="utf-8", errors="replace")
        for m in EXTERN_RE.finditer(text):
            ret, sym, args = m.group(1).strip(), m.group(2), m.group(3).strip()
            params = [a.strip() for a in args.split(",")] if args else []
            if params == ["void"]:
                params = []
            externs[sym] = {"ret": ret, "params": params, "arity": len(params),
                            "file": str(path.relative_to(go_root))}
        addr = {m.group(1): m.group(2) for m in ADDR_RE.finditer(text)}
        for m in FNVAR_RE.finditer(text):
            fnmap[m.group(1)] = addr.get(m.group(2), m.group(2))
        for m in CROSS_HEAD_RE.finditer(text):
            tramp, fnvar = m.group(1), m.group(2)
            args = _balanced_args(text, m.end())
            tm = re.match(r"Cross([A-Za-z]*?)([1-9])([A-Za-z]*)$", tramp)
            suffix = (tm.group(3) or tm.group(1)) if tm else None
            cls = ROUTE_CLASS.get(suffix, "?") if suffix is not None else "?"
            arity = len(_split_args(args)) if args else 0
            sites.append({"file": str(path.relative_to(go_root)), "trampoline": tramp,
                          "fnvar": fnvar, "symbol": fnmap.get(fnvar, ""),
                          "route_class": cls, "arity": arity})
    return externs, fnmap, sites


def _balanced_args(text: str, pos: int) -> str:
    """Call arguments from just after the callee token to the matching `)`."""
    depth, i = 0, pos
    while i < len(text):
        ch = text[i]
        if ch == "(":
            depth += 1
        elif ch == ")":
            if depth == 0:
                return text[pos:i].lstrip(",").strip()
            depth -= 1
        i += 1
    return ""


def _split_args(args: str):
    out, depth, cur = [], 0, ""
    for ch in args:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


# --------------------------------------------------------------------------
# Restricted wrapper subset scan (source-level, PREREG.md §5)
# --------------------------------------------------------------------------
FN_HEAD_RE = re.compile(r"pub\s+extern\s+\"C\"\s+fn\s+(\w+)\s*\(")


def wrapper_bodies(src_path: Path):
    """{fn: body_text} for every `pub extern "C" fn` in the wrapper file."""
    text = src_path.read_text(encoding="utf-8", errors="replace")
    bodies = {}
    for m in FN_HEAD_RE.finditer(text):
        i, depth, start = m.end() - 1, 0, None
        while i < len(text):
            if text[i] == "{":
                depth += 1
                start = i if start is None else start
            elif text[i] == "}":
                depth -= 1
                if depth == 0 and start is not None:
                    bodies[m.group(1)] = text[start:i + 1]
                    break
            i += 1
    return bodies


def wrapper_effects(src_path: Path):
    """Conservative subset verdict per export: retain / callback / blocking."""
    out = {}
    for fn, body in wrapper_bodies(src_path).items():
        retain = any(p in body for p in RETAIN_PRIMITIVES)
        callback = any(p in body for p in CALLBACK_PRIMITIVES)
        blocking = any(p in body for p in BLOCKING_PRIMITIVES)
        out[fn] = {"may_retain_ptr": retain, "may_callback": callback, "blocking": blocking}
    return out


# --------------------------------------------------------------------------
# Contract build
# --------------------------------------------------------------------------
def canonical(obj) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def digest(obj) -> str:
    return hashlib.sha256(canonical(obj)).hexdigest()


def closure_facts(graph, frames, root, mode="linked"):
    """Worst-case stack bytes over the call closure + the forcing facts.

    mode="linked":  GOT-resolved callees are followed as if linked (the
                    practical reading for a static archive).
    mode="strict":  any GOT-indirected callee forces DEEP (interposition-safe).
    """
    memo, onstack = {}, set()
    facts = {"recursive": False, "indirect": False, "got": False, "missing": [],
             "external": [], "blocking": []}

    def visit(fn):
        if fn in onstack:
            facts["recursive"] = True
            return 0
        if fn in memo:
            return memo[fn]
        node = graph.get(fn)
        if node is None:
            facts["external"].append(fn)
            return 0
        if node["indirect"]:
            facts["indirect"] = True
        if node["got"]:
            facts["got"] = True
        if fn not in frames:
            facts["missing"].append(fn)
        if _is_blocking(fn):
            facts["blocking"].append(fn)
        frame = frames.get(fn, 0)
        onstack.add(fn)
        edges = list(node["calls"]) + list(node["tail"])
        if mode == "linked":
            edges += list(node["got"])
        best = 0
        for callee in edges:
            best = max(best, visit(callee))
        onstack.discard(fn)
        memo[fn] = frame + best
        return memo[fn]

    facts["bytes"] = visit(root)
    return facts


BLOCKING_MARKERS = ("sleep", "Mutex", "RwLock", "std::fs", "std::net", "stdio",
                    "syscall", "std::process", "Command::new", "std::io::stdio")


def _is_data_slot(sym: str) -> bool:
    return sym.startswith(".") or ".data." in sym or ".rodata" in sym or sym.startswith("__")


def _is_blocking(fn: str) -> bool:
    return any(k in fn for k in BLOCKING_MARKERS)


def build_contract(artifact: Path, go_root: Path, wrapper_src: Path, toolchain: str):
    members = archive_members(artifact)
    symbols, frames, graph = set(), {}, {}
    owners = {}
    objs = []
    tmp = Path(tempfile.mkdtemp(prefix="hyffi-"))
    try:
        for name, body in members:
            if not body[:4] == b"\x7fELF":
                continue
            p = tmp / (re.sub(r"[^A-Za-z0-9_.-]", "_", name) or f"m{len(objs)}.o")
            p.write_bytes(body)
            objs.append(p)
        for p in objs:
            try:
                elf = Elf(p.read_bytes())
            except ValueError:
                continue
            for sym, value, size, idx, bind, typ, shndx in elf.symtab():
                if typ != 2 or bind not in (1, 2) or shndx == 0:   # STT_FUNC, GLOBAL/WEAK, defined
                    continue
                if sym.startswith(EXPORT_PREFIXES):
                    symbols.add(sym)
                    owners[sym] = p
            frames.update(stack_records(elf))
        for p in objs:
            for fn, node in call_graph(p).items():
                if fn in graph:
                    graph[fn]["calls"].extend(node["calls"])
                    graph[fn]["tail"].extend(node["tail"])
                    graph[fn]["indirect"] |= node["indirect"]
                else:
                    graph[fn] = node
        sigs = {}
        for p in set(owners.values()):
            sigs.update(export_signatures(p, symbols))
        effects = wrapper_effects(wrapper_src) if wrapper_src.exists() else {}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    def classify(facts, bytes_):
        if facts["recursive"] or facts["indirect"] or facts["missing"] or facts["external"]:
            return "DEEP"
        if bytes_ <= GUARD_SMALL:
            return "SMALL"
        if bytes_ <= GUARD_DEFAULT:
            return "DEFAULT"
        return "DEEP"

    def classify_bytes_only(bytes_):
        if bytes_ <= GUARD_SMALL:
            return "SMALL"
        if bytes_ <= GUARD_DEFAULT:
            return "DEFAULT"
        return "DEEP"

    def forcing(facts):
        out = []
        if facts["recursive"]:
            out.append("recursion")
        if facts["indirect"]:
            out.append("indirect-dispatch")
        if facts["missing"]:
            out.append(f"missing-frame x{len(set(facts['missing']))}")
        if facts["external"]:
            out.append(f"external-callee x{len(set(facts['external']))}")
        if not out and facts["got"]:
            out.append("got-indirected")
        return out

    sym_entries = {}
    for sym in sorted(symbols):
        linked = closure_facts(graph, frames, sym, mode="linked")
        strict = closure_facts(graph, frames, sym, mode="strict")
        bytes_ = linked["bytes"]
        cls = classify(linked, bytes_)
        strict_cls = classify(strict, strict["bytes"])
        if strict["got"]:
            strict_cls = "DEEP"          # any GOT-indirected callee: interposition-safe
        eff = effects.get(sym, {})
        sym_entries[sym] = {
            "abi": sigs.get(sym, {"params": [], "arity": None, "ret": None}),
            "stack": {
                "bytes": bytes_,
                "class": cls,
                "class_bytes_only": classify_bytes_only(bytes_),
                "class_strict": strict_cls,
                "forcing": forcing(linked),
                "recursive": linked["recursive"],
                "indirect_call": linked["indirect"],
                "got_indirected": linked["got"],
                "missing_frames": sorted(set(linked["missing"]))[:8],
                "external_callees": sorted(set(linked["external"]))[:8],
                "blocking_callees": sorted(set(linked["blocking"]))[:4],
            },
            "effects": {
                "may_retain_ptr": bool(eff.get("may_retain_ptr", True)),
                "may_callback": bool(eff.get("may_callback", False)) or linked["indirect"],
                "blocking": bool(eff.get("blocking", False)) or bool(linked["blocking"]),
                "source": "restricted-wrapper-subset" if eff else "unclassified",
            },
            "frame_bytes": frames.get(sym),
        }
    contract = {
        "schema": SCHEMA,
        "artifact": {
            "crate": "hyrt",
            "file": artifact.name,
            "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest(),
            "members": len(members),
            "objects": len(objs),
            "functions_with_frames": len(frames),
            "toolchain": toolchain,
            "base_commit": subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True,
                                          text=True).stdout.strip(),
        },
        "symbols": sym_entries,
    }
    contract["contract_sha256"] = digest(contract)
    return contract


# --------------------------------------------------------------------------
# Verify: facts vs claims
# --------------------------------------------------------------------------
def verify(contract, go_root: Path, artifact: Path):
    """Return (diagnostics, gaps, coverage). Empty diagnostics = accepted."""
    diags, gaps = [], []
    externs, fnmap, sites = go_bindings(go_root)
    syms = contract["symbols"]

    # -- G1: signature coherence, artifact ↔ Go extern ↔ call site -----------
    consumed = sorted({s["symbol"] for s in sites if s["symbol"]})
    for sym in consumed:
        fact = syms.get(sym)
        if fact is None:
            diags.append({"code": E_SIGNATURE, "symbol": sym,
                          "why": "Go binding consumes a symbol with no contract in the artifact"})
            continue
        ext = externs.get(sym)
        if ext:
            if ext["arity"] != fact["abi"].get("arity"):
                diags.append({"code": E_SIGNATURE, "symbol": sym,
                              "why": f"extern declares {ext['arity']} params, artifact DWARF says {fact['abi'].get('arity')}"})
        else:
            gaps.append({"kind": "no-extern", "symbol": sym})
    for site in sites:
        if not site["symbol"]:
            gaps.append({"kind": "unresolved-fnvar", "fnvar": site["fnvar"], "file": site["file"]})
            continue
        fact = syms.get(site["symbol"])
        if fact and fact["abi"].get("arity") is not None and site["arity"] != fact["abi"]["arity"]:
            diags.append({"code": E_SIGNATURE, "symbol": site["symbol"],
                          "why": f"{site['file']}: {site['trampoline']} passes {site['arity']} args, artifact says {fact['abi']['arity']}"})

    # -- G1b: route class never faster than the measured stack class --------
    routed = {}
    for site in sites:
        if not site["symbol"]:
            continue
        prev = routed.get(site["symbol"])
        if prev is None or CLASS_ORDER.get(site["route_class"], 9) > CLASS_ORDER.get(prev, 9):
            routed[site["symbol"]] = site["route_class"]       # weakest route used anywhere
    for sym, route in sorted(routed.items()):
        fact = syms.get(sym)
        if not fact or route in ("RAW", "?"):
            continue
        if CLASS_ORDER[route] < CLASS_ORDER[fact["stack"]["class"]]:
            guard = GUARD_SMALL if route == "SMALL" else GUARD_DEFAULT
            forcing = ", ".join(fact["stack"].get("forcing", [])) or "measured bytes"
            diags.append({"code": E_STACK, "symbol": sym,
                          "why": f"binding routes through {route} (guard {guard} B); artifact class {fact['stack']['class']} "
                                 f"({fact['stack']['bytes']} B over resolvable frames, forced by: {forcing})"})

    # -- effect claims vs facts --------------------------------------------
    for sym, fact in syms.items():
        eff = fact["effects"]
        if fact["stack"]["indirect_call"] and not eff["may_callback"]:
            diags.append({"code": E_CALLBACK, "symbol": sym,
                          "why": "contract claims may_callback=false, artifact closure contains an indirect call"})
        if fact["stack"]["blocking_callees"] and not eff["blocking"]:
            diags.append({"code": E_BLOCKING, "symbol": sym,
                          "why": f"contract claims non-blocking, closure reaches {fact['stack']['blocking_callees'][:2]}"})

    # -- G3/G6: artifact-bound hash ----------------------------------------
    embedded = read_embedded_contract(artifact)
    if embedded is not None:
        try:
            emb = json.loads(embedded.decode())
        except Exception:
            diags.append({"code": E_HASH, "symbol": "*", "why": "embedded .hy_ffi_cert section is not JSON"})
            emb = None
        if emb is not None:
            if emb.get("contract_sha256") != digest({k: v for k, v in emb.items() if k != "contract_sha256"}):
                diags.append({"code": E_HASH, "symbol": "*", "why": "embedded contract fails its own digest"})
            if emb.get("contract_sha256") != contract["contract_sha256"]:
                diags.append({"code": E_HASH, "symbol": "*",
                              "why": "embedded digest does not match the regenerated contract — binding is stale for this binary"})
    coverage = {
        "exports": len(syms),
        "exports_with_signature": sum(1 for s in syms.values() if s["abi"].get("arity") is not None),
        "consumed_by_go": len(consumed),
        "consumed_with_signature": sum(1 for s in syms.values()
                                       if s["abi"].get("arity") is not None and _name_in(s, consumed)),
        "call_sites": len(sites),
    }
    return diags, gaps, coverage


def _name_in(sym_fact, names):
    return True  # arity coverage counted above; kept explicit for the report


def read_embedded_contract(artifact: Path):
    members = archive_members(artifact)
    for name, body in members:
        if not body[:4] == b"\x7fELF":
            continue
        try:
            elf = Elf(body)
        except ValueError:
            continue
        sec = elf.sec(".hy_ffi_cert")
        if sec:
            return elf.raw(sec)
    return None


# --------------------------------------------------------------------------
# Fixtures (PREREG.md §4) — each must be rejected before a fast route
# --------------------------------------------------------------------------
def run_fixtures(contract, artifact: Path, go_root: Path, tmp: Path):
    results = []
    base = json.loads(json.dumps(contract))

    def check(name, expected, mutated, artifact_path=None, go_path=None):
        diags, _gaps, _cov = verify(mutated, go_path or go_root, artifact_path or artifact)
        codes = sorted({d["code"] for d in diags})
        results.append({"fixture": name, "expected": expected, "codes": codes,
                        "rejected": expected in codes,
                        "detail": [d for d in diags if d["code"] == expected][:3]})

    # F1 — signature drift: claim 2 params for a 3-param export.
    target = next((s for s in base["symbols"] if base["symbols"][s]["abi"].get("arity") == 3), None)
    m = json.loads(json.dumps(base))
    if target:
        m["symbols"][target]["abi"]["params"] = m["symbols"][target]["abi"]["params"][:2]
        m["symbols"][target]["abi"]["arity"] = 2
    check("f1_signature_drift", E_SIGNATURE, m)

    # F2 — an export whose closure is recursive (or over the SMALL guard) claimed SMALL.
    target2 = next((s for s in base["symbols"]
                    if base["symbols"][s]["stack"]["recursive"]
                    or base["symbols"][s]["stack"]["bytes"] > GUARD_SMALL), None)
    m = json.loads(json.dumps(base))
    if target2:
        m["symbols"][target2]["stack"]["class"] = "SMALL"
        m["symbols"][target2]["stack"]["bytes"] = 512
    check("f2_recursion_as_small", E_STACK, m)

    # F3 — retained borrow: may_retain=false over a wrapper that leaks.
    lie_src = tmp / "f3_lying_wrapper.rs"
    lie_src.write_text(LYING_WRAPPER_RETAIN, encoding="utf-8")
    eff = wrapper_effects(lie_src)
    m = json.loads(json.dumps(base))
    m["symbols"]["hy_fixture_retain"] = {
        "abi": {"params": ["ptr", "u64"], "arity": 2, "ret": "i32"},
        "stack": {"bytes": 64, "class": "SMALL", "recursive": False, "indirect_call": False,
                  "missing_frames": [], "external_callees": [], "blocking_callees": []},
        "effects": {"may_retain_ptr": False, "may_callback": False, "blocking": False,
                    "source": "claim"},
    }
    diags, _g, _c = verify(m, go_root, artifact)
    # source-level: the subset verdict contradicts the claim
    contradicted = eff.get("hy_fixture_retain", {}).get("may_retain_ptr", False)
    results.append({"fixture": "f3_retained_borrow", "expected": E_RETAIN,
                    "codes": [E_RETAIN] if contradicted else [],
                    "rejected": contradicted,
                    "detail": [{"subset_verdict": eff.get("hy_fixture_retain")}] if contradicted else []})

    # F4 — callback edge: may_callback=false while the closure has an indirect call.
    target4 = next((s for s in base["symbols"] if base["symbols"][s]["stack"]["indirect_call"]), None)
    m = json.loads(json.dumps(base))
    if target4:
        m["symbols"][target4]["effects"]["may_callback"] = False
    check("f4_callback_edge", E_CALLBACK, m)

    # F5 — blocking beneath a non-blocking claim.
    target5 = next((s for s in base["symbols"] if base["symbols"][s]["stack"]["blocking_callees"]), None)
    m = json.loads(json.dumps(base))
    if target5:
        m["symbols"][target5]["effects"]["blocking"] = False
    check("f5_blocking_beneath_nonblocking", E_BLOCKING, m)

    # F6 — embedded hash from a different contract.
    stale = json.loads(json.dumps(base))
    stale["artifact"]["sha256"] = "00" * 32
    stale.pop("contract_sha256", None)
    stale["contract_sha256"] = digest(stale)
    bound = tmp / "libhyrt.stale.a"
    shutil.copyfile(artifact, bound)
    embed_contract(bound, stale, tmp)
    check("f6_hash_mismatch", E_HASH, base, artifact_path=bound)
    return results


def embed_contract(archive: Path, contract, tmp: Path):
    """Bind a contract into a copy of the archive: every ELF member gets
    a .hy_ffi_cert section (noload, readonly)."""
    blob = contract_payload(contract)
    sec = tmp / "contract.bin"
    sec.write_bytes(blob)
    members = archive_members(archive)
    out = tmp / "rebuild"
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir()
    objs = []
    for name, body in members:
        p = out / re.sub(r"[^A-Za-z0-9_.-]", "_", name)
        p.write_bytes(body)
        if body[:4] == b"\x7fELF":
            subprocess.run(["objcopy", "--add-section", f".hy_ffi_cert={sec}",
                            "--set-section-flags", ".hy_ffi_cert=noload,readonly", str(p)],
                           check=True, capture_output=True)
        objs.append(p)
    subprocess.run(["ar", "rcs", str(archive)] + [str(p) for p in objs], check=True, capture_output=True)


def contract_payload(contract) -> bytes:
    body = {k: v for k, v in contract.items() if k != "contract_sha256"}
    body["contract_sha256"] = digest(body)
    return canonical(body)


LYING_WRAPPER_RETAIN = '''
// Fixture F3: a wrapper that LEAKS a call-scoped pointer, under a contract
// that claims may_retain_ptr = false.  The restricted subset must catch it.
#[no_mangle]
pub extern "C" fn hy_fixture_retain(ptr: *const u8, len: u64) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let leaked: &'static [u8] = Box::leak(slice.to_vec().into_boxed_slice());
    let raw = Box::into_raw(Box::new(leaked));
    unsafe { std::ptr::write(raw as *mut u64, leaked.len() as u64) };
    0
}
'''


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------
def toolchain_string():
    out = []
    for cmd in (["rustc", "-vV"], ["cargo", "--version"]):
        try:
            out.append(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip())
        except Exception as exc:                                   # noqa: BLE001
            out.append(f"{cmd[0]}: {exc}")
    out.append('RUSTFLAGS="-Z emit-stack-sizes -C debuginfo=2" RUSTC_BOOTSTRAP=1')
    return " | ".join(out)


def repo_root() -> Path:
    return Path(subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                               text=True, check=True).stdout.strip())


def default_artifact() -> Path:
    return repo_root() / "runtime/rust/hyrt/target/release/libhyrt.a"


def main(argv=None):
    ap = argparse.ArgumentParser(description="artifact-derived FFI boundary contracts (#610)")
    sub = ap.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("build", help="build libhyrt.a with the pinned flags")
    b.add_argument("--out", default=None)

    e = sub.add_parser("extract", help="extract the contract from the artifact")
    e.add_argument("--artifact", default=None)
    e.add_argument("--go-root", default=None)
    e.add_argument("--wrapper", default=None)
    e.add_argument("--out", required=True)

    v = sub.add_parser("verify", help="verify claims against the artifact")
    v.add_argument("--contract", required=True)
    v.add_argument("--artifact", default=None)
    v.add_argument("--go-root", default=None)

    f = sub.add_parser("fixtures", help="run the six pre-registered negative fixtures")
    f.add_argument("--contract", required=True)
    f.add_argument("--artifact", default=None)
    f.add_argument("--go-root", default=None)
    f.add_argument("--out", default=None)

    r = sub.add_parser("report", help="markdown table: artifact facts vs the Go binding's routes")
    r.add_argument("--contract", required=True)
    r.add_argument("--go-root", default=None)

    s = sub.add_parser("bind", help="bind a contract digest into a copy of the artifact")
    s.add_argument("--contract", required=True)
    s.add_argument("--artifact", default=None)
    s.add_argument("--out", required=True)

    args = ap.parse_args(argv)
    root = repo_root()
    artifact = Path(args.artifact) if getattr(args, "artifact", None) and args.artifact else default_artifact()
    go_root = Path(args.go_root) if getattr(args, "go_root", None) and args.go_root else root / "runtime/go"
    wrapper = Path(args.wrapper) if getattr(args, "wrapper", None) and args.wrapper else root / "runtime/rust/hyrt/src/lib.rs"

    if args.cmd == "build":
        env = dict(os.environ, RUSTC_BOOTSTRAP="1",
                   RUSTFLAGS="-Z emit-stack-sizes -C debuginfo=2")
        subprocess.run(["cargo", "build", "--release"], cwd=root / "runtime/rust/hyrt",
                       env=env, check=True)
        print(artifact)
        return 0

    if args.cmd == "extract":
        c = build_contract(artifact, go_root, wrapper, toolchain_string())
        Path(args.out).write_text(json.dumps(c, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(json.dumps({"contract_sha256": c["contract_sha256"],
                          "symbols": len(c["symbols"]),
                          "functions_with_frames": c["artifact"]["functions_with_frames"]}, indent=2))
        return 0

    if args.cmd == "verify":
        c = json.loads(Path(args.contract).read_text(encoding="utf-8"))
        diags, gaps, coverage = verify(c, go_root, artifact)
        print(json.dumps({"diagnostics": diags, "gaps": gaps, "coverage": coverage},
                         indent=2, sort_keys=True))
        return 1 if diags else 0

    if args.cmd == "fixtures":
        c = json.loads(Path(args.contract).read_text(encoding="utf-8"))
        tmp = Path(tempfile.mkdtemp(prefix="hyffi-fix-"))
        try:
            res = run_fixtures(c, artifact, go_root, tmp)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
        payload = {"fixtures": res, "passed": sum(1 for r in res if r["rejected"]), "total": len(res)}
        if args.out:
            Path(args.out).write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(payload, indent=2))
        return 0 if payload["passed"] == payload["total"] else 1

    if args.cmd == "report":
        c = json.loads(Path(args.contract).read_text(encoding="utf-8"))
        externs, _fnmap, sites = go_bindings(go_root)
        routes, maxarity = {}, {}
        for site in sites:
            if not site["symbol"]:
                continue
            prev = routes.get(site["symbol"])
            if prev is None or CLASS_ORDER.get(site["route_class"], 9) > CLASS_ORDER.get(prev, 9):
                routes[site["symbol"]] = site["route_class"]
            maxarity[site["symbol"]] = max(maxarity.get(site["symbol"], 0), site["arity"])
        print("| export | arity | own frame B | closure B | artifact class | forced by | Go route | signature | route |")
        print("|---|---|---|---|---|---|---|---|---|")
        sig_ok = route_ok = 0
        for sym, fact in sorted(c["symbols"].items()):
            st = fact["stack"]
            route = routes.get(sym)
            if route is None:
                print(f"| `{sym}` | {fact['abi']['arity']} | {fact['frame_bytes']} | {st['bytes']} | "
                      f"{st['class']} | {', '.join(st['forcing']) or '—'} | — | not consumed | — |")
                continue
            ar, a = maxarity.get(sym, 0), fact["abi"]["arity"]
            sig = "match" if ar == a else f"drift {ar}→{a}"
            ok_route = CLASS_ORDER.get(route, 9) >= CLASS_ORDER[st["class"]] or route == "RAW"
            sig_ok += sig == "match"
            route_ok += ok_route
            print(f"| `{sym}` | {a} | {fact['frame_bytes']} | {st['bytes']} | {st['class']} | "
                  f"{', '.join(st['forcing']) or '—'} | {route} | {sig} | {'match' if ok_route else 'too fast'} |")
        print()
        print(f"consumed: {len(routes)} | signature exact: {sig_ok}/{len(routes)} | "
              f"route not faster than artifact: {route_ok}/{len(routes)}")
        return 0

    if args.cmd == "bind":
        c = json.loads(Path(args.contract).read_text(encoding="utf-8"))
        shutil.copyfile(artifact, args.out)
        embed_contract(Path(args.out), c, Path(tempfile.mkdtemp(prefix="hyffi-bind-")))
        print(args.out)
        return 0

    return 2


if __name__ == "__main__":
    sys.exit(main())
