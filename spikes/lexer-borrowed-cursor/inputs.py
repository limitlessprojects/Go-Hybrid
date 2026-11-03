#!/usr/bin/env python3
"""Generate three bounded normal-source inputs in a NEW output directory."""
from pathlib import Path
import sys, json, hashlib
out = Path(sys.argv[1]); out.mkdir(exist_ok=False)
ordinary = "package bench\nfn main() {\n" + "".join(f"    let item_{i} := {i} + 1\n" for i in range(1000)) + "}\n"
comments = "package bench\n" + "// A normal generated comment about data processing and compiler input.\n" * 15000
unicode_text = "package bench\nfn main() {\n" + "".join(f"    let message_{i} := `catalogue λ🙂 é line {i}`\n" for i in range(1000)) + "}\n"
rows=[]
for name, text in [("ordinary.hy", ordinary), ("comments.hy", comments), ("unicode.hy", unicode_text)]:
    data=text.encode("utf-8"); (out/name).write_bytes(data)
    rows.append({"name":name,"bytes":len(data),"sha256":hashlib.sha256(data).hexdigest()})
print(json.dumps(rows, indent=2))
