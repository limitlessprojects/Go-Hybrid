#!/usr/bin/env python3
"""Transitive STEXT call-graph + per-slot .opendefer @[noheap] certificate verifier."""
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

PKG = "hybrid.spike/noheap."
PINNED_GO_PREFIX = "go1.23."

# Tier 1: Pure zero-CALL assembly leaf primitives (verified via `go tool objdump`).
LEAF_MEM_RT = frozenset({
    "runtime.duffzero",
    "runtime.duffcopy",
    "runtime.memmove",
    "runtime.memclrNoHeapPointers",
    "runtime.memequal",
    "runtime.memequal8",
    "runtime.memequal16",
    "runtime.memequal32",
    "runtime.memequal64",
    "runtime.memequal128",
})

# Tier 2: Function prologue stack-segment growth (allocates from runtime.stackpool,
# never Go-heap mallocgc; excluded only if paired with @[nosplit]).
STACK_GROWTH_RT = frozenset({
    "runtime.morestack",
    "runtime.morestack_noctxt",
})

# Tier 3: Cold diverging unwind/trap stubs. Explicitly closed (never `panic*` wildcard,
# so `runtime.gopanic` and heap-boxing dynamic type assertions fail closed).
# Permitted only when `runtime.gorecover` is unreachable in the transitive cone.
COLD_UNWIND_TRAP_RT = frozenset({
    "runtime.deferreturn",
    "runtime.panicIndex",
    "runtime.panicIndexU",
    "runtime.panicSliceAlen",
    "runtime.panicSliceAlenU",
    "runtime.panicSliceAcap",
    "runtime.panicSliceAcapU",
    "runtime.panicSliceB",
    "runtime.panicSliceBU",
    "runtime.panicSlice3Alen",
    "runtime.panicSlice3AlenU",
    "runtime.panicSlice3Acap",
    "runtime.panicSlice3AcapU",
    "runtime.panicSlice3B",
    "runtime.panicSlice3BU",
    "runtime.panicSlice3C",
    "runtime.panicSlice3CU",
    "runtime.panicSliceConvert",
    "runtime.panicdivide",
    "runtime.panicshift",
    "runtime.panicoverflow",
    "runtime.panicfloat",
    "runtime.panicmem",
    "runtime.checkptrAlignment",
    "runtime.checkptrArithmetic",
})

STEXT_RE = re.compile(r"^(\S+)\s+STEXT\b")
INSN_LOC_RE = re.compile(r"^\s+0x[0-9a-f]+\s+\d+\s+\(([^()]+?):(\d+)(?:\[[^\]]*\])?\)\s+(.*)$")
OPENDEFER_RE = re.compile(r"^FUNCDATA\s+\$4,\s+(\S+)\.opendefer\(SB\)")
LEAQ_FUNCN_RE = re.compile(r"^LEAQ\s+(\S+\.func\d+)\(SB\),\s*(\w+)$")
MOVQ_REG_SLOT_RE = re.compile(r"^MOVQ\s+(\w+),\s*(\S+\.\.autotmp_\d+\+\d+\(SP\))$")
LEAQ_SLOT_REG_RE = re.compile(r"^LEAQ\s+(\S+\.\.autotmp_\d+\+\d+\(SP\)),\s*(\w+)$")
MOVQ_SLOT_DX_RE = re.compile(r"^MOVQ\s+(\S+\.\.autotmp_\d+\+\d+\(SP\)),\s*DX$")
MOVQ_DX_DEREF_RE = re.compile(r"^MOVQ\s+\(DX\),\s*(\w+)$")
CALL_INSN_RE = re.compile(r"^CALL\s+(\S+)$")
ESC_RE = re.compile(r"^(.+?):(\d+)(?::\d+)?:\s+(.*\b(?:escapes to heap|moved to heap:).*)$")


def detect_go_version(pkg_dir: Path) -> str:
    out = subprocess.run(
        ["go", "env", "GOVERSION"], cwd=pkg_dir, check=True, capture_output=True, text=True
    ).stdout.strip()
    return out


