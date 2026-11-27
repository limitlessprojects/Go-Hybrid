#!/usr/bin/env python3
"""Reproduce the two production ports; never turn missing evidence into PASS."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
COMPILER_COMMIT = "82037c3da4f3b888eef2e21ee1cb5e347e74a0c4"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    with path.open("w", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(value, indent=2, sort_keys=True) + "\n")


def run(command, cwd, timeout=180):
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True,
                            encoding="utf-8", errors="replace", timeout=timeout)
    return {"exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr}


def check(hyb, source):
    result = run([hyb, "check", str(source), "--json", "--report-managed"], ROOT)
    if result["exit"] not in (0, 1):
        raise RuntimeError(f"checker infrastructure failure: exit {result['exit']}")
    output = json.loads(result["stdout"])
    summary = output["summary"]
    errors = [d for d in output["diagnostics"] if d["severity"] == "error"]
    expected = "accept" if result["exit"] == 0 else "reject"
    if summary["verdict"] != expected or summary["errors"] != len(errors):
        raise RuntimeError("checker exit, verdict and diagnostics disagree")
    if (expected == "accept") != (not errors):
        raise RuntimeError("checker verdict does not match its errors")
    return output


def diff_counts(before, after):
    result = run(["git", "diff", "--no-index", "--numstat", "--",
                  str(Path(before).resolve()), str(Path(after).resolve())], ROOT)
    if result["exit"] not in (0, 1):
        raise RuntimeError("git diff failed: " + result["stderr"])
    added = deleted = 0
    for line in result["stdout"].splitlines():
        a, d, _ = line.split("\t", 2)
        if not a.isdigit() or not d.isdigit():
            raise RuntimeError("binary input cannot be counted as source lines")
        added += int(a)
        deleted += int(d)
    return added, deleted


def lines(path):
    return len(Path(path).read_text(encoding="utf-8").splitlines())


def verify_inputs():
    manifest = json.loads((ROOT / "inputs.json").read_text(encoding="utf-8"))
    for name, expected in manifest["sha256"].items():
        path = (ROOT / name).resolve()
        if not path.is_relative_to(ROOT) or digest(path) != expected:
            raise RuntimeError("frozen input missing or changed: " + name)
    return manifest


def go_test(go, directory, packages, count):
    result = run([go, "test", "-race", "-json", f"-count={count}",
                  "-timeout=120s", *packages], directory, timeout=300)
    events = []
    for line in result["stdout"].splitlines():
        events.append(json.loads(line))
    failures = [x for x in events if x.get("Action") == "fail"]
    passed_packages = {x["Package"] for x in events
                       if x.get("Action") == "pass" and "Test" not in x}
    passed_tests = Counter(x["Package"] + "::" + x["Test"] for x in events
                           if x.get("Action") == "pass" and "Test" in x)
    if result["exit"] or failures or len(passed_packages) != len(packages) or not passed_tests:
        raise RuntimeError("upstream behavioral tests failed: " + result["stdout"] + result["stderr"])
    return result, {"packages": sorted(passed_packages),
                    "passed_test_events": sum(passed_tests.values()),
                    "unique_test_names": len(passed_tests),
                    "test_events_by_name": dict(sorted(passed_tests.items())), "count": count}


def classify(checker, tests_passed, escaped):
    if checker["summary"]["verdict"] != "accept" or not tests_passed:
        return "NOT_VERIFIED"
    managed = checker["summary"].get("managed", {}).get("managed_blocks")
    if managed is None:
        raise RuntimeError("missing managed evidence")
    if managed or escaped:
        return "ESCAPED"
    # A later reviewer must distinguish CLEAN from REDESIGN using edit history.
    return "REVIEW_REQUIRED"


def sweep(hyb, go, output):
    goroot = run([go, "env", "GOROOT"], output)
    module = run([go, "mod", "download", "-json", "golang.org/x/sync@v0.18.0"], output)
    if goroot["exit"] or module["exit"]:
        raise RuntimeError("cannot locate pinned sweep sources")
    module_info = json.loads(module["stdout"])
    manifest = verify_inputs()
    if module_info["Sum"] != manifest["upstream"]["sum"]:
        raise RuntimeError("upstream module checksum differs")
    origins = json.loads((ROOT / "sweep-origins.json").read_text(encoding="utf-8"))
    histogram = Counter()
    records = []
    for name, origin in sorted(origins.items()):
        base = Path(module_info["Dir"]) if origin["project"] == "golang.org/x/sync" else Path(goroot["stdout"].strip())
        source = base / origin["path"]
        if digest(source) != origin["sha256"]:
            raise RuntimeError("pinned sweep source differs: " + name)
        value = check(hyb, source)
        codes = Counter(d["code"] for d in value["diagnostics"] if d["severity"] == "error")
        histogram.update(codes)
        records.append({"file": name, "sha256": digest(source),
                        "verdict": value["summary"]["verdict"], "errors": dict(codes)})
    save(output / "sweep.json", {"unit": "untranslated source files, not complete programs",
                                "histogram": dict(histogram), "records": records,
                                "ownership_inference": "none: lexical/parse rejection is reported separately"})
    return len(records), dict(histogram)


def reproduce(args):
    manifest = verify_inputs()
    parent = Path(args.out).resolve()
    if parent.is_relative_to(ROOT):
        raise RuntimeError("--out must be outside the committed study")
    parent.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="census-", dir=parent))
    go_version = run([args.go, "version"], ROOT)
    rust_version = run(["rustc", "--version"], ROOT)
    if go_version["exit"] or "go1.26.9 " not in go_version["stdout"]:
        raise RuntimeError("Go 1.26.9 required")
    if rust_version["exit"] or not rust_version["stdout"].startswith("rustc 1.99.0 "):
        raise RuntimeError("Rust 1.99.0 required")
    if os.environ.get("HYB_ALLOW_TOOLCHAIN_DRIFT"):
        raise RuntimeError("toolchain drift override must be unset")
    save(output / "environment.json", {"compiler_source_commit": COMPILER_COMMIT,
                                      "compiler_binary_sha256": digest(args.hyb),
                                      "go": go_version["stdout"].strip(),
                                      "rust": rust_version["stdout"].strip(),
                                      "platform": sys.platform})
    baseline = output / "baseline"
    candidate = output / "candidate"
    for module in (baseline, candidate):
        module.mkdir()
        (module / "go.mod").write_text("module golang.org/x/sync\n\ngo 1.26.9\n", encoding="utf-8")
    for package in ("errgroup", "singleflight"):
        shutil.copytree(ROOT / "upstream" / package, baseline / package)
        build = run([args.hyb, "build", str(Path("ports") / package / "accepted.hy"),
                     "--package", package, "--out-dir", str(candidate / package),
                     "--binding-prefix", "golang.org/x/sync"], ROOT)
        save(output / f"{package}-build.json", build)
        if build["exit"]:
            raise RuntimeError(f"{package} package emission failed")
        shutil.copyfile(ROOT / "ports" / package / "api.go", candidate / package / "api.go")
        for test in (ROOT / "upstream" / package).glob("*_test.go"):
            shutil.copyfile(test, candidate / package / test.name)
    packages = ["./errgroup", "./singleflight"]
    baseline_log, baseline_summary = go_test(args.go, baseline, packages, args.count)
    candidate_log, candidate_summary = go_test(args.go, candidate, packages, args.count)
    save(output / "baseline-tests.json", baseline_log)
    save(output / "candidate-tests.json", candidate_log)
    if baseline_summary != candidate_summary:
        raise RuntimeError("baseline and candidate test inventories differ")
    rows = []
    for package in ("errgroup", "singleflight"):
        source = ROOT / "ports" / package / "accepted.hy"
        checker = check(args.hyb, source.relative_to(ROOT))
        save(output / f"{package}-checker.json", checker)
        if checker["summary"]["verdict"] != "accept":
            raise RuntimeError(package + " is not accepted")
        # Remove only managed headers. Braces remain as ordinary if blocks.
        mutation = re.sub(r'managed\([^\n]+\)\s*\{', 'if true {', source.read_text(encoding="utf-8"))
        mutant = output / (package + "-without-managed.hy")
        mutant.write_text(mutation, encoding="utf-8")
        rejected = check(args.hyb, mutant)
        reject_codes = {x["code"] for x in rejected["diagnostics"] if x["severity"] == "error"}
        if not reject_codes.intersection({"HYB-E3001", "HYB-E3013", "HYB-E3005"}):
            raise RuntimeError("managed reversal did not restore a boundary diagnostic")
        save(output / f"{package}-managed-reversal.json", rejected)
        original = ROOT / "upstream" / package / (package + ".go")
        added, deleted = diff_counts(original, source)
        helper = ROOT / "ports" / package / "api.go"
        row = {"program": package, "original_lines": lines(original),
               "hybrid_lines": lines(source), "helper_go_lines": lines(helper),
               "added": added, "deleted": deleted,
               "hybrid_diff_tax_percent": round(100 * (added + deleted) / lines(original), 3),
               "total_tax_including_helpers_percent": round(100 * (added + deleted + lines(helper)) / lines(original), 3),
               "managed_blocks": checker["summary"]["managed"]["managed_blocks"],
               "go_escape_inventory": manifest["ports"][package]["escapes"],
               "behavior": "UNCHANGED_UPSTREAM_TESTS_PASS_RACE",
               "classification": classify(checker, True, manifest["ports"][package]["escapes"]),
               "reversal_codes": sorted(reject_codes),
               "independent_review": "PENDING",
               "primary_census_eligible": False}
        naive = ROOT / "ports" / package / "naive.hy"
        if naive.exists():
            save(output / f"{package}-naive-checker.json", check(args.hyb, naive.relative_to(ROOT)))
            a, d = diff_counts(naive, source)
            row["naive_to_accepted_added"] = a
            row["naive_to_accepted_deleted"] = d
        else:
            row["naive_to_accepted"] = "NOT_AVAILABLE_EXPLORATORY_PORT"
        rows.append(row)
    sweep_count, histogram = sweep(args.hyb, args.go, output)
    result = {"ports": rows, "baseline_tests": baseline_summary,
              "candidate_tests": candidate_summary, "sweep_source_files": sweep_count,
              "sweep_histogram": histogram, "census_verdict": "INCONCLUSIVE",
              "reason": "production sources exceed size target; singleflight exploratory; independent review and seven other ports outstanding",
              "descriptive_median_managed_per_100_hybrid_lines": statistics.median(
                  100 * x["managed_blocks"] / x["hybrid_lines"] for x in rows),
              "descriptive_median_total_tax_percent": statistics.median(
                  x["total_tax_including_helpers_percent"] for x in rows)}
    save(output / "results.json", result)
    keys = ["program", "original_lines", "hybrid_lines", "helper_go_lines", "added", "deleted",
            "managed_blocks", "total_tax_including_helpers_percent", "classification", "behavior"]
    (output / "census.tsv").write_text("\t".join(keys) + "\n" + "".join(
        "\t".join(str(row[key]) for key in keys) + "\n" for row in rows), encoding="utf-8")
    print(json.dumps({"output": str(output), "verdict": result["census_verdict"],
                      "ports": [x["classification"] for x in rows],
                      "test_events": candidate_summary["passed_test_events"]}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hyb", required=True)
    parser.add_argument("--go", default="go")
    parser.add_argument("--out", required=True)
    parser.add_argument("--count", type=int, default=3)
    args = parser.parse_args()
    if args.count < 1:
        parser.error("--count must be positive")
    args.hyb = str(Path(args.hyb).resolve())
    reproduce(args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        print("census failed: " + str(error), file=sys.stderr)
        sys.exit(1)
