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


def parse_dwarf_uint(val_str):
    """Parse decimal, hexadecimal, or DW_OP_plus_uconst unsigned integer attributes."""
    if not val_str:
        return None
    val = val_str.strip()
    m = re.search(r"\(DW_OP_plus_uconst:\s*(0x[0-9a-fA-F]+|\d+)\)\s*$", val)
    if m:
        val = m.group(1)
    if re.fullmatch(r"0x[0-9a-fA-F]+|\d+", val):
        try:
            return int(val, 0)
        except ValueError:
            return None
    return None


def main(binpath):
    proc = subprocess.run(
        ["readelf", "--debug-dump=info", binpath],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=600,
    )
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
        h = re.match(r"\s*<(\d+)><[0-9a-f]+>: Abbrev Number: \d+ \((DW_TAG_\w+)\)", line)
        if h:
            cur = {"depth": int(h.group(1)), "tag": h.group(2)}
            dies.append(cur)
            continue
        if cur is None:
            continue
        nm = re.search(r"DW_AT_name\s*:\s*(.+?)\s*$", line)
        off_m = re.search(r"DW_AT_data_member_location\s*:\s*(.+?)\s*$", line)
        sz_m = re.search(r"DW_AT_byte_size\s*:\s*(.+?)\s*$", line)
        decl_m = re.search(r"DW_AT_declaration\s*:\s*(.+?)\s*$", line)
        if nm is not None:
            cur["name"] = nm.group(1).strip()
        if off_m is not None:
            parsed_off = parse_dwarf_uint(off_m.group(1))
            if parsed_off is not None:
                cur["off"] = parsed_off
        if sz_m is not None:
            parsed_sz = parse_dwarf_uint(sz_m.group(1))
            if parsed_sz is not None:
                cur["size"] = parsed_sz
        if decl_m is not None:
            decl_val = parse_dwarf_uint(decl_m.group(1))
            if decl_val == 1:
                cur["declaration"] = True

    def members_of(struct_name):
        candidates = []
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
                candidates.append((d.get("size"), mem))
        for size, mem in candidates:
            if mem:
                return size, mem
        if candidates:
            return candidates[0]
        return None, None

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

    if gsize is None or msize is None:
        missing_sizes = []
        if gsize is None:
            missing_sizes.append("runtime.g byte_size")
        if msize is None:
            missing_sizes.append("runtime.m byte_size")
        print(f"FAILED: missing required byte size: {', '.join(missing_sizes)}", file=sys.stderr)
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
