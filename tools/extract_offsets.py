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


def _parse_dwarf_name(raw):
    if raw.startswith("(") and "):" in raw:
        return raw.rsplit("):", 1)[1].strip()
    return raw


def _parse_member_offset(line):
    uconst = re.search(r"DW_OP_plus_uconst:\s*(\d+)", line)
    if uconst is not None:
        return int(uconst.group(1))
    direct = re.search(r"DW_AT_data_member_location\s*:\s*(\d+)\s*$", line)
    if direct is not None:
        return int(direct.group(1))
    return None


def main(binpath):
    proc = subprocess.run(["readelf", "--debug-dump=info", binpath],
                          capture_output=True, text=True, timeout=600)
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
        h = re.match(r"\s*<(\d+)><[0-9a-f]+>: Abbrev Number: (\d+)(?: \((DW_TAG_\w+)\))?", line)
        if h:
            if h.group(2) == "0" or h.group(3) is None:
                dies.append({"depth": int(h.group(1)), "tag": "NULL"})
                cur = None
            else:
                cur = {"depth": int(h.group(1)), "tag": h.group(3)}
                dies.append(cur)
            continue
        if cur is None:
            continue
        nm = re.search(r"DW_AT_name\s*:\s*(.+?)\s*$", line)
        sz = re.search(r"DW_AT_byte_size\s*:\s*(\d+)", line)
        if nm is not None:
            cur["name"] = _parse_dwarf_name(nm.group(1))
        if "DW_AT_data_member_location" in line:
            off_val = _parse_member_offset(line)
            if off_val is not None:
                cur["off"] = off_val
        if sz is not None:
            cur["size"] = int(sz.group(1))

    def members_of(struct_name):
        fallback = (None, None)
        for i, d in enumerate(dies):
            if d["tag"] == "DW_TAG_structure_type" and d.get("name") == struct_name:
                base = d["depth"]
                mem = {}
                for e in dies[i + 1:]:
                    if e["depth"] <= base:
                        break
                    if e["tag"] == "DW_TAG_member" and "name" in e:
                        mem[e["name"]] = e.get("off")
                if mem:
                    return d.get("size"), mem
                fallback = (d.get("size"), mem)
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
