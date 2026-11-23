"""Spike for wildcard proposal #4644: let the caller's observation decide completion.

A producer publishes a shrinking interval of possible answers instead of one value.
A caller asking a yes/no question may stop once every value in the interval gives the
same answer; a caller that needs the number keeps counting.

Model, no backend. The claim under test is a soundness claim, so it is tested that way:
for every small tile set, every completion is checked against the true answers of all
values still allowed by the interval. Completion while two allowed values disagree is a
failure, not a slowdown.
"""

from __future__ import annotations

import dataclasses
import itertools
from typing import Iterable, List, Sequence, Tuple

EXACT, ATLEAST, BELOW = "exact", "atleast", "below"

E_INVERTED = "HYB-E4644-01"     # lo > hi on a tile
E_NEGATIVE = "HYB-E4644-02"     # negative count
E_UNSOUND = "HYB-E4644-03"      # completed although the answer is still open


class ClaimError(Exception):
    def __init__(self, code: str, detail: str = "") -> None:
        super().__init__(f"{code}: {detail}" if detail else code)
        self.code = code


@dataclasses.dataclass(frozen=True)
class Tile:
    """One disjoint piece of the image. `lo`/`hi` bound its clipped-pixel count."""

    lo: int
    hi: int

    def __post_init__(self) -> None:
        if self.lo < 0 or self.hi < 0:
            raise ClaimError(E_NEGATIVE, f"{self.lo}..{self.hi}")
        if self.lo > self.hi:
            raise ClaimError(E_INVERTED, f"{self.lo}..{self.hi}")


@dataclasses.dataclass(frozen=True)
class Claim:
    kind: str
    k: int = 0

    def answer(self, total: int) -> bool:
        if self.kind == EXACT:
            return True
        if self.kind == ATLEAST:
            return total >= self.k
        if self.kind == BELOW:
            return total < self.k
        raise ClaimError(E_UNSOUND, self.kind)

    @property
    def needs_number(self) -> bool:
        return self.kind == EXACT


@dataclasses.dataclass
class Count:
    tiles: List[Tile]

    def bounds(self) -> Tuple[int, int]:
        return sum(t.lo for t in self.tiles), sum(t.hi for t in self.tiles)

    def refine(self, index: int, lo: int, hi: int) -> None:
        old = self.tiles[index]
        if max(old.lo, lo) > min(old.hi, hi):
            raise ClaimError(E_INVERTED, f"[{old.lo}..{old.hi}] vs [{lo}..{hi}]")
        new = Tile(max(old.lo, lo), min(old.hi, hi))
        if new.lo > old.lo or new.hi < old.hi:
            self.tiles[index] = new

    def complete(self, claim: Claim) -> bool:
        lo, hi = self.bounds()
        if claim.kind == ATLEAST:
            return lo >= claim.k
        if claim.kind == BELOW:
            return hi < claim.k
        return lo == hi

    def answer(self, claim: Claim) -> bool:
        """Guaranteed answer for a completed claim; open claims are unsound to answer."""
        lo, hi = self.bounds()
        if not self.complete(claim):
            raise ClaimError(E_UNSOUND, f"{claim.kind}{claim.k} on {lo}..{hi}")
        if claim.kind == EXACT:
            return True
        return claim.answer(lo)

    def point_value(self) -> int:
        lo, hi = self.bounds()
        if lo != hi:
            raise ClaimError(E_UNSOUND, "not a point interval")
        return lo


def admitted_values(claim: Claim, tiles: Sequence[Tile]) -> List[int]:
    """Every total the interval still allows, one representative per value."""
    seen: List[int] = []
    for combo in itertools.product(*[range(t.lo, t.hi + 1) for t in tiles]):
        total = sum(combo)
        if not seen or seen[-1] != total:
            seen.append(total)
    return seen


def check_sound(claim: Claim, tiles: Sequence[Tile]) -> Tuple[bool, bool, str]:
    """(completes, sound, detail) for one claim over one tile set."""
    count = Count(list(tiles))
    if not count.complete(claim):
        return False, True, "open"
    values = admitted_values(claim, tiles)
    answers = {claim.answer(v) for v in values}
    if len(answers) > 1:
        return True, False, f"completed with both answers over {values}"
    return True, True, f"answered {answers.pop()} over {len(values)} values"


# --- exhaustive sweep -------------------------------------------------------

ALPHABET = [Tile(0, 0), Tile(1, 1), Tile(0, 1), Tile(2, 2), Tile(0, 2)]


def sweep(fn) -> Tuple[int, int]:
    """Run fn over every 1..3 tile combination of the alphabet: (cases, failures)."""
    cases = failures = 0
    for n in (1, 2, 3):
        for combo in itertools.product(ALPHABET, repeat=n):
            for k in (1, 2, 3, 4):
                for claim in (Claim(ATLEAST, k), Claim(BELOW, k), Claim(EXACT)):
                    cases += 1
                    ok, detail = fn(claim, list(combo))
                    if not ok:
                        failures += 1
    return cases, failures


def sound_completion(claim, tiles):
    completes, sound, _ = check_sound(claim, tiles)
    return (not completes) or sound, ""


def midpoint_completion(claim, tiles):
    """The naive rule: stop as soon as the midpoint answers, even if the range is open."""
    count = Count(tiles)
    lo, hi = count.bounds()
    mid = (lo + hi) // 2
    if claim.needs_number:
        return lo == hi, ""
    answered = claim.answer(mid)
    values = admitted_values(claim, tiles)
    return all(claim.answer(v) == answered for v in values), ""


MONOTONE = [Tile(0, 1), Tile(1, 2), Tile(0, 2)]


def monotone_sweep() -> Tuple[int, int]:
    """Refining a tile must never widen the bound of the whole count."""
    cases = bad = 0
    for n in (1, 2, 3):
        for combo in itertools.product(MONOTONE, repeat=n):
            for i, t in enumerate(combo):
                for lo, hi in ((t.lo, t.hi), (t.hi, t.hi), (0, t.hi)):
                    cases += 1
                    count = Count(list(combo))
                    before = count.bounds()
                    count.refine(i, lo, hi)
                    after = count.bounds()
                    if after[0] < before[0] or after[1] > before[1]:
                        bad += 1
    return cases, bad


if __name__ == "__main__":
    cases, failures = sweep(sound_completion)
    print(f"sound completion: {cases} cases, {failures} failures")
    nc, nf = sweep(midpoint_completion)
    print(f"midpoint rule:    {nc} cases, {nf} unsound completions")
    mc, mb = monotone_sweep()
    print(f"refinement:       {mc} refinements, {mb} widenings")
    ex = Count([Tile(0, 10 ** 6)])
    print("exact consumer completes early:", ex.complete(Claim(EXACT)))
    print("atleast 500 with a wide tile:", ex.complete(Claim(ATLEAST, 500)))
