#!/usr/bin/env python3
"""Extract runtime.g / runtime.m field offsets from a Go binary's DWARF.

Usage: python3 extract_offsets.py <go-binary>
Output: the trampoline offsets (PINNED go1.26.9).

Arch note (row 16): the offset that matters to the bridge is
g.stackguard0; run this against a binary built for the target arch —
  amd64: GOOS=linux GOARCH=amd64 go build ...   (call_amd64.s)
  arm64: GOOS=linux GOARCH=arm64 go build ...   (call_arm64.s)
DWARF member layout is arch-neutral to parse; the extractor reads
whatever binary it is handed. Both arches pin 16; the toolchain's own
arm64 stacksplit (cmd/internal/obj/arm64/obj7.go) independently emits
2*PtrSize for the same field.
"""
import re
import subprocess
import sys


def main(binpath):
    try:
        proc = subprocess.run(["readelf", "--debug-dump=info", binpath],
                              capture_output=True, text=True,
                              encoding="utf-8", errors="replace", timeout=600)
    except OSError as err:
        print(f"FAILED: readelf execution failed: {err}", file=sys.stderr)
        sys.exit(1)
    if proc.returncode != 0:
        err = proc.stderr.strip()
        msg = f"readelf failed with exit code {proc.returncode}"
        if err:
            msg += f": {err}"
        print(f"FAILED: {msg}", file=sys.stderr)
        sys.exit(proc.returncode or 1)

    out = proc.stdout

    dies = []
    cur = None
    for line in out.splitlines():
        h = re.match(r"\s*<(\d+)><[0-9a-fA-F]+>: Abbrev Number: \d+ \((DW_TAG_\w+)\)", line)
        if h:
            cur = {"depth": int(h.group(1)), "tag": h.group(2)}
            dies.append(cur)
            continue
        null_h = re.match(r"\s*<(\d+)><[0-9a-fA-F]+>: Abbrev Number: 0\b", line)
        if null_h:
            dies.append({"depth": int(null_h.group(1)), "tag": None})
            cur = None
            continue
        if cur is None:
            continue
        nm = re.search(r"DW_AT_name\s*:\s*(?:\([^)]*\)\s*:\s*)*(.*?)\s*$", line)
        sz = re.search(r"DW_AT_byte_size\s*:\s*(0x[0-9a-fA-F]+|\d+)", line)
        decl = re.search(r"DW_AT_declaration\s*:\s*(\S+)", line)
        if nm is not None and nm.group(1):
            cur["name"] = nm.group(1)
        if "DW_AT_data_member_location" in line:
            op_m = re.search(r"DW_OP_plus_uconst\s*:\s*(0x[0-9a-fA-F]+|\d+)", line)
            if op_m is not None:
                cur["off"] = int(op_m.group(1), 0)
            else:
                off = re.search(
                    r"DW_AT_data_member_location\s*:\s*(0x[0-9a-fA-F]+|\d+)(?!\s*byte\s+block)\b",
                    line,
                )
                if off is not None:
                    cur["off"] = int(off.group(1), 0)
        if sz is not None:
            cur["size"] = int(sz.group(1), 0)
        if decl is not None:
            cur["declaration"] = decl.group(1).strip().lower() not in ("0", "no", "false")

    def members_of(struct_name):
        fallback = (None, None)
        for i, d in enumerate(dies):
            if d["tag"] == "DW_TAG_structure_type" and d.get("name") == struct_name:
                if d.get("declaration"):
                    continue
                base = d["depth"]
                mem = {}
                for e in dies[i + 1:]:
                    if e["depth"] <= base:
                        break
                    if e["depth"] == base + 1 and e["tag"] == "DW_TAG_member" and "name" in e:
                        mem[e["name"]] = e.get("off")
                if not mem and d.get("size") is None:
                    continue
                if not mem:
                    fallback = (d.get("size"), mem)
                    continue
                return d.get("size"), mem
        return fallback

    gsize, g = members_of("runtime.g")
    msize, m = members_of("runtime.m")
    if g is None or m is None:
        missing_structs = []
        if g is None:
            missing_structs.append("runtime.g")
        if m is None:
            missing_structs.append("runtime.m")
        print(f"FAILED: {' / '.join(missing_structs)} not found in DWARF", file=sys.stderr)
        sys.exit(1)

    required_g = ("m", "stackguard0", "stack")
    required_m = ("g0", "curg", "id", "locks")
    missing = []
    for f in required_g:
        if g.get(f) is None:
            missing.append(f"runtime.g.{f}")
    for f in required_m:
        if m.get(f) is None:
            missing.append(f"runtime.m.{f}")

    if missing:
        print(f"FAILED: missing required field(s): {', '.join(missing)}", file=sys.stderr)
        sys.exit(1)

    print(f"# runtime.g size={gsize}  runtime.m size={msize}  (from {binpath})")
    for name, off in (
        ("g_m", g.get("m")),
        ("g_stackguard0", g.get("stackguard0")),
        ("g_stack_lo", g.get("stack")),
        ("g_stack_hi", None if g.get("stack") is None else g["stack"] + 8),
        ("m_g0", m.get("g0")),
        ("m_curg", m.get("curg")),
        ("m_id", m.get("id")),
        ("m locks", m.get("locks")),
    ):
        print(f"{name:16} = {off}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 extract_offsets.py <go-binary>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1])