def parse_stext_stream(asm_text: str) -> dict:
    """Parse `-gcflags=-S` output with per-slot `.opendefer` R_CALLIND tracking
    and `//line` inlined call-site attribution."""
    symbols = {}
    cur = None
    reg_funcn = {}
    struct_slot_funcn = {}
    ptr_slot_funcn = {}
    reg_struct_slot = {}
    pending_dx_target = None
    pending_call_reg = None

    for raw in asm_text.splitlines():
        if sm := STEXT_RE.match(raw):
            cur = sm.group(1)
            symbols[cur] = {
                "entry_loc": None,
                "has_opendefer": False,
                "calls": [],
            }
            reg_funcn = {}
            struct_slot_funcn = {}
            ptr_slot_funcn = {}
            reg_struct_slot = {}
            pending_dx_target = None
            pending_call_reg = None
            continue
        if not cur:
            continue
        im = INSN_LOC_RE.match(raw)
        if not im:
            continue
        fname = Path(im.group(1)).name
        line = int(im.group(2))
        body = im.group(3).strip()
        info = symbols[cur]
        if body.startswith("TEXT\t") or body.startswith("TEXT "):
            info["entry_loc"] = (fname, line)
            info["last_outer_loc"] = (fname, line)
            continue
        if body.startswith(("PCDATA", "NOP")):
            continue
        if OPENDEFER_RE.match(body):
            info["has_opendefer"] = True
            continue
        if body.startswith("FUNCDATA"):
            continue

        # Track outer function `.hy` anchor vs inlined helper `.hy` lines:
        # In `kernels.hy`, helpers compiled via inlining sit at lines strictly
        # earlier than `entry_loc` (or in another `.hy` file).
        entry_f, entry_ln = info["entry_loc"] or (fname, line)
        if fname == entry_f and line >= entry_ln:
            info["last_outer_loc"] = (fname, line)

        # Dataflow of compiler-synthesized open-coded defer closure slots:
        #   LEAQ sym.funcN(SB), R1
        #   MOVQ R1, pkg..autotmp_A+off(SP)
        #   LEAQ pkg..autotmp_A+off(SP), R2
        #   MOVQ R2, pkg..autotmp_B+off2(SP)
        # Epilogue dispatch:
        #   MOVQ pkg..autotmp_B+off2(SP), DX
        #   MOVQ (DX), Rcall
        #   CALL Rcall
        if m := LEAQ_FUNCN_RE.match(body):
            reg = m.group(2)
            reg_funcn[reg] = m.group(1)
            reg_struct_slot.pop(reg, None)
            pending_dx_target = None
            pending_call_reg = None
            continue
        if m := MOVQ_REG_SLOT_RE.match(body):
            reg, slot = m.group(1), m.group(2)
            if reg in reg_struct_slot and reg_struct_slot[reg] in struct_slot_funcn:
                ptr_slot_funcn[slot] = struct_slot_funcn[reg_struct_slot[reg]]
            elif reg in reg_funcn:
                struct_slot_funcn[slot] = reg_funcn[reg]
            pending_dx_target = None
            pending_call_reg = None
            continue
        if m := LEAQ_SLOT_REG_RE.match(body):
            slot, reg = m.group(1), m.group(2)
            reg_funcn.pop(reg, None)
            if slot in struct_slot_funcn:
                reg_struct_slot[reg] = slot
            else:
                reg_struct_slot.pop(reg, None)
            pending_dx_target = None
            pending_call_reg = None
            continue
        if body.startswith("LEAQ\t") or body.startswith("LEAQ "):
            parts = body.rsplit(",", 1)
            if len(parts) == 2:
                dst_reg = parts[1].strip()
                reg_funcn.pop(dst_reg, None)
                reg_struct_slot.pop(dst_reg, None)
            pending_dx_target = None
            pending_call_reg = None
            continue
        if m := MOVQ_SLOT_DX_RE.match(body):
            slot = m.group(1)
            pending_dx_target = ptr_slot_funcn.get(slot)
            pending_call_reg = None
            continue
        if m := MOVQ_DX_DEREF_RE.match(body):
            pending_call_reg = m.group(1) if pending_dx_target else None
            continue

        if cm := CALL_INSN_RE.match(body):
            raw_t = cm.group(1)
            outer_loc = info.get("last_outer_loc", (fname, line))
            inlined_from = outer_loc if (fname, line) != outer_loc else None
            if raw_t.endswith("(SB)"):
                callee = raw_t[:-4]
                resolved_opendefer = None
            elif (
                info["has_opendefer"]
                and pending_dx_target
                and pending_call_reg == raw_t
            ):
                callee = f"<opendefer:{pending_dx_target}>"
                resolved_opendefer = pending_dx_target
            else:
                callee = f"<indirect:{raw_t}>"
                resolved_opendefer = None
            info["calls"].append({
                "file": fname,
                "line": line,
                "outer_loc": inlined_from,
                "callee": callee,
                "opendefer_target": resolved_opendefer,
            })
        pending_dx_target = None
        pending_call_reg = None

    return symbols


