"""W-2620 (#2620): Compiler-Generated Durable Continuations — spike & verifier by @ket4min-susan."""
import argparse, copy, hashlib, json, pathlib, re, shutil, subprocess, sys

SPIKE_DIR = pathlib.Path(__file__).resolve().parent
WORK_DIR, HY_DIR = SPIKE_DIR / "work", SPIKE_DIR / "hy"
BUILD_ID, SCHEMA_VERSION = "hyb-w2620-build-v1", 1

RULES = [
    (r"\b(os|net|http)\.[a-zA-Z_]", "HYB-E3620-IO", "external I/O or foreign call forbidden in @[durable_region]"),
    (r"\b(time\.now|rand\.[a-zA-Z_])", "HYB-E3620-NONDET", "clocks/randomness forbidden in @[durable_region]"),
    (r"\*[a-zA-Z_]", "HYB-E3620-PTR", "pointers/handles forbidden in @[durable_region]"),
    (r":=\s*fn\s*\(", "HYB-E3620-CLOSURE", "arbitrary closure forbidden in @[durable_region]"),
    (r"\bmanaged\s*\(", "HYB-E3620-MANAGED", "managed block forbidden in @[durable_region]"),
    (r"make\s*\(\s*chan\s+[^,)]+\)", "HYB-E3620-UNBOUNDED-CHAN", "durable region channels require bounded FIFO capacity"),
]

def check_durable_region_admission(src: str):
    diags, in_loop = [], 0
    for lineno, raw in enumerate(src.splitlines(), 1):
        code = raw.split("//", 1)[0].strip()
        if not code:
            continue
        in_loop += 1 if re.match(r"^(for|while)\b", code) else 0
        diags.extend({"line": lineno, "code": dcode, "msg": msg} for pat, dcode, msg in RULES if re.search(pat, code))
        if in_loop > 0 and re.search(r"\btask\s+[a-zA-Z_]", code):
            diags.append({"line": lineno, "code": "HYB-E3620-DYNAMIC-SPAWN", "msg": "dynamic task spawn in loop forbidden"})
    return diags

def compute_digest(snap: dict) -> str:
    body = {**copy.deepcopy(snap), "digest": ""}
    return hashlib.sha256(json.dumps(body, sort_keys=True, separators=(",", ":")).encode()).hexdigest()

def validate_snapshot(snap: dict) -> tuple[bool, str]:
    if snap.get("build_id") != BUILD_ID or snap.get("schema_version") != SCHEMA_VERSION:
        return False, "ERR_BUILD_IDENTITY_MISMATCH"
    if not snap.get("digest") or compute_digest(snap) != snap["digest"]:
        return False, "ERR_CORRUPT_CUT"
    qmap = snap["queues"]
    if any(t["pc"] < 0 or t["pc"] > 2 or (t["blocked_on"] and t["blocked_on"] not in qmap) for t in snap["tasks"]) or any(len(q["items"]) > q["cap"] for q in qmap.values()):
        return False, "ERR_INVALID_CONTINUATION"
    prod = sum(t["locals"]["emitted"] for t in snap["tasks"])
    cons = sum(t["locals"]["consumed"] for t in snap["tasks"])
    inflight = sum(len(q["items"]) for q in qmap.values())
    return (True, "OK") if prod == inflight + cons else (False, "ERR_TORN_CUT")

def new_ptr_region(n: int = 10, q_cap: int = 3) -> dict:
    snap = {
        "region_id": "ptr_v1", "build_id": BUILD_ID, "schema_version": SCHEMA_VERSION, "commit_seq": 0, "scheduler_cursor": 0, "result": 0, "completed": False,
        "tasks": [
            {"id": "producer", "kind": "prod", "pc": 0, "blocked_on": "", "done": False, "locals": {"i": 1, "n": n, "emitted": 0, "consumed": 0}},
            {"id": "transformer", "kind": "xfm", "pc": 0, "blocked_on": "", "done": False, "locals": {"cur": 0, "emitted": 0, "consumed": 0}},
            {"id": "reducer", "kind": "red", "pc": 0, "blocked_on": "", "done": False, "locals": {"acc": 0, "emitted": 0, "consumed": 0}},
        ],
        "queues": {"raw_q": {"cap": q_cap, "items": [], "closed": False}, "scaled_q": {"cap": q_cap, "items": [], "closed": False}}, "digest": "",
    }
    snap["digest"] = compute_digest(snap)
    return snap

