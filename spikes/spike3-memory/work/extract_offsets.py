#!/usr/bin/env python3
"""Extract runtime.g / runtime.m field offsets from a Go binary's DWARF.

Usage: python3 extract_offsets.py <go-binary>
Output: hardcoded offsets for the g0-switch trampoline (PINNED go1.26.9).
"""
import re
import subprocess
import sys


_UINT = r"(?:0[xX][0-9a-fA-F]+|\d+)"


def parse_uint(line, attr):
    """Return (found, value) for a DW_AT_<attr> numeric attribute on `line`.

    Accepts decimal (16), hexadecimal (0x10) and a single
    `(DW_OP_plus_uconst: N)` block form, anchored to the end of the value so a
    malformed value such as `16_corrupt` is never parsed by its leading digits.
    A present-but-unparseable value yields (True, None) so callers fail closed.
    """
    m = re.search(rf"{attr}\s*:\s*(.*?)\s*$", line)
    if m is None:
        return False, None
    raw = m.group(1)
    for pat in (rf"({_UINT})", rf"(?:\d+\s+byte block:.*?)?\(DW_OP_plus_uconst:\s*({_UINT})\)"):
        v = re.fullmatch(pat, raw)
        if v is not None:
            tok = v.group(1)
            return True, int(tok, 16) if tok[:2].lower() == "0x" else int(tok)
    return True, None


def main(binpath):
    proc = subprocess.run(["readelf", "--debug-dump=info", binpath],
                          capture_output=True, text=True,
                          encoding="utf-8", errors="replace", timeout=600)
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
        off_seen, off = parse_uint(line, "DW_AT_data_member_location")
        sz_seen, sz = parse_uint(line, "DW_AT_byte_size")
        if nm is not None:
            cur["name"] = nm.group(1)
        if off_seen:
            cur["off"] = off
        if sz_seen:
            cur["size"] = sz
        if re.search(r"DW_AT_declaration\s*:\s*(?:1|0x1|yes)\b", line):
            cur["declaration"] = True

    def members_of(struct_name):
        for i, d in enumerate(dies):
            if (d["tag"] == "DW_TAG_structure_type" and d.get("name") == struct_name
                    and not d.get("declaration")):
                base = d["depth"]
                mem = {}
                for e in dies[i + 1:]:
                    if e["depth"] <= base:
                        break
                    if (e["depth"] == base + 1 and e["tag"] == "DW_TAG_member"
                            and "name" in e):
                        mem[e["name"]] = e.get("off")
                return d.get("size"), mem
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

    required_g = ("m", "stackguard0", "stack")
    required_m = ("g0", "curg", "id", "locks")
    missing = []
    for f in required_g:
        if g.get(f) is None:
            missing.append(f"runtime.g.{f}")
    for f in required_m:
        if m.get(f) is None:
            missing.append(f"runtime.m.{f}")

    if gsize is None:
        missing.append("runtime.g DW_AT_byte_size")
    if msize is None:
        missing.append("runtime.m DW_AT_byte_size")

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