def certify_symbol(
    sym: str,
    symbols: dict,
    *,
    allow_stack_growth: bool = True,
) -> list[str]:
    """Audit transitive STEXT call cone of `sym`, returning sorted violation witnesses."""
    cone_callees = set()

    def collect_cone(cur: str, seen: set[str]) -> None:
        if cur in seen or cur not in symbols:
            return
        seen.add(cur)
        for edge in symbols[cur]["calls"]:
            if edge["opendefer_target"]:
                collect_cone(edge["opendefer_target"], seen)
            else:
                cone_callees.add(edge["callee"])
                if edge["callee"] in symbols:
                    collect_cone(edge["callee"], seen)

    collect_cone(sym, set())
    has_gorecover = "runtime.gorecover" in cone_callees

    def fmt_loc(edge: dict) -> str:
        loc = f"{edge['file']}:{edge['line']}"
        if edge["outer_loc"]:
            of, oln = edge["outer_loc"]
            return f"{of}:{oln} -> {loc} (inlined)"
        return loc

    def walk(cur: str, seen: set[str]) -> list[str]:
        if cur in seen:
            return []
        seen.add(cur)
        if cur not in symbols:
            return [f"missing STEXT body for {cur}"]
        bad = []
        for edge in symbols[cur]["calls"]:
            site = fmt_loc(edge)
            if edge["opendefer_target"]:
                target = edge["opendefer_target"]
                bad.extend(f"{site} -> {w}" for w in walk(target, seen))
                continue
            callee = edge["callee"]
            if callee in LEAF_MEM_RT:
                continue
            if allow_stack_growth and callee in STACK_GROWTH_RT:
                continue
            if not has_gorecover and callee in COLD_UNWIND_TRAP_RT:
                continue
            if callee.startswith("<indirect:"):
                bad.append(f"{site}: CALL {callee} [HYB-E7002 uncertified R_CALLIND]")
            elif callee in symbols:
                bad.extend(f"{site} -> {w}" for w in walk(callee, seen))
            else:
                bad.append(f"{site}: CALL {callee} [HYB-E7001]")
        return bad

    return sorted(set(walk(sym, set())))


def audit_linked_leaf_primitives(pkg_dir: Path) -> dict[str, list[str]]:
    """Disassemble linked Go runtime leaf primitives via `go tool objdump` and
    machine-verify that every symbol in `LEAF_MEM_RT` contains zero CALL instructions."""
    with tempfile.NamedTemporaryFile(suffix=".testbin", delete=False) as tf:
        bin_path = tf.name
    try:
        subprocess.run(
            ["go", "test", "-c", "-o", bin_path, "."],
            cwd=pkg_dir,
            check=True,
            capture_output=True,
        )
        report = {}
        for sym in sorted(LEAF_MEM_RT):
            out = subprocess.run(
                ["go", "tool", "objdump", "-s", f"^{re.escape(sym)}(\\.abi0)?$", bin_path],
                cwd=pkg_dir,
                check=True,
                capture_output=True,
                text=True,
            ).stdout
            calls = sorted(set(re.findall(r"\bCALL\s+(\S+)", out)))
            report[sym] = calls
        return report
    finally:
        if os.path.exists(bin_path):
            os.remove(bin_path)


