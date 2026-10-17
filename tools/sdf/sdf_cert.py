#!/usr/bin/env python3
"""SDF schedule certificate: generator + independent checker (issue #378).

Closed, fixed-rate, scalar-token network -> periodic schedule witness with
per-edge high-water marks. The checker shares no code with the generator: it
only replays the witness. Guarantee on acceptance: bounded FIFO storage and no
channel-token deadlock under the certified periodic schedule (nothing more).

Network JSON: {"actors": {name: {"effects": []}},
               "edges": [{"src","dst","prod","cons","init","token":"i32"}]}
Certificate JSON: {"edges", "repetitions", "schedule", "high_water", "bytes"}
"""
import json, sys
from fractions import Fraction
from math import lcm

SCALAR_BYTES = {"i8": 1, "u8": 1, "i16": 2, "u16": 2, "i32": 4, "u32": 4,
                "f32": 4, "i64": 8, "u64": 8, "f64": 8, "bool": 1}


class Reject(Exception):
    pass


def _admit(net):
    actors = net.get("actors", {})
    if not actors:
        raise Reject("empty network")
    for n, a in actors.items():
        if a.get("effects"):
            raise Reject(f"actor {n}: unmodeled effects {a['effects']}")
    for e in net.get("edges", []):
        for k in ("src", "dst"):
            if e[k] not in actors:
                raise Reject(f"edge references unknown actor {e[k]}")
        if e.get("token") not in SCALAR_BYTES:
            raise Reject(f"non-scalar token type {e.get('token')!r}")
        if e["prod"] < 1 or e["cons"] < 1 or e.get("init", 0) < 0:
            raise Reject("rates must be >= 1 and initial tokens >= 0")


def repetitions(net):
    """Smallest positive integer solution of the balance equations."""
    actors = list(net["actors"])
    r = {}
    adj = {a: [] for a in actors}
    for e in net["edges"]:
        adj[e["src"]].append((e["dst"], Fraction(e["prod"], e["cons"])))
        adj[e["dst"]].append((e["src"], Fraction(e["cons"], e["prod"])))
    for root in actors:  # closed network must also be connected per component
        if root in r:
            continue
        r[root] = Fraction(1)
        stack = [root]
        while stack:
            u = stack.pop()
            for v, f in adj[u]:
                want = r[u] * f
                if v not in r:
                    r[v] = want
                    stack.append(v)
                elif r[v] != want:
                    raise Reject("inconsistent rates (no periodic schedule)")
    m = lcm(*(x.denominator for x in r.values()))
    vec = {a: int(r[a] * m) for a in actors}
    # normalise per connected component is unnecessary: gcd over all
    from math import gcd
    g = 0
    for v in vec.values():
        g = gcd(g, v)
    return {a: v // g for a, v in vec.items()}


def certify(net):
    _admit(net)
    reps = repetitions(net)
    edges = net["edges"]
    tok = [e.get("init", 0) for e in edges]
    hw = list(tok)
    left = dict(reps)
    sched = []
    while any(left.values()):
        progress = False
        for a in net["actors"]:
            if left[a] and all(tok[i] >= e["cons"] for i, e in enumerate(edges) if e["dst"] == a):
                for i, e in enumerate(edges):
                    if e["dst"] == a:
                        tok[i] -= e["cons"]
                    if e["src"] == a:
                        tok[i] += e["prod"]
                        hw[i] = max(hw[i], tok[i])
                left[a] -= 1
                sched.append(a)
                progress = True
        if not progress:
            raise Reject("deadlock: insufficient initial tokens on a cycle")
    total = sum(h * SCALAR_BYTES[e["token"]] for h, e in zip(hw, edges))
    return {"edges": edges, "repetitions": reps, "schedule": sched,
            "high_water": hw, "bytes": total}


def check(net, cert):
    """Independent replay. Returns byte budget or raises Reject."""
    _admit(net)
    edges = net["edges"]
    if cert.get("edges") != edges:
        raise Reject("certificate edges differ from network")
    reps = cert["repetitions"]
    if set(reps) != set(net["actors"]) or any(v < 1 for v in reps.values()):
        raise Reject("bad repetition vector")
    fired = {a: 0 for a in reps}
    tok = [e.get("init", 0) for e in edges]
    peak = list(tok)
    for a in cert["schedule"]:
        if a not in fired:
            raise Reject(f"unknown actor {a} in schedule")
        for i, e in enumerate(edges):
            if e["dst"] == a:
                if tok[i] < e["cons"]:
                    raise Reject(f"underflow on edge {i} firing {a}")
                tok[i] -= e["cons"]
        for i, e in enumerate(edges):
            if e["src"] == a:
                tok[i] += e["prod"]
                peak[i] = max(peak[i], tok[i])
                if tok[i] > cert["high_water"][i]:
                    raise Reject(f"overflow on edge {i}: exceeds declared budget")
        fired[a] += 1
    if fired != reps:
        raise Reject("firing counts differ from repetition vector")
    if tok != [e.get("init", 0) for e in edges]:
        raise Reject("final token counts differ from initial counts")
    if peak != list(cert["high_water"]):
        raise Reject("declared high-water marks are not tight")
    budget = sum(h * SCALAR_BYTES[e["token"]] for h, e in zip(peak, edges))
    if budget != cert["bytes"]:
        raise Reject("byte budget mismatch")
    return budget


def main(argv):
    if len(argv) < 3 or argv[1] not in ("certify", "check"):
        print("usage: sdf_cert.py certify NET.json | check NET.json CERT.json")
        return 2
    net = json.load(open(argv[2]))
    try:
        if argv[1] == "certify":
            print(json.dumps(certify(net), indent=1))
        else:
            print("ACCEPT bytes=%d" % check(net, json.load(open(argv[3]))))
    except Reject as ex:
        print("REJECT:", ex)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
