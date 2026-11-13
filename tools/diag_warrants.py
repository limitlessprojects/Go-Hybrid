#!/usr/bin/env python3
"""
tools/diag_warrants.py
Diagnostic Warrant Registry audit tool for Go-Hybrid.

Scans the diagnostic registry, Rust compiler crate emitters, the checker corpus,
and tests/diag-warrants/<CODE>/ to ensure every diagnostic carries a black-box
two-sided witness (reject.hy + near_miss.hy) or is classified under a triaged baseline.
"""

import os
import sys
import json
import re
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTRY_PATH = ROOT / "compiler" / "crates" / "hy-diag" / "data" / "diagnostics.json"
CRATES_DIR = ROOT / "compiler" / "crates"
CORPUS_DIR = ROOT / "tests" / "checker-corpus"
WARRANTS_DIR = ROOT / "tests" / "diag-warrants"

DIAG_PATTERN = re.compile(r'\bHYB-E\d{4}\b')
EMIT_PATTERN = re.compile(r'"(HYB-E\d{4})"')
CORPUS_HEADER_PATTERN = re.compile(r'//\s*EXPECTED-DIAGNOSTIC:\s*(HYB-E\d{4})')

def load_registry():
    if not REGISTRY_PATH.exists():
        return {}
    with open(REGISTRY_PATH, "r", encoding="utf-8") as f:
        data = json.load(f)
    codes = {}
    for entry in data.get("codes", []):
        c = entry.get("code")
        if c:
            codes[c] = entry
    return codes

def scan_emitted_codes():
    emitted = set()
    for rs_file in CRATES_DIR.rglob("*.rs"):
        # Skip unit test files and benches to only scan actual compiler emit sites
        if "/tests/" in rs_file.as_posix() or "/benches/" in rs_file.as_posix() or rs_file.name.endswith("_test.rs"):
            continue
        try:
            content = rs_file.read_text(encoding="utf-8", errors="ignore")
            for m in EMIT_PATTERN.finditer(content):
                emitted.add(m.group(1))
        except Exception:
            pass
    return sorted(list(emitted))

def scan_corpus_pins():
    pinned = {}
    if not CORPUS_DIR.exists():
        return pinned
    for hy_file in CORPUS_DIR.rglob("*.hy"):
        try:
            content = hy_file.read_text(encoding="utf-8", errors="ignore")
            m = CORPUS_HEADER_PATTERN.search(content)
            if m:
                pinned[hy_file.name] = m.group(1)
        except Exception:
            pass
    return pinned

def scan_warrants():
    warrants = {}
    if not WARRANTS_DIR.exists():
        return warrants
    for warrant_dir in WARRANTS_DIR.iterdir():
        if warrant_dir.is_dir() and DIAG_PATTERN.match(warrant_dir.name):
            code = warrant_dir.name
            reject_file = warrant_dir / "reject.hy"
            near_miss_file = warrant_dir / "near_miss.hy"
            toml_file = warrant_dir / "warrant.toml"
            if reject_file.exists() and near_miss_file.exists() and toml_file.exists():
                warrants[code] = {
                    "dir": str(warrant_dir.relative_to(ROOT)),
                    "reject": str(reject_file.relative_to(ROOT)),
                    "near_miss": str(near_miss_file.relative_to(ROOT)),
                    "warrant_toml": str(toml_file.relative_to(ROOT))
                }
    return warrants

def run_audit(baseline_file=None, fail_on_orphan=False):
    registry = load_registry()
    emitted = set(scan_emitted_codes())
    corpus_pins = scan_corpus_pins()
    warrants = scan_warrants()

    registry_codes = set(registry.keys())
    pinned_codes = set(corpus_pins.values())
    warranted_codes = set(warrants.keys())

    emitted_unregistered = sorted(list(emitted - registry_codes))
    registered_unemitted = sorted(list(registry_codes - emitted))
    
    # An error code is considered witnessed if it has an explicit black-box warrant or exact corpus pin
    witnessed = sorted(list(warranted_codes | pinned_codes))
    unwitnessed = sorted(list(registry_codes - set(witnessed)))

    baseline_orphans = set()
    if baseline_file and os.path.exists(baseline_file):
        with open(baseline_file, "r", encoding="utf-8") as f:
            b_data = json.load(f)
            baseline_orphans = set(b_data.get("unwitnessed", b_data.get("grandfathered_unwitnessed", [])))

    new_orphans = sorted(list(set(unwitnessed) - baseline_orphans))

    report = {
        "summary": {
            "total_registered_codes": len(registry_codes),
            "total_emitted_codes": len(emitted),
            "warranted_codes_count": len(warranted_codes),
            "corpus_pinned_count": len(pinned_codes),
            "witnessed_codes_count": len(witnessed),
            "unwitnessed_codes_count": len(unwitnessed),
            "emitted_unregistered_count": len(emitted_unregistered),
            "registered_unemitted_count": len(registered_unemitted),
            "new_orphans_count": len(new_orphans)
        },
        "warrants": warrants,
        "emitted_unregistered": emitted_unregistered,
        "registered_unemitted": registered_unemitted,
        "unwitnessed": unwitnessed,
        "new_orphans": new_orphans
    }

    status_code = 0
    if fail_on_orphan and new_orphans:
        status_code = 2

    return report, status_code

def main():
    parser = argparse.ArgumentParser(description="Go-Hybrid Diagnostic Warrant Registry Audit")
    subparsers = parser.add_subparsers(dest="command")

    audit_parser = subparsers.add_parser("audit", help="Audit diagnostic warrants")
    audit_parser.add_argument("--json", action="store_true", help="Output JSON report")
    audit_parser.add_argument("--baseline", type=str, help="Grandfather baseline JSON file")
    audit_parser.add_argument("--fail-on-orphan", action="store_true", help="Exit code 2 on new unwitnessed codes")

    args = parser.parse_args()

    if args.command == "audit":
        report, code = run_audit(baseline_file=args.baseline, fail_on_orphan=args.fail_on_orphan)
        if args.json:
            print(json.dumps(report, indent=2))
        else:
            s = report["summary"]
            print("=" * 60)
            print("GO-HYBRID DIAGNOSTIC WARRANT AUDIT REPORT")
            print("=" * 60)
            print(f"Total Registered Codes:    {s['total_registered_codes']}")
            print(f"Total Emitted in Rust:     {s['total_emitted_codes']}")
            print(f"Warranted Codes (Pairs):   {s['warranted_codes_count']}")
            print(f"Corpus Pinned Codes:       {s['corpus_pinned_count']}")
            print(f"Total Witnessed Codes:     {s['witnessed_codes_count']}")
            print(f"Unwitnessed (Orphans):     {s['unwitnessed_codes_count']}")
            print(f"Emitted Unregistered:      {s['emitted_unregistered_count']}")
            print(f"Registered Unemitted:      {s['registered_unemitted_count']}")
            print(f"New Orphans (vs Baseline): {s['new_orphans_count']}")
            print("=" * 60)
            if report["emitted_unregistered"]:
                print(f"[!] Emitted but not in diagnostics.json: {report['emitted_unregistered']}")
            if report["warrants"]:
                print(f"[+] Active Warrants ({len(report['warrants'])}): {sorted(list(report['warrants'].keys()))}")
        sys.exit(code)
    else:
        parser.print_help()

if __name__ == "__main__":
    main()
