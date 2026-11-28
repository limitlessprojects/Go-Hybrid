#!/usr/bin/env python3
"""Execute #1755's preregistered contract probes; retain real tool output."""

import argparse
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True,
                        help="new directory for commands, raw output and summary")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.update(GOTOOLCHAIN="local", GOWORK="off", CGO_ENABLED="0")
    records = []

    def run(name, command, cwd=ROOT, expected=0):
        result = subprocess.run(command, cwd=cwd, env=env, capture_output=True,
                                text=True, timeout=120)
        (output / (name + ".stdout.txt")).write_text(result.stdout, encoding="utf-8")
        (output / (name + ".stderr.txt")).write_text(result.stderr, encoding="utf-8")
        records.append({"name": name, "command": command, "cwd": str(cwd),
                        "exit_code": result.returncode})
        (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
        print(f"{name}: exit {result.returncode}", flush=True)
        if result.stdout and name != "symbols":
            print(result.stdout, end="", flush=True)
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr, flush=True)
        if expected is None:
            require(result.returncode != 0, name + " unexpectedly succeeded")
        else:
            require(result.returncode == expected, name + " failed")
        return result

    require(sys.platform == "linux" and platform.machine() in ("aarch64", "arm64"),
            "this audit requires native linux/arm64; it must not silently skip")
    go = shutil.which("go")
    require(go is not None, "Go is required")
    version = run("go-version", [go, "version"]).stdout.strip()
    require(version == "go version go1.26.9 linux/arm64", "requires pinned Go 1.26.9")
    goenv = json.loads(run("go-env", [go, "env", "-json", "GOOS", "GOARCH", "GOROOT"]).stdout)
    require(goenv["GOOS"] == "linux" and goenv["GOARCH"] == "arm64",
            "cross compilation is not an executable native control")

    # The bad object is never linked or executed. Match the real rejection,
    # rather than counting any failure (e.g. a missing include) as evidence.
    negative = run("external-abiinternal", [
        go, "tool", "asm", "-p", "w1755probe", "-I", goenv["GOROOT"] + "/pkg/include",
        "-o", str(output / "negative.o"), "testdata/abiinternal_arm64.s",
    ], expected=None)
    require("ABI selector only permitted when compiling runtime" in negative.stderr,
            "assembler failed for a reason other than the ABI selector restriction")

    binary = output / "control"
    run("build-control", [go, "build", "-trimpath", "-o", str(binary), "."], ROOT / "control")
    control = run("execute-control", [str(binary)])
    require(control.stdout.strip() == "ABI0 identity: 4 values OK; NOSPLIT chain: returned",
            "the native positive control did not complete")
    run("vet-control", [go, "vet", "."], ROOT / "control")
    symbols = run("symbols", [go, "tool", "nm", str(binary)]).stdout
    require(re.search(r"\bmain\.identity$", symbols, re.M) is not None and
            re.search(r"\bmain\.identity\.abi0$", symbols, re.M) is not None,
            "both ABIInternal wrapper and ABI0 body must be linked")
    wrapper = run("wrapper", [go, "tool", "objdump", "-s", "^main.identity$", str(binary)]).stdout
    require("CALL main.identity.abi0(SB)" in wrapper, "wrapper does not call ABI0 body")
    chain = run("chain", [go, "tool", "objdump", "-s", "^main.frame", str(binary)]).stdout
    for callee in ("frameMiddle", "frameLeaf"):
        require(re.search(r"CALL main\." + callee + r"(?:\.abi0)?\(SB\)", chain) is not None,
                "missing nested call: " + callee)
    require("runtime.morestack" not in chain, "NOSPLIT control unexpectedly calls morestack")
    frames = {}
    for match in re.finditer(r"TEXT main\.(frame(?:Root|Middle|Leaf))(?:\.abi0)?\(SB\)"
                             r"(.*?)(?=\nTEXT |\Z)", chain, re.S):
        allocation = re.search(r"MOVD\.W R30, -(\d+)\(RSP\)", match[2])
        require(allocation is not None, "missing observed frame allocation: " + match[1])
        frames[match[1]] = int(allocation[1])
    require(len(frames) == 3 and all(size >= 96 for size in frames.values()),
            "linked chain did not retain all three declared local frames")

    full = 0xFFFF
    child = full >> 1
    permissions = {
        "full": full, "shifted_child": child,
        "or_reunion": child | child, "sum_reunion": child + child,
        "overlapping_bits": child & child,
        "exact_fraction_reunion": str(Fraction(1, 2) + Fraction(1, 2)),
        "scope": "literal equal right-shift children; alternative splits are not disproved",
    }
    require(permissions["or_reunion"] != full and permissions["sum_reunion"] != full,
            "literal shifted halves unexpectedly reconstructed the full permission")
    require(Fraction(1, 2) + Fraction(1, 2) == 1, "exact fractional control failed")
    (output / "permissions.json").write_text(json.dumps(permissions, indent=2) + "\n")
    summary = {
        "go_version": version, "platform": "linux/arm64", "audit_passed": True,
        "external_abiinternal": "rejected by pinned assembler",
        "abi0_control": "native execution and vet passed; ABI wrapper observed",
        "chain": {"declared_local_frames_bytes": [96, 96, 96],
                  "local_bytes_lower_bound": 288, "proposed_path_bound": 192,
                  "observed_frame_allocations_bytes": frames,
                  "native_execution": "returned", "calls_morestack": False},
        "permissions": permissions,
        "not_measured": ["Rust kernels", "amd64", "H1 corpus parity", "H3 latency", "GC safety"],
        "inputs_sha256": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in sorted(ROOT.rglob("*"))
                          if p.is_file() and p.suffix in (".py", ".go", ".s")},
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