def verify_package(
    pkg_dir: Path,
    targets: list[str],
    *,
    enforce_toolchain_pin: bool = True,
) -> dict:
    go_ver = detect_go_version(pkg_dir)
    if enforce_toolchain_pin and not go_ver.startswith(PINNED_GO_PREFIX):
        raise RuntimeError(
            f"HYB-E7003: unverified Go toolchain `{go_ver}` "
            f"(certificate allowlist is pinned to `{PINNED_GO_PREFIX}x`)"
        )

    t0 = time.perf_counter()
    subprocess.run(
        ["go", "build", "-o", "/dev/null", "-a", "."],
        cwd=pkg_dir,
        check=True,
        capture_output=True,
    )
    plain_ms = (time.perf_counter() - t0) * 1000.0

    m1 = subprocess.run(
        ["go", "build", "-o", "/dev/null", "-a", "-gcflags=-m=1", "."],
        cwd=pkg_dir,
        check=True,
        capture_output=True,
        text=True,
    )
    # Full rebuild with `-gcflags=all=-S` vs cached single-package `-gcflags=-S`:
    t_gate = time.perf_counter()
    asm = subprocess.run(
        ["go", "build", "-o", "/dev/null", "-a", "-gcflags=all=-S", "."],
        cwd=pkg_dir,
        check=True,
        capture_output=True,
        text=True,
    )
    gate_ms = (time.perf_counter() - t_gate) * 1000.0

    t_parse = time.perf_counter()
    asm_text = asm.stderr + "\n" + asm.stdout
    symbols = parse_stext_stream(asm_text)

    esc_lines = [
        (Path(m.group(1)).name, int(m.group(2)), m.group(3))
        for raw in (m1.stderr + "\n" + m1.stdout).splitlines()
        if (m := ESC_RE.match(raw.strip()))
    ]

    verdicts = {}
    for fn in targets:
        sym = PKG + fn
        asm_bad = certify_symbol(sym, symbols)
        info = symbols.get(sym, {"calls": [], "entry_loc": None})
        fn_lines = {e["line"] for e in info["calls"]}
        if info["entry_loc"]:
            fn_lines.add(info["entry_loc"][1])
        m_hits = [f"{f}:{ln}: {msg}" for f, ln, msg in esc_lines if ln in fn_lines]
        verdicts[fn] = {
            "m_only_noheap": len(m_hits) == 0,
            "certified_noheap": len(asm_bad) == 0,
            "asm_witnesses": asm_bad,
        }
    parse_ms = (time.perf_counter() - t_parse) * 1000.0

    # Measure cached `go build` vs cached `go build -gcflags=.:-S` + verifier parse:
    t_cached_plain = time.perf_counter()
    subprocess.run(
        ["go", "build", "-o", "/dev/null", "."],
        cwd=pkg_dir,
        check=True,
        capture_output=True,
    )
    cached_plain_ms = (time.perf_counter() - t_cached_plain) * 1000.0

    t_cached_cert = time.perf_counter()
    pkg_asm = subprocess.run(
        ["go", "build", "-o", "/dev/null", "-gcflags=-S", "."],
        cwd=pkg_dir,
        check=True,
        capture_output=True,
        text=True,
    )
    pkg_syms = parse_stext_stream(pkg_asm.stderr + "\n" + pkg_asm.stdout)
    for fn in targets:
        certify_symbol(PKG + fn, pkg_syms)
    cached_cert_ms = (time.perf_counter() - t_cached_cert) * 1000.0

    return {
        "go_version": go_ver,
        "plain_rebuild_ms": round(plain_ms, 2),
        "cached_plain_build_ms": round(cached_plain_ms, 2),
        "cached_certified_build_ms": round(cached_cert_ms, 2),
        "verifier_parse_only_ms": round(parse_ms, 2),
        "full_rebuild_asm_ms": round(gate_ms, 2),
        "verdicts": verdicts,
    }


TARGETS = [
    "K1FixedScratch",
    "K2CallerScratch",
    "K3Const32KiB",
    "K4OpenCodedDefer",
    "K5NonLeakingAny",
    "K6WindowChecksum",
    "K7InlinedCleanHelper",
    "F1InterfaceBox",
    "F2DynamicMake",
    "F3Const128KiB",
    "F4LoopDefer",
    "F5ReturnedClosure",
    "F6SliceAppend",
    "F7TransitiveCallee",
    "F8IndirectInterface",
    "F9OpenDeferPlusFuncVal",
    "F10OpenDeferAllocFuncN",
    "F11RecoverBoundsTrap",
    "F12InlinedHelperHyLine",
]

if __name__ == "__main__":
    work_dir = Path(__file__).resolve().parent
    res = verify_package(work_dir, TARGETS)
    leaf_audit = audit_linked_leaf_primitives(work_dir)
    res["leaf_mem_runtime_outgoing_calls"] = leaf_audit
    print(json.dumps(res, indent=2))
    for k, v in res["verdicts"].items():
        if v["certified_noheap"] != k.startswith("K"):
            sys.exit(f"MISMATCH on {k}: {v}")
    for rsym, calls in leaf_audit.items():
        if calls:
            sys.exit(f"LEAF_MEM_RT symbol {rsym} has unexpected outgoing calls: {calls}")
