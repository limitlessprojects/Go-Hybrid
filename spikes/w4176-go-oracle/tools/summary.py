#!/usr/bin/env python3
"""summary.py — render the measured numbers from a run.sh work directory.

Reads results.json, results-extended.json, oracle.json and prints the table
that RESULTS.md is copied from. No numbers are invented here: every value is
read from a file produced by the run.
"""
import json
import os
import sys


def load(p):
    with open(p) as f:
        return json.load(f)


def main(work):
    base = load(os.path.join(work, "results.json"))
    ext = load(os.path.join(work, "results-extended.json"))
    orc = load(os.path.join(work, "oracle.json"))

    failed = []
    bf = os.path.join(work, "build_failed.txt")
    if os.path.exists(bf):
        failed = [l.strip() for l in open(bf) if l.strip()]

    def count(rows, status):
        return sum(1 for r in rows if r["status"] == status)

    print("corpus programs           : %d" % len(base))
    print("emitted Go (ACCEPT corpus): %d" % len([r for r in base if r["status"] != "NOT_EMITTED"]))
    print("build failures            : %d %s" % (len(failed), failed))
    print("pre-registered Tier B     : certified=%d race=%d outside=%d"
          % (count(base, "CERTIFIED"), count(base, "RACE_DETECTED"), count(base, "OUTSIDE_BOUND")))
    print("extended bound            : certified=%d race=%d outside=%d"
          % (count(ext, "CERTIFIED"), count(ext, "RACE_DETECTED"), count(ext, "OUTSIDE_BOUND")))

    verified = [r for r in ext if r["status"] in ("CERTIFIED", "RACE_DETECTED")]
    print("POR agreement (full vs POR): %d/%d" % (sum(1 for r in verified if r.get("por_agrees")), len(verified)))
    print("certificates verified     : %d/%d" % (sum(1 for r in verified if r["verify_ok"]), len(verified)))

    cert = [r for r in verified if r["status"] == "CERTIFIED"]
    if cert:
        print("max certified states      : %d (%s)"
              % (max(r["states"] for r in cert), max(cert, key=lambda r: r["states"])["program"]))
        print("max full states           : %d (%s)"
              % (max(r["full_states"] for r in cert), max(cert, key=lambda r: r["full_states"])["program"]))
        print("POR reduction             : %d -> %d states (%.1f%%)"
              % (sum(r["full_states"] for r in cert), sum(r["states"] for r in cert),
                 100.0 * sum(r["states"] for r in cert) / max(1, sum(r["full_states"] for r in cert))))
        print("max generation            : %.3f ms (%s)"
              % (max(r["gen_us"] for r in cert) / 1000.0,
                 max(cert, key=lambda r: r["gen_us"])["program"]))
        print("max replay                : %.3f ms (%s)"
              % (max(r["verify_us"] for r in cert) / 1000.0,
                 max(cert, key=lambda r: r["verify_us"])["program"]))
        print("median replay             : %.3f ms"
              % (sorted(r["verify_us"] for r in cert)[len(cert) // 2] / 1000.0))

    ran = [r for r in orc if r.get("runs", 0) > 0]
    agree = [r for r in ran if r.get("agrees")]
    print("oracle programs run       : %d" % len(ran))
    print("oracle agreement          : %d/%d" % (len(agree), len(ran)))
    for r in ran:
        if not r.get("agrees"):
            print("  DISAGREE %s status=%s races=%d fatals=%d"
                  % (r["program"], r["status"], r["races"], r["fatals"]))
    race = [r for r in ran if r["status"] == "RACE_DETECTED"]
    for r in race:
        print("  RACE_DETECTED %s: %d/%d runs reported a race; sample: %s"
              % (r["program"], r["races"], r["runs"], r.get("sample", "")[:80]))

    # corpus agreement on the extended set
    agree_corpus = 0
    for r in verified:
        exp = r.get("expected", "")
        got = r["status"]
        if (exp == "ACCEPT" and got == "CERTIFIED") or \
           (exp == "ACCEPT-MANAGED" and got in ("CERTIFIED", "RACE_DETECTED")):
            agree_corpus += 1
    print("corpus-verdict agreement  : %d/%d" % (agree_corpus, len(verified)))


if __name__ == "__main__":
    main(sys.argv[1])
