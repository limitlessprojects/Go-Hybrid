"""Spike for wildcard proposal #4642: bind an orientation decision to what it may change.

The proposal makes the association between a robustness sign and the input sites it
was computed from part of the checked interface, instead of leaving it in caller code:
`observe(mesh, i, j, k)` borrows a coordinate snapshot, `orientation(observation)`
returns a turn with the evidence attached, and a checked insert consumes that evidence
only for exactly that mesh revision, that ordered triple and those coordinates.

Model, no backend. The point is to falsify the claim that a bare sign is enough:

- exact orientation over integers bounded by 2^50, big-int arithmetic, no rounding
- a float64 orientation of the same formula, to measure how often the bare sign is
  wrong at that bound (motivation, not decoration)
- an evidence object carrying (mesh revision, ordered triple, snapshot digest, turn)
  and every stale-evidence rejection the proposal names
- what the fragment cannot promise once a caller hands control to unverified Go
"""

from __future__ import annotations

import dataclasses
import hashlib
from typing import Dict, List, Sequence, Tuple

COORD_LIMIT = 1 << 50
LEFT, RIGHT, COLLINEAR = "left", "right", "collinear"

E_STALE_REVISION = "HYB-E4642-01"   # evidence from another revision of the mesh
E_OTHER_MESH = "HYB-E4642-02"       # evidence from a different mesh
E_TRIPLE_MISMATCH = "HYB-E4642-03"  # reordered or different vertex triple
E_MOVED_VERTEX = "HYB-E4642-04"     # a coordinate changed since the observation
E_TURN_DISAGREES = "HYB-E4642-05"   # supplied turn is not the certified one
E_ESCAPED = "HYB-E4642-06"          # site written through unverified Go


class EvidenceError(Exception):
    def __init__(self, code: str, detail: str = "") -> None:
        super().__init__(f"{code}: {detail}" if detail else code)
        self.code = code


@dataclasses.dataclass
class Mesh:
    name: str
    coords: Dict[int, Tuple[int, int]]
    revision: int = 0

    def digest(self) -> str:
        raw = repr(sorted(self.coords.items())).encode()
        return hashlib.sha256(raw).hexdigest()[:16]

    def move(self, vertex: int, x: int, y: int) -> None:
        self.coords[vertex] = (x, y)
        self.revision += 1


@dataclasses.dataclass(frozen=True)
class Observation:
    mesh: str
    revision: int
    triple: Tuple[int, int, int]
    snapshot: Tuple[int, int, int, int, int, int]
    digest: str


@dataclasses.dataclass(frozen=True)
class Turn:
    observation: Observation
    turn: str

    def __post_init__(self) -> None:
        if self.turn not in (LEFT, RIGHT, COLLINEAR):
            raise EvidenceError(E_TURN_DISAGREES, self.turn)


def _bounded(mesh: Mesh, vertex: int) -> Tuple[int, int]:
    x, y = mesh.coords[vertex]
    if abs(x) > COORD_LIMIT or abs(y) > COORD_LIMIT:
        raise EvidenceError(E_ESCAPED, f"vertex {vertex} above 2^50")
    return x, y


def exact_orientation(p, q, r) -> int:
    """Sign of the cross product (q-p)x(r-p) in whole numbers."""
    ax, ay = q[0] - p[0], q[1] - p[1]
    bx, by = r[0] - p[0], r[1] - p[1]
    v = ax * by - ay * bx
    return (v > 0) - (v < 0)


def float_orientation(p, q, r) -> int:
    """Same formula in float64 — the 'bare sign' a library would hand the caller."""
    ax, ay = float(q[0]) - float(p[0]), float(q[1]) - float(p[1])
    bx, by = float(r[0]) - float(p[0]), float(r[1]) - float(p[1])
    v = ax * by - ay * bx
    return (v > 0) - (v < 0)


def _sign_to_turn(s: int) -> str:
    return {1: LEFT, -1: RIGHT, 0: COLLINEAR}[s]


def observe(mesh: Mesh, i: int, j: int, k: int) -> Observation:
    triple = (i, j, k)
    snap: List[int] = []
    for v in triple:
        snap.extend(_bounded(mesh, v))
    return Observation(mesh.name, mesh.revision, triple, tuple(snap), mesh.digest())


def orientation(obs: Observation) -> Turn:
    x0, y0, x1, y1, x2, y2 = obs.snapshot
    return Turn(obs, _sign_to_turn(exact_orientation((x0, y0), (x1, y1), (x2, y2))))


def _split(obs: Observation) -> Tuple[Tuple[int, int], ...]:
    s = obs.snapshot
    return ((s[0], s[1]), (s[2], s[3]), (s[4], s[5]))


