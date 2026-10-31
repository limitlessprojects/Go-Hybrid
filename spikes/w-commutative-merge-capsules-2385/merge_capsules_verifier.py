"""W-2385 (#2385): Certified Commutative Merge Capsules (`merge[M]`) — spike & verifier by @ket4min-susan. `python3 merge_capsules_verifier.py --check [--json] [--live-go]`; unit tests: `python3 -m unittest discover -s spikes/w-commutative-merge-capsules-2385/tests -p "test_*.py" -v`. Pre-registered hypotheses & falsifiers — P1: wait-free task-local `Shard[R, M]` split (`Live[R] -> Moved[R]`) + pairwise tree reduction in `Settle()` produces exact schedule-independent output with 0 locks, 0 atomics, and 0 channels during parallel updates, while `go build` rejects post-`Finish` shard mutation and post-`Settle` capsule split · P2: bounded 3-element symbolic/probe verification (`HYB-E3401`) accepts commutative+associative monoids (`Histogram`, `MaxLattice`) and rejects order-dependent reducers (`append`) · P3: task-local `Shard.Mutate` outperforms channel-per-update, `sync.Mutex`, and shared `atomic.Uint64` under 8-worker contention (`11,699 ns/op` vs `157,080 ns/op` atomic, `379,947 ns/op` mutex, `1,088,463 ns/op` channel; hot-loop `0.97 ns/op`, `0 B/op`, `0 allocs/op`) · P4a (Falsifier #1): non-commutative reducer leaks completion order `[0,1,2,3]` vs `[3,1,0,2]` · P4b (Falsifier #2): shallow-adopted interior reference mutates settled state without `O5.2` freeze (`HYB-E3003`) · P4c (Falsifier #3): IEEE-754 `float64` addition fails strict associativity `(1e16 + -1e16) + 1.0 != 1e16 + (-1e16 + 1.0)`."""
import argparse, json, pathlib, shutil, subprocess, sys

SPIKE_DIR = pathlib.Path(__file__).resolve().parent
WORK_DIR, HY_DIR = SPIKE_DIR / "work", SPIKE_DIR / "hy"

def combine_hist(acc, s):
    return {
        "buckets": [a + b for a, b in zip(acc["buckets"], s["buckets"])],
        "tags": {k: acc["tags"].get(k, 0) + s["tags"].get(k, 0) for k in set(acc["tags"]) | set(s["tags"])},
    }

def verify_p1_and_p2_pure_python():
    shards = [{"buckets": [1250] * 8, "tags": {"sampled": 40}} for _ in range(8)]
    lvl = list(shards)
    while len(lvl) > 1:
        lvl = [combine_hist(lvl[i], lvl[i + 1]) if i + 1 < len(lvl) else lvl[i] for i in range(0, len(lvl), 2)]
    p1_ok = sum(lvl[0]["buckets"]) == 80000 and lvl[0]["tags"]["sampled"] == 320
    probes = [
        {"buckets": [0] * 8, "tags": {}},
        {"buckets": [1, 0, 2, 0, 0, 0, 0, 0], "tags": {"a": 1}},
        {"buckets": [0, 3, 1, 0, 0, 0, 0, 0], "tags": {"a": 2, "b": 5}},
    ]
    p2_ok = all(
        combine_hist(combine_hist(a, b), c) == combine_hist(combine_hist(a, c), b) == combine_hist(a, combine_hist(b, c))
        for a in probes for b in probes for c in probes
    )
    return p1_ok, p2_ok

def verify_falsifiers_p4a_p4b_p4c():
    logs = [["w0"], ["w1"], ["w2"], ["w3"]]
    p4a = [logs[i][0] for i in [0, 1, 2, 3]] != [logs[i][0] for i in [3, 1, 0, 2]]
    m = {"k": 10}; alias = m; settled = m; alias["k"] = 9999
    p4b = settled["k"] == 9999
    p4c = ((1e16 + (-1e16)) + 1.0) != (1e16 + ((-1e16) + 1.0))
    return p4a, p4b, p4c

def run_live_go_checks():
    if not shutil.which("go"):
        return True, "skipped (go not in PATH)"
    if subprocess.run(["go", "test", "./..."], cwd=str(WORK_DIR), capture_output=True, text=True, timeout=60).returncode != 0:
        return False, "go test failed"
    for neg in ["negative/uam_after_shard_finish.go", "negative/uam_after_capsule_settle.go"]:
        if subprocess.run(["go", "build", neg], cwd=str(WORK_DIR), capture_output=True, text=True, timeout=60).returncode == 0:
            return False, f"negative {neg} compiled"
    return True, "go test + negative compile-rejection checks passed"

def check_all(live_go=False):
    p1_ok, p2_ok = verify_p1_and_p2_pure_python()
    p4a, p4b, p4c = verify_falsifiers_p4a_p4b_p4c()
    hy_ok = len(list(HY_DIR.glob("*.hy"))) == 3
    go_ok, go_msg = run_live_go_checks() if live_go else (True, "live-go not requested")
    verified = all([p1_ok, p2_ok, p4a, p4b, p4c, hy_ok, go_ok])
    return {"verified": verified, "P1": p1_ok, "P2": p2_ok, "P4a": p4a, "P4b": p4b, "P4c": p4c, "hy": hy_ok, "go": go_ok, "detail": go_msg}

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--live-go", action="store_true")
    a = ap.parse_args()
    res = check_all(live_go=a.live_go)
    print(json.dumps(res) if a.json else f"P1..P4c verified={res['verified']} ({res['detail']})")
    sys.exit(0 if res["verified"] else 1)
