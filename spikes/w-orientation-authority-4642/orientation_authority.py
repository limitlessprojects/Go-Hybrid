"""W-4642 runtime half: certified orientation authority for a mesh edit.

Two separate claims are modelled here.

1. The predicate.  A conservative binary64 filter over exactly representable
   coordinate differences, with an exact integer fallback.  The filter is
   allowed to answer only when its outward-rounded intervals are disjoint;
   everything else falls back to exact integer arithmetic on the *original*
   coordinates.

2. The authority.  A ``Turn`` witness carries the mesh identity, the mesh
   revision, the ordered vertex triple, and a digest of the coordinates that
   were actually classified.  ``Mesh.commit`` accepts it only when all four
   still match the mesh at commit time.  A sign-only API (the control) has no
   such gate and happily applies a valid sign to a different edit.

Requires Python 3.10+.  Standard library only.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from math import inf, nextafter
from random import Random

COORD_BOUND = 1 << 50

LEFT, COLLINEAR, RIGHT = 1, 0, -1


def sign(number: int | float) -> int:
    return (number > 0) - (number < 0)


# ---------------------------------------------------------------------------
# 1. The predicate
# ---------------------------------------------------------------------------


def _check_point(p: tuple[int, int]) -> None:
    for coordinate in p:
        if type(coordinate) is not int:
            raise TypeError("orientation authority is defined for integer coordinates only")
        if abs(coordinate) > COORD_BOUND:
            raise ValueError(f"coordinate {coordinate} exceeds the pre-registered bound 2^50")


def edges(a: tuple[int, int], b: tuple[int, int], c: tuple[int, int]) -> tuple[int, int, int, int]:
    """Differences from ``a``.  Exact in binary64 because |diff| <= 2^51 < 2^53."""
    for point in (a, b, c):
        _check_point(point)
    return b[0] - a[0], b[1] - a[1], c[0] - a[0], c[1] - a[1]


def oracle(a: tuple[int, int], b: tuple[int, int], c: tuple[int, int]) -> int:
    """Exact integer shoelace sum in the original coordinate system."""
    forward = a[0] * b[1] + b[0] * c[1] + c[0] * a[1]
    reverse = a[1] * b[0] + b[1] * c[0] + c[1] * a[0]
    return sign(forward - reverse)


def naive(a: tuple[int, int], b: tuple[int, int], c: tuple[int, int]) -> int:
    """The obvious float64 implementation.  The control that must be wrong."""
    bx, by, cx, cy = edges(a, b, c)
    return sign(float(bx) * float(cy) - float(by) * float(cx))


def classify(a: tuple[int, int], b: tuple[int, int], c: tuple[int, int]) -> tuple[int, bool]:
    """Return ``(direction, used_exact_fallback)``.

    The filter answers only when the outward-rounded products cannot overlap.
    Otherwise the exact integer determinant of the original coordinates is used.
    """
    bx, by, cx, cy = edges(a, b, c)
    left = float(bx) * float(cy)
    right = float(by) * float(cx)
    if nextafter(left, -inf) > nextafter(right, inf):
        return LEFT, False
    if nextafter(left, inf) < nextafter(right, -inf):
        return RIGHT, False
    # Exact path: integer products of the original differences.
    return sign(bx * cy - by * cx), True


def exact_from_rounded(a: tuple[int, int], b: tuple[int, int], c: tuple[int, int]) -> int:
    """Negative control: exactness applied *after* rounding cannot recover the bit."""
    bx, by, cx, cy = edges(a, b, c)
    return sign(int(float(bx) * float(cy)) - int(float(by) * float(cx)))


# ---------------------------------------------------------------------------
# 2. The authority
# ---------------------------------------------------------------------------

_MINT_KEY = object()


class ForgedWitness(Exception):
    """Raised when a witness did not come from this certifier."""


class StaleWitness(Exception):
    """Raised when the mesh, revision, triple or coordinates moved under the witness."""


@dataclass(frozen=True)
class Turn:
    """Opaque evidence.  Constructible only through ``Certifier.mint``."""

    mesh: int
    revision: int
    triple: tuple[int, int, int]
    direction: int
    coords_digest: str
    _key: object

    def __post_init__(self) -> None:
        if self._key is not _MINT_KEY:
            raise ForgedWitness("Turn witnesses are minted by the certifier, not by callers")

    def __repr__(self) -> str:  # never leak the digest into logs by accident
        return f"Turn(mesh={self.mesh}, rev={self.revision}, triple={self.triple}, dir={self.direction})"


class Mesh:
    """A mesh with a revision counter and private coordinate storage."""

    def __init__(self, mesh_id: int, points: list[tuple[int, int]]) -> None:
        self._id = mesh_id
        self._revision = 1
        self._points = list(points)

    # -- read-only observation surface ------------------------------------
    @property
    def id(self) -> int:
        return self._id

    @property
    def revision(self) -> int:
        return self._revision

    def point(self, index: int) -> tuple[int, int]:
        return self._points[index]

    def digest(self, triple: tuple[int, int, int]) -> str:
        payload = "|".join(f"{i}:{self._points[i][0]},{self._points[i][1]}" for i in triple)
        return hashlib.sha256(payload.encode()).hexdigest()[:16]

    # -- the only write path ----------------------------------------------
    def commit(self, triple: tuple[int, int, int], witness: Turn) -> int:
        if witness.mesh != self._id:
            raise StaleWitness(f"witness is bound to mesh {witness.mesh}, not {self._id}")
        if witness.revision != self._revision:
            raise StaleWitness(f"witness is bound to revision {witness.revision}, mesh is at {self._revision}")
        if witness.triple != triple:
            raise StaleWitness(f"witness is bound to {witness.triple}, commit asked for {triple}")
        if witness.coords_digest != self.digest(triple):
            raise StaleWitness("coordinates moved under a same-revision witness")
        self._apply_face(triple, witness.direction)
        return witness.direction

    def _apply_face(self, triple: tuple[int, int, int], direction: int) -> None:
        self._revision += 1

    def edit_point(self, index: int, point: tuple[int, int]) -> None:
        _check_point(point)
        self._points[index] = point
        self._revision += 1


class Certifier:
    """The trusted mint.  One per process; nothing else can create a Turn."""

    def observe(self, mesh: Mesh, triple: tuple[int, int, int]) -> Turn:
        a, b, c = (mesh.point(i) for i in triple)
        direction, _ = classify(a, b, c)
        return Turn(
            mesh=mesh.id,
            revision=mesh.revision,
            triple=triple,
            direction=direction,
            coords_digest=mesh.digest(triple),
            _key=_MINT_KEY,
        )


# ---------------------------------------------------------------------------
# 3. Helper boundary: classify here, commit there
# ---------------------------------------------------------------------------


def classify_helper(certifier: Certifier, mesh: Mesh, triple: tuple[int, int, int]) -> Turn:
    """An independently written helper.  It only ever returns evidence."""
    return certifier.observe(mesh, triple)


def commit_helper(mesh: Mesh, triple: tuple[int, int, int], witness: Turn) -> int:
    """The other half.  It cannot forge, only verify."""
    return mesh.commit(triple, witness)


# ---------------------------------------------------------------------------
# 4. Sign-only control: what the ordinary library API would do
# ---------------------------------------------------------------------------


def sign_only_commit(mesh: Mesh, triple: tuple[int, int, int], direction: int) -> int:
    """The control: the caller carries a bare sign and the order it *claims*."""
    mesh._apply_face(triple, direction)  # noqa: SLF001 - deliberately unguarded
    return direction


# ---------------------------------------------------------------------------
# 5. Measurement harness
# ---------------------------------------------------------------------------


def family_random(count: int, bound: int, seed: int) -> list[tuple[tuple[int, int], ...]]:
    rng = Random(seed)
    return [
        tuple((rng.randint(-bound, bound), rng.randint(-bound, bound)) for _ in range(3))
        for _ in range(count)
    ]


def family_unit_determinant() -> list[tuple[tuple[int, int], ...]]:
    """Determinant +-1 at n = 2^27 .. 2^49, both vertex orders."""
    cases = []
    for exponent in range(27, 50):
        n = 1 << exponent
        a = (3, -5)
        b = (3 + n, -5 + n - 1)
        c = (3 + n + 1, -5 + n)
        cases.append((a, b, c))
        cases.append((a, c, b))
    return cases


def family_degenerate() -> list[tuple[tuple[int, int], ...]]:
    return [((1, -1), (2, 2), (3, 5)), ((7, 5), (7, 5), (4, 3))]


def measure(label: str, cases: list[tuple[tuple[int, int], ...]]) -> dict[str, int]:
    naive_wrong = 0
    fallbacks = 0
    filtered_wrong = 0
    for triangle in cases:
        expected = oracle(*triangle)
        direction, used_exact = classify(*triangle)
        if direction != expected:
            filtered_wrong += 1
        if used_exact:
            fallbacks += 1
        if naive(*triangle) != expected:
            naive_wrong += 1
    return {
        "cases": len(cases),
        "naive_wrong": naive_wrong,
        "fallbacks": fallbacks,
        "filtered_wrong": filtered_wrong,
    }


def measure_controls() -> list[tuple[str, str, int, int]]:
    """Return ``(control, outcome, sign_only_would_apply, correct)`` rows."""
    rows = []
    base = ((3, -5), (3 + (1 << 27), -5 + (1 << 27) - 1), (3 + (1 << 27) + 1, -5 + (1 << 27)))
    certifier = Certifier()

    # Positive control first: the honest path must commit.
    mesh = Mesh(1, list(base))
    witness = certifier.observe(mesh, (0, 1, 2))
    mesh.commit((0, 1, 2), witness)
    rows.append(("positive: same triple, same revision", "accepted", witness.direction, witness.direction))

    # 1. wrong vertex order
    mesh = Mesh(2, list(base))
    witness = certifier.observe(mesh, (0, 1, 2))
    correct = oracle(base[0], base[2], base[1])
    try:
        mesh.commit((0, 2, 1), witness)
        outcome = "ACCEPTED (bug)"
    except StaleWitness:
        outcome = "rejected"
    rows.append(("wrong vertex order", outcome, witness.direction, correct))

    # 2. coordinate edit, revision bumped
    mesh = Mesh(3, list(base))
    witness = certifier.observe(mesh, (0, 1, 2))
    mesh.edit_point(2, (base[2][0], base[2][1] - 1))
    correct = oracle(base[0], base[1], (base[2][0], base[2][1] - 1))
    try:
        mesh.commit((0, 1, 2), witness)
        outcome = "ACCEPTED (bug)"
    except StaleWitness:
        outcome = "rejected"
    rows.append(("coordinate edit (revision bumped)", outcome, witness.direction, correct))

    # 3. different mesh, same revision, same coordinates
    mesh = Mesh(4, list(base))
    other = Mesh(5, list(base))
    witness = certifier.observe(mesh, (0, 1, 2))
    correct = oracle(*base)
    try:
        other.commit((0, 1, 2), witness)
        outcome = "ACCEPTED (bug)"
    except StaleWitness:
        outcome = "rejected"
    rows.append(("different mesh, same revision", outcome, witness.direction, correct))

    # 4. forgery: build the dataclass directly
    mesh = Mesh(6, list(base))
    try:
        Turn(mesh=6, revision=1, triple=(0, 1, 2), direction=1, coords_digest="0" * 16, _key=object())
        outcome = "ACCEPTED (bug)"
    except ForgedWitness:
        outcome = "rejected"
    rows.append(("forged witness (direct construction)", outcome, 1, oracle(*base)))

    # 5. coordinate edit that does NOT bump the revision (adversarial internal write)
    mesh = Mesh(7, list(base))
    witness = certifier.observe(mesh, (0, 1, 2))
    mesh._points[2] = (base[2][0], base[2][1] - 1)  # noqa: SLF001 - simulates a bypass
    correct = oracle(base[0], base[1], mesh._points[2])
    try:
        mesh.commit((0, 1, 2), witness)
        outcome = "ACCEPTED (bug)"
    except StaleWitness:
        outcome = "rejected"
    rows.append(("coordinate edit without revision bump", outcome, witness.direction, correct))

    return rows


def main() -> None:
    families = [
        ("random", family_random(10_000, 1 << 26, 1729)),
        ("unit determinant", family_unit_determinant()),
        ("collinear/repeated", family_degenerate()),
    ]
    print(f"{'family':<20}{'cases':>7}{'naive_wrong':>13}{'fallbacks':>11}{'filtered_wrong':>16}")
    for label, cases in families:
        row = measure(label, cases)
        print(
            f"{label:<20}{row['cases']:>7}{row['naive_wrong']:>13}"
            f"{row['fallbacks']:>11}{row['filtered_wrong']:>16}"
        )

    # Exactness applied after rounding cannot recover the lost bit.
    hard = family_unit_determinant()
    rounded_wrong = sum(1 for t in hard if exact_from_rounded(*t) != oracle(*t))
    print(f"\nrounded-then-exact, unit-determinant family: wrong {rounded_wrong}/{len(hard)}")

    print(f"\n{'control':<40}{'outcome':<18}{'sign-only':>11}{'correct':>9}")
    for name, outcome, applied, correct in measure_controls():
        print(f"{name:<40}{outcome:<18}{applied:>11}{correct:>9}")


if __name__ == "__main__":
    main()