def certify(mesh: Mesh, turn: Turn) -> Tuple[int, int, int]:
    """Consume the evidence; returns the authorized triple or raises."""
    obs = turn.observation
    if obs.mesh != mesh.name:
        raise EvidenceError(E_OTHER_MESH, obs.mesh)
    if obs.revision != mesh.revision:
        raise EvidenceError(E_STALE_REVISION, f"{obs.revision} != {mesh.revision}")
    if obs.digest != mesh.digest():
        raise EvidenceError(E_MOVED_VERTEX, "coordinates moved under the evidence")
    for vertex, (x, y) in zip(obs.triple, _split(obs)):
        if mesh.coords[vertex] != (x, y):
            raise EvidenceError(E_MOVED_VERTEX, f"vertex {vertex}")
    current = _sign_to_turn(exact_orientation(*_split(obs)))
    if current != turn.turn:
        raise EvidenceError(E_TURN_DISAGREES, f"{turn.turn} != {current}")
    return obs.triple


def insert_face(mesh: Mesh, turn: Turn, face: Tuple[int, int, int] | None = None) -> Tuple[int, int, int]:
    """The checked write: evidence in, topology out. Mesh revision advances.

    `face` is the ordered triple the caller means to write. A permutation of the
    same vertices is a different face, so it is a mismatch, not a warning.
    """
    if face is not None and face != turn.observation.triple:
        raise EvidenceError(E_TRIPLE_MISMATCH,
                            f"{turn.observation.triple} != {face}")
    triple = certify(mesh, turn)
    mesh.revision += 1
    return triple


# --- measurement ------------------------------------------------------------
#
# Two generators, because random triples are rarely near-degenerate and would
# flatter float64. The near-collinear one is the honest adversary: it is where a
# bare sign loses the association to its inputs.


def _lcg(seed: int):
    state = seed & 0xFFFFFFFFFFFFFFFF

    def nxt() -> int:
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        return state

    return nxt


def _clamp(v: int, limit: int) -> int:
    return max(-limit, min(limit, v))


def _wide(nxt, limit: int):
    return [(nxt() % (2 * limit + 1)) - limit for _ in range(6)]


def _near_collinear(nxt, limit: int):  # noqa: D401
    """r sits on the line p->q, pushed off it by at most one unit."""
    px = (nxt() % (2 * limit + 1)) - limit
    py = (nxt() % (2 * limit + 1)) - limit
    dx = (nxt() % limit) + 1
    dy = (nxt() % limit) + 1
    k = (nxt() % 3) + 1
    ex, ey = _clamp(px + k * dx, limit), _clamp(py + k * dy, limit)
    q = (_clamp(px + dx, limit), _clamp(py + dy, limit))
    off = (nxt() % 3) - 1          # -1, 0, +1 unit off the line, vertically
    r = (ex, _clamp(ey + off, limit))
    return [px, py, q[0], q[1], r[0], r[1]]


def bare_sign_failures(samples: int, seed: int = 4642, near: bool = True, limit: int = COORD_LIMIT):
    """Count cases where the float64 sign differs from the exact one."""
    nxt = _lcg(seed)
    bad = 0
    first = []
    for _ in range(samples):
        base = _near_collinear(nxt, limit) if near else _wide(nxt, limit)
        p, q, r = (base[0], base[1]), (base[2], base[3]), (base[4], base[5])
        if float_orientation(p, q, r) != exact_orientation(p, q, r):
            bad += 1
            if len(first) < 3:
                first.append((p, q, r))
    return bad, first


def hunt_degenerate(limit: int = COORD_LIMIT, tries: int = 20000):
    """The smallest witness found: exact sign non-zero, float sign zero."""
    nxt = _lcg(4642)
    best = None
    for _ in range(tries):
        base = _near_collinear(nxt, limit)
        p, q, r = (base[0], base[1]), (base[2], base[3]), (base[4], base[5])
        exact = exact_orientation(p, q, r)
        if exact != 0 and float_orientation(p, q, r) == 0:
            span = max(abs(p[0]), abs(q[0]), abs(r[0]), abs(p[1]), abs(q[1]), abs(r[1]))
            if best is None or span < best[0]:
                best = (span, p, q, r, exact)
    return best


if __name__ == "__main__":
    for exp in (50, 51, 52, 53):
        limit = 1 << exp
        bad, _ = bare_sign_failures(20000, limit=limit)
        print(f"2^{exp} near-collinear: {bad}/20000 float signs wrong")
    wide_bad, _ = bare_sign_failures(20000, near=False)
    print(f"2^50 uniform random:   {wide_bad}/20000 float signs wrong")
    wit = hunt_degenerate()
    if wit:
        span, p, q, r, exact = wit
        print(f"witness span={span} p={p} q={q} r={r} exact={_sign_to_turn(exact)} float=collinear")
