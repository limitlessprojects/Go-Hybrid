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



def parse_uint(text):
    """A DWARF unsigned attribute value, or None when it is not one.

    readelf emits decimal (`16`), hexadecimal (`0x10`), and
    `(DW_OP_plus_uconst: 48)` block forms. The previous digit-only match took the
    leading `0` of `0x10`, truncating `stackguard0` to 0, and accepted
    malformed values like `16_corrupt` by reading the prefix. Anchor and
    parse the whole value, or return None so the caller fails closed.
    """
    if text is None:
        return None
    val = text.strip()
    block = re.fullmatch(r"\(DW_OP_plus_uconst:\s*([^)]+?)\s*\)", val)
    if block is not None:
        val = block.group(1).strip()
    if re.fullmatch(r"0[xX][0-9a-fA-F]+", val):
        return int(val, 16)
    if re.fullmatch(r"\d+", val):
        return int(val, 10)
    return None


def main(binpath):
    proc = subprocess.run(["readelf", "--debug-dump=info", binpath],
                          capture_output=True, text=True, encoding="utf-8",
                          errors="replace", timeout=600)
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
        off = re.search(r"DW_AT_data_member_location\s*:\s*(.+?)\s*$", line)
        sz = re.search(r"DW_AT_byte_size\s*:\s*(.+?)\s*$", line)
        dec = re.search(r"DW_AT_declaration\s*:\s*(\d+)", line)
        if nm is not None:
            cur["name"] = nm.group(1)
        if dec is not None:
            cur["declaration"] = dec.group(1) != "0"
        if off is not None:
            # A member offset that cannot be read is not "0": record the raw
            # text so the member is reported unusable instead of silently
            # landing at the wrong field. `None` means "present but unknown".
            cur["off"] = parse_uint(off.group(1))
        if sz is not None:
            cur["raw_size"] = sz.group(1)
            cur["size"] = parse_uint(sz.group(1))

    def members_of(struct_name):
        # A compilation unit may emit a forward declaration (`DW_AT_declaration
        # : 1`, no members) before the full definition. Returning on the first
        # name match made the forwards win and the real layout unreachable.
        complete = None
        for i, d in enumerate(dies):
            if d["tag"] != "DW_TAG_structure_type" or d.get("name") != struct_name:
                continue
            if d.get("declaration"):
                continue
            base = d["depth"]
            mem = {}
            for e in dies[i + 1:]:
                if e["depth"] <= base:
                    break
                # Only DIRECT children lay out this struct. A nested
                # struct/union member that happens to share a field name used
                # to overwrite the outer offset (stack -> 999).
                if e["depth"] == base + 1 and e["tag"] == "DW_TAG_member" and "name" in e:
                    mem[e["name"]] = e.get("off")
            if mem:
                # A complete definition with members wins over an empty body.
                complete = (d.get("size"), mem)
                break
        if complete is None:
            return None, None
        return complete

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

    # A struct size the DWARF does not give is not "None": the trampoline
    # offsets are only meaningful against a known record layout, so this is a
    # hard failure rather than a line of output with `size=None`.
    if gsize is None or msize is None:
        bad = []
        if gsize is None:
            bad.append("runtime.g")
        if msize is None:
            bad.append("runtime.m")
        print(f"FAILED: missing or unparseable DW_AT_byte_size for "
              f"{' / '.join(bad)}", file=sys.stderr)
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
