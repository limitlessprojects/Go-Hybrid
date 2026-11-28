"""Small statistics helpers (standard library only).

Nothing here is a statistics package; it is the minimum needed to make the
claims in RESULTS.md checkable: a chi-square survival function, a Wilson score
interval, a risk-ratio confidence interval, and entropy.
"""

from __future__ import annotations

import math


# --------------------------------------------------------------- chi-square
def _gammaincc(a: float, x: float) -> float:
    """Regularised upper incomplete gamma Q(a, x) (Numerical Recipes 6.2)."""
    if a <= 0.0 or x < 0.0:
        raise ValueError("gammaincc domain")
    if x == 0.0:
        return 1.0
    if x < a + 1.0:
        return 1.0 - _gser(a, x)
    return _gcf(a, x)


def _gser(a: float, x: float) -> float:
    itmax, eps = 500, 3.0e-16
    ap = a
    total = 1.0 / a
    delta = total
    for _ in range(itmax):
        ap += 1.0
        delta *= x / ap
        total += delta
        if abs(delta) < abs(total) * eps:
            break
    return total * math.exp(-x + a * math.log(x) - math.lgamma(a))


def _gcf(a: float, x: float) -> float:
    itmax, eps, fpmin = 500, 3.0e-16, 1.0e-300
    b = x + 1.0 - a
    c = 1.0 / fpmin
    d = 1.0 / b
    h = d
    for i in range(1, itmax + 1):
        an = -i * (i - a)
        b += 2.0
        d = an * d + b
        if abs(d) < fpmin:
            d = fpmin
        c = b + an / c
        if abs(c) < fpmin:
            c = fpmin
        d = 1.0 / d
        delta = d * c
        h *= delta
        if abs(delta - 1.0) < eps:
            break
    return math.exp(-x + a * math.log(x) - math.lgamma(a)) * h


def chi2_sf(stat: float, df: int) -> float:
    """P(X >= stat) for X ~ chi-square(df)."""
    if df <= 0:
        raise ValueError("df must be positive")
    return _gammaincc(df / 2.0, stat / 2.0)


def chi2_gof(observed: dict, expected: dict | float) -> tuple[float, int, float]:
    """Goodness of fit.  ``expected`` is a total probability per category or a
    mapping ``category -> expected count``.  Returns ``(stat, df, p)``.

    Categories with zero expected count are skipped; categories observed but
    absent from a mapping expectation are an error.
    """
    if isinstance(expected, (int, float)):
        n = sum(observed.values())
        exp = {k: float(expected) * n for k in observed}
    else:
        exp = dict(expected)
    stat = 0.0
    used = 0
    for key, obs in observed.items():
        e = exp[key]
        if e <= 0.0:
            continue
        stat += (obs - e) ** 2 / e
        used += 1
    return stat, max(used - 1, 1), chi2_sf(stat, max(used - 1, 1))


# ------------------------------------------------------------------ intervals
def wilson(k: int, n: int, z: float = 1.96) -> tuple[float, float]:
    """Wilson score interval for a binomial proportion."""
    if n == 0:
        raise ValueError("n must be positive")
    p = k / n
    denom = 1.0 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return max(0.0, centre - half), min(1.0, centre + half)


def risk_ratio_ci(k1: int, n1: int, k2: int, n2: int, z: float = 1.96):
    """Katz log risk-ratio CI (adds 0.5 when a cell is empty)."""
    if min(k1, k2) == 0:
        k1, n1, k2, n2 = k1 + 0.5, n1 + 0.5, k2 + 0.5, n2 + 0.5
    p1, p2 = k1 / n1, k2 / n2
    rr = p1 / p2
    se = math.sqrt(1 / k1 - 1 / n1 + 1 / k2 - 1 / n2)
    return rr, rr * math.exp(-z * se), rr * math.exp(z * se)


# ------------------------------------------------------------------- entropy
def entropy(probs) -> float:
    """Shannon entropy in bits of an iterable of probabilities."""
    total = 0.0
    for p in probs:
        if p > 0:
            total -= p * math.log2(p)
    return total


def conditional_entropy(joint: dict) -> float:
    """``H(Y | X)`` from a ``{(x, y): probability}`` mapping."""
    py: dict = {}
    pxy: dict = {}
    for (x, y), p in joint.items():
        py[x] = py.get(x, 0.0) + p
        pxy[(x, y)] = p
    total = 0.0
    for (x, y), p in pxy.items():
        if p > 0:
            total -= p * math.log2(p / py[x])
    return total