def step_once(snap: dict) -> bool:
    if snap["completed"]:
        return False
    tasks, qmap, n_tasks = snap["tasks"], snap["queues"], len(snap["tasks"])
    for off in range(n_tasks):
        idx = (snap["scheduler_cursor"] + off) % n_tasks
        t, ok = tasks[idx], False
        if t["done"]:
            continue
        if t["kind"] == "prod":
            q = qmap["raw_q"]
            if t["locals"]["i"] > t["locals"]["n"]:
                q["closed"], t["done"], t["pc"], t["blocked_on"], ok = True, True, 2, "", True
            elif len(q["items"]) < q["cap"]:
                v = t["locals"]["i"]
                q["items"].append(v * v + 3 * v)
                t["locals"]["i"] += 1; t["locals"]["emitted"] += 1; t["pc"], t["blocked_on"], ok = 1, "", True
            else:
                t["blocked_on"] = "raw_q"
        elif t["kind"] == "xfm":
            if t["pc"] == 0:
                in_q = qmap["raw_q"]
                if in_q["items"]:
                    t["locals"]["cur"] = in_q["items"].pop(0); t["locals"]["consumed"] += 1; t["pc"], t["blocked_on"], ok = 1, "", True
                elif in_q["closed"]:
                    qmap["scaled_q"]["closed"], t["done"], t["pc"], t["blocked_on"], ok = True, True, 2, "", True
                else:
                    t["blocked_on"] = "raw_q"
            elif len(qmap["scaled_q"]["items"]) < qmap["scaled_q"]["cap"]:
                qmap["scaled_q"]["items"].append(t["locals"]["cur"] * 5 + 7); t["locals"]["emitted"] += 1; t["pc"], t["blocked_on"], ok = 0, "", True
            else:
                t["blocked_on"] = "scaled_q"
        elif t["kind"] == "red":
            in_q = qmap["scaled_q"]
            if in_q["items"]:
                t["locals"]["acc"] += in_q["items"].pop(0); t["locals"]["consumed"] += 1; t["pc"], t["blocked_on"], ok = 1, "", True
            elif in_q["closed"]:
                snap["result"], t["done"], t["pc"], t["blocked_on"], ok = t["locals"]["acc"], True, 2, "", True
            else:
                t["blocked_on"] = "scaled_q"
        if ok:
            snap["scheduler_cursor"] = (idx + 1) % n_tasks
            snap["completed"] = all(x["done"] for x in tasks)
            return True
    return False

def verify_pure_python_gates():
    hy = [f.read_text(encoding="utf-8") for f in sorted(HY_DIR.glob("*.hy"))]
    codes03 = {d["code"] for d in check_durable_region_admission(hy[2])}
    p1_ok = not check_durable_region_admission(hy[0]) and not check_durable_region_admission(hy[1]) and {"HYB-E3620-IO", "HYB-E3620-NONDET", "HYB-E3620-PTR", "HYB-E3620-CLOSURE", "HYB-E3620-MANAGED"} <= codes03
    ref, total_steps = new_ptr_region(10, 3), 0
    while step_once(ref):
        total_steps += 1
    p2_ok, saved_75 = True, 0
    for crash_step in range(1, total_steps + 1):
        s = new_ptr_region(10, 3); committed = copy.deepcopy(s)
        for step in range(1, crash_step + 1):
            step_once(s)
            if step % 3 == 0:
                s["commit_seq"] += 1; s["digest"] = compute_digest(s); committed = copy.deepcopy(s)
        rec, post = copy.deepcopy(committed), 0
        while step_once(rec):
            post += 1
        p2_ok = p2_ok and validate_snapshot(committed)[0] and rec["result"] == ref["result"] and rec["completed"]
        if crash_step == (total_steps * 3) // 4:
            saved_75 = total_steps - post
    base = new_ptr_region(8, 4)
    for _ in range(5):
        step_once(base)
    base["digest"] = compute_digest(base)
    f1, f2, f3, f4, f5 = [copy.deepcopy(base) for _ in range(5)]
    f1["queues"]["raw_q"]["items"].pop(0); f1["digest"] = compute_digest(f1)
    f2["tasks"][0]["locals"]["emitted"] += 1; f2["digest"] = compute_digest(f2)
    f3["tasks"][1]["pc"] = 99; f3["digest"] = compute_digest(f3)
    f4["build_id"] = "stale"; f4["digest"] = compute_digest(f4)
    f5["digest"] = "00" * 32
    negs_ok = (validate_snapshot(f1) == (False, "ERR_TORN_CUT") and validate_snapshot(f2) == (False, "ERR_TORN_CUT") and validate_snapshot(f3) == (False, "ERR_INVALID_CONTINUATION") and validate_snapshot(f4) == (False, "ERR_BUILD_IDENTITY_MISMATCH") and validate_snapshot(f5) == (False, "ERR_CORRUPT_CUT"))
    return p1_ok, p2_ok and saved_75 > 0, negs_ok, {"total_steps": total_steps, "saved_steps_75pct": saved_75, "snapshot_bytes": len(json.dumps(base, separators=(",", ":")).encode())}

def run_live_go_checks():
    if not shutil.which("go"):
        return True, "skipped (go not in PATH)"
    r = subprocess.run(["go", "test", "./..."], cwd=str(WORK_DIR), capture_output=True, text=True, timeout=60)
    return (r.returncode == 0), ("go test passed" if r.returncode == 0 else r.stderr or r.stdout)

def check_all(live_go: bool = False):
    p1_ok, p2_ok, negs_ok, metrics = verify_pure_python_gates()
    go_ok, go_msg = run_live_go_checks() if live_go else (True, "live-go not requested")
    return {"verified": all([p1_ok, p2_ok, negs_ok, go_ok]), "P1_admission": p1_ok, "P2_recovery_parity": p2_ok, "P3_negative_controls": negs_ok, "go": go_ok, "metrics": metrics, "detail": go_msg}

if __name__ == "__main__":
    ap = argparse.ArgumentParser(); ap.add_argument("--check", action="store_true"); ap.add_argument("--json", action="store_true"); ap.add_argument("--live-go", action="store_true")
    a = ap.parse_args(); res = check_all(live_go=a.live_go)
    print(json.dumps(res) if a.json else f"W-2620 verified={res['verified']} ({res['detail']}) metrics={res['metrics']}")
    sys.exit(0 if res["verified"] else 1)
