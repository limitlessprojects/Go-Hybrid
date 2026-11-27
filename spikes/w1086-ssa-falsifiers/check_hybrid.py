"""Check the hand-written Hybrid witnesses using a real, supplied hyb binary."""

import argparse
import json
from pathlib import Path
import subprocess


def check(binary):
    root = Path(__file__).resolve().parent / "fixtures"
    results = {}
    for name, rejected in (("loop_reconsume", True), ("loop_fresh", False),
                           ("alias_write", True), ("clone_write", False)):
        proc = subprocess.run([str(Path(binary).resolve()), "check",
                               str(root / (name + ".hy")), "--json"],
                              capture_output=True, text=True, encoding="utf-8",
                              timeout=30)
        if proc.returncode != (1 if rejected else 0):
            raise AssertionError((name, proc.returncode, proc.stdout, proc.stderr))
        payload = json.loads(proc.stdout)
        verdict = payload["summary"]["verdict"]
        diagnostics = payload["diagnostics"]
        errors = [item["code"] for item in diagnostics
                  if item["severity"] == "error"]
        if rejected:
            if verdict != "reject" or "HYB-E3003" not in errors:
                raise AssertionError((name, payload))
        elif verdict != "accept" or errors:
            raise AssertionError((name, payload))
        results[name] = {"verdict": verdict, "error_codes": errors}
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", required=True)
    args = parser.parse_args()
    print(json.dumps(check(args.bin), indent=2, sort_keys=True))
