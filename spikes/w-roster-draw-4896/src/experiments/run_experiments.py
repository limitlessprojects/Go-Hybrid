"""Run every experiment reported in RESULTS.md and dump the raw numbers.

Usage (from the project root):

    python src/experiments/run_experiments.py

Writes ``out/results.json`` and ``out/tables.md``.  Standard library only,
fixed seeds, no network.
"""

from __future__ import annotations

import gc
import hashlib
import json
import math
import random
import sys
import time
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "src"))

from hybrid_roster.determinize import PathCounter, SetAutomaton
from hybrid_roster.oracle import (
    brute_product_pairs,
    enumerate_pairs,
    multiplicity_histogram,
    totals,
)
from hybrid_roster.policy import (
    POLICY_A,
    POLICY_A_SPLIT,
    POLICY_B,
    Policy,
    scale_policy,
)
from hybrid_roster.samplers import (
    Determinized,
    PairsUniform,
    PathUniform,
    RosterThenHandoff,
)
from hybrid_roster.stats import chi2_gof, conditional_entropy, risk_ratio_ci, wilson

MASTER = 20261128
DRAWS = 100_000
TARGETS = ("ABABABAB", "ABACACBA", "ABABACAC", "ABACABAB")


def seed_for(config_id: str) -> int:
    digest = hashlib.sha256(f"{MASTER}|{config_id}".encode()).hexdigest()
    return int(digest[:16], 16)


def rng_for(config_id: str) -> random.Random:
    return random.Random(seed_for(config_id))


def timed(fn):
    """Run ``fn`` and return ``(value, seconds)`` with a clean heap."""
    gc.collect()
    t0 = time.perf_counter()
    value = fn()
    return value, time.perf_counter() - t0


def build_counted(policy: Policy, compact: bool = False):
    """Build the automaton and pay for the count table; timings wrap this."""
    automaton = SetAutomaton(policy, compact=compact)
    return automaton, automaton.denominator


# --------------------------------------------------------------------------- H1
def h1_independent_enumeration() -> dict:
    """Oracle at n=8, a third brute-force check, and a full length sweep 0..9."""
    policy = POLICY_A
    pairs, oracle_seconds = timed(lambda: enumerate_pairs(
        policy.alphabet, policy.horizon, policy.caps,
        policy.handoff, policy.require_handoff))
    rosters, pairs_total = totals(pairs)
    hist = multiplicity_histogram(pairs)

    truth, _ = timed(lambda: brute_product_pairs(
        policy.alphabet, policy.horizon, policy.caps,
        policy.handoff, policy.require_handoff))

    sweep = []
    all_ok = True
    for length in range(0, 10):
        small = Policy(name=f"A/n={length}", alphabet=policy.alphabet,
                       horizon=length, caps=policy.caps, handoff=policy.handoff)
        automaton = SetAutomaton(small)
        oracle = enumerate_pairs(small.alphabet, length, small.caps,
                                 small.handoff, small.require_handoff)
        decoded = [automaton.decode(r) for r in range(automaton.denominator)]
        ok = (automaton.denominator == len(oracle)
              and decoded == sorted(oracle)
              and all(automaton.rank_of(w) == r for r, w in enumerate(decoded)))
        all_ok = all_ok and ok
        sweep.append({"n": length, "oracle": len(oracle),
                      "automaton": automaton.denominator,
                      "pairs": sum(len(b) for b in oracle.values()),
                      "agree": ok})

    return {
        "rosters": rosters,
        "pairs": pairs_total,
        "histogram": hist,
        "oracle_seconds": round(oracle_seconds, 6),
        "product_agrees": truth == pairs,
        "sweep": sweep,
        "sweep_agree": all_ok,
        "examples": {w: list(pairs[w]) for w in TARGETS},
        "issue_claim": {"rosters": 151, "pairs": 284,
                        "histogram": {"1": 43, "2": 84, "3": 23, "4": 1}},
    }


# --------------------------------------------------------------------------- H2
def describe_sample(name: str, sampler, rosters: list[str],
                    multiplicities: dict[str, int], draws: int = DRAWS) -> dict:
    """Draw ``draws`` rosters and reduce the sample to the reported statistics."""
    rng = rng_for(name)
    counts: Counter = Counter()
    joint: Counter = Counter()

    def sample():
        for _ in range(draws):
            word, handoff = sampler.draw(rng)
            counts[word] += 1
            joint[(word, handoff)] += 1

    _, elapsed = timed(sample)

    n = draws
    k = len(rosters)
    observed = {w: counts.get(w, 0) for w in rosters}
    stat_u, df_u, p_u = chi2_gof(observed, 1 / k)
    mass = sum(multiplicities[w] for w in rosters)
    stat_m, _, p_m = chi2_gof(observed, {w: n * multiplicities[w] / mass for w in rosters})

    k1, k2 = counts.get(TARGETS[0], 0), counts.get(TARGETS[1], 0)
    rr, lo, hi = risk_ratio_ci(k1, n, k2, n)

    emp_h = conditional_entropy({key: c / n for key, c in joint.items()})

    # exact law implied by the sampler's own construction (no sampling noise).
    # Weights are the sampler's *own* roster probabilities, so the exact and the
    # empirical value must agree; a uniform-roster weighting would silently
    # disagree for the multiplicity-proportional samplers.
    exact_h = 0.0
    exact_same = 0.0
    for w in rosters:
        law = sampler.reveal_distribution(w)
        pw = sampler.roster_prob(w)
        exact_h += pw * (-sum(p * math.log2(p) for p in law.values()))
        exact_same += pw * sum(p * p for p in law.values())

    expected = n / k
    worst = max(observed.items(), key=lambda kv: abs(kv[1] - expected))
    outside = 0
    for w in rosters:
        lo_w, hi_w = wilson(observed[w], n, 3.29)  # 99.9%
        if not (lo_w <= 1 / k <= hi_w):
            outside += 1

    return {
        "config": name,
        "sampler": sampler.kind,
        "draws": n,
        "space_size": sampler.denominator,
        "seconds": round(elapsed, 3),
        "target_counts": {w: counts.get(w, 0) for w in TARGETS},
        "chi2_uniform": round(stat_u, 3),
        "df_uniform": df_u,
        "p_uniform": p_u,
        "chi2_multiplicity_model": round(stat_m, 3),
        "p_multiplicity_model": p_m,
        "max_abs_dev_vs_uniform": round(abs(worst[1] - expected) / n, 6),
        "worst_roster": worst[0],
        "rosters_outside_999_wilson": outside,
        "expected_sd_count": round(math.sqrt(expected * (1 - 1 / k)), 3),
        "freq_ABABABAB": k1 / n,
        "freq_ABACACBA": k2 / n,
        "ratio_ABABABAB_over_ABACACBA": round(rr, 4),
        "ratio_ci95": [round(lo, 4), round(hi, 4)],
        "entropy_reveal_exact": round(exact_h, 6),
        "entropy_reveal_empirical": round(emp_h, 6),
        "repeat_agreement_exact": round(exact_same, 6),
    }


def h2_samplers() -> dict:
    policy = POLICY_A
    pairs = enumerate_pairs(policy.alphabet, policy.horizon, policy.caps,
                            policy.handoff, policy.require_handoff)
    rosters = sorted(pairs)
    multiplicities = {w: len(b) for w, b in pairs.items()}

    out = {}
    out["impl1_pairs"] = describe_sample(
        "H2/impl1/pairs", PairsUniform(policy), rosters, multiplicities)
    out["impl1b_paths"] = describe_sample(
        "H2/impl1b/pathcount", PathUniform(policy), rosters, multiplicities)
    out["impl2_boundary"] = describe_sample(
        "H2/impl2/roster-then-boundary", RosterThenHandoff(policy, "boundary"),
        rosters, multiplicities)
    out["impl2_run"] = describe_sample(
        "H2/impl2/roster-then-run", RosterThenHandoff(policy, "run"),
        rosters, multiplicities)
    out["impl3_det"] = describe_sample(
        "H2/impl3/determinized", Determinized(policy), rosters, multiplicities)

    # post-hoc: the two independent pair-space samplers draw from the same law, so
    # pooling their 2 x 100k draws gives a tighter estimate of the 4:1 ratio than
    # either single run (each has only ~350 ABACACBA hits).
    k1 = (out["impl1_pairs"]["target_counts"]["ABABABAB"]
          + out["impl1b_paths"]["target_counts"]["ABABABAB"])
    k2 = (out["impl1_pairs"]["target_counts"]["ABACACBA"]
          + out["impl1b_paths"]["target_counts"]["ABACACBA"])
    n = 2 * DRAWS
    rr, lo, hi = risk_ratio_ci(k1, n, k2, n)
    out["posthoc_pooled_pair_ratio"] = {
        "note": "post-hoc: impl1 + impl1b pooled, 200,000 draws total",
        "counts": {"ABABABAB": k1, "ABACACBA": k2},
        "ratio": round(rr, 4),
        "ci95": [round(lo, 4), round(hi, 4)],
        "covers_4": lo < 4.0 < hi,
    }
    return out


# --------------------------------------------------------------------------- H3
def h3_contract() -> dict:
    plain, split = POLICY_A, POLICY_A_SPLIT
    a_plain, a_split = SetAutomaton(plain), SetAutomaton(split)
    pairs = enumerate_pairs(plain.alphabet, plain.horizon, plain.caps,
                            plain.handoff, plain.require_handoff)
    rosters = sorted(pairs)
    multiplicities = {w: len(b) for w, b in pairs.items()}
    pc_plain, pc_split = PathCounter(plain), PathCounter(split)

    det_plain = [a_plain.decode(r) for r in range(a_plain.denominator)]
    det_split = [a_split.decode(r) for r in range(a_split.denominator)]

    # H3a: the determinised draw must be invariant under the refactor
    sample_plain = describe_sample("H3/det-plain", Determinized(plain),
                                   rosters, multiplicities)
    sample_split = describe_sample("H3/det-split", Determinized(split),
                                   rosters, multiplicities)
    ratio = risk_ratio_ci(sample_plain["target_counts"]["ABABACAC"], DRAWS,
                          sample_split["target_counts"]["ABABACAC"], DRAWS)

    # H3b: path counting must be falsified by the same refactor
    path_plain = describe_sample("H3/path-plain", PathUniform(plain),
                                 rosters, multiplicities)
    path_split = describe_sample("H3/path-split", PathUniform(split),
                                 rosters, multiplicities)
    rr_path, lo_path, hi_path = risk_ratio_ci(
        path_plain["target_counts"]["ABABACAC"], DRAWS,
        path_split["target_counts"]["ABABACAC"], DRAWS)

    # H3c: conditional reveal law for one roster, both modes, both policies.
    # Two chi-squares per cell: the sample against its *own* law (a sanity check
    # that the sampler implements its declared law) and against the law of the
    # pre-refactor policy (the actual leak test).
    word = "ABACABAB"
    reveal = {}
    for mode in ("boundary", "run"):
        sampler_plain = RosterThenHandoff(plain, mode)
        sampler_split = RosterThenHandoff(split, mode)
        law_plain = sampler_plain.reveal_distribution(word)
        law_split = sampler_split.reveal_distribution(word)
        for tag, sampler, own, other in (
            ("plain", sampler_plain, law_plain, law_split),
            ("split", sampler_split, law_split, law_plain),
        ):
            label = f"impl2_{mode}_{tag}"
            rng = rng_for(f"H3/reveal/{label}")
            counts: Counter = Counter()

            def sample():
                for _ in range(DRAWS):
                    counts[sampler.sample_reveal(word, rng)] += 1

            _, elapsed = timed(sample)
            if sum(counts.values()) != DRAWS:  # guards the conditional-sampling bug
                raise AssertionError(f"{label}: {sum(counts.values())} != {DRAWS} reveals")
            keys = sorted(own)
            stat, _, p = chi2_gof({k: counts.get(k, 0) for k in keys},
                                  {k: DRAWS * own[k] for k in keys})
            keys_o = sorted(other)
            stat_o, _, p_o = chi2_gof({k: counts.get(k, 0) for k in keys_o},
                                      {k: DRAWS * other[k] for k in keys_o})
            reveal[label] = {
            "exact_law": {str(k): own[k] for k in keys},
            "observed": {str(k): counts.get(k, 0) for k in keys},
            "observed_total": sum(counts.values()),
            "chi2_vs_exact": round(stat, 3),
            "p_vs_exact": p,
            "chi2_vs_other_policy_law": round(stat_o, 3),
            "p_vs_other_policy_law": p_o,
            "seconds": round(elapsed, 3),
        }
    det_law = Determinized(split).reveal_distribution(word)
    reveal["impl3_canonical_split"] = {
        "exact_law": {str(k): v for k, v in det_law.items()},
        "observed": None,
    }

    return {
        "visible_count_plain": a_plain.denominator,
        "visible_count_split": a_split.denominator,
        "path_count_plain": pc_plain.denominator,
        "path_count_split": pc_split.denominator,
        "decode_identical": det_plain == det_split,
        "decoded_rosters": a_plain.denominator,
        "per_roster_paths": {w: [pc_plain.paths_of(w), pc_split.paths_of(w)]
                             for w in TARGETS},
        "det_plain": sample_plain,
        "det_split": sample_split,
        "det_ratio_ABABACAC": [round(ratio[0], 4), round(ratio[1], 4), round(ratio[2], 4)],
        "path_plain": path_plain,
        "path_split": path_split,
        "path_ratio_ABABACAC": [round(rr_path, 4), round(lo_path, 4), round(hi_path, 4)],
        "reveal_conditional": reveal,
    }


# --------------------------------------------------------------------------- H4
def h4_scale() -> dict:
    # warm the interpreter so the first timed row is not measuring import cost
    SetAutomaton(scale_policy(8), compact=True).denominator
    SetAutomaton(scale_policy(8)).denominator

    # post-hoc: compact counting state (only "handoff chosen?" kept), pushed far
    compact_rows = []
    for n in (8, 12, 16, 20, 24, 32, 48, 64):
        policy = scale_policy(n)
        (automaton, total), elapsed = timed(lambda p=policy: build_counted(p, True))
        compact_rows.append({
            "n": n,
            "visible_words": total,
            "bits": total.bit_length(),
            "seconds": round(elapsed, 4),
            "sets": automaton.distinct_sets(),
            "count_entries": len(automaton._count),
            "peak_members": automaton.peak_members,
            "candidate_space": policy.words(),
        })

    rows = []
    # n=32 with the age-carrying state needs ~400k frozensets; on this machine it
    # completes in tens of seconds but also OOMs on some runs, so the table stops
    # at 24 and the dedicated n=32 attempt lives in measure_full_n32.py.
    for n in (8, 10, 12, 16, 20, 24):
        policy = scale_policy(n)
        (automaton, total), elapsed = timed(lambda p=policy: build_counted(p))
        (compact, compact_total), compact_elapsed = timed(
            lambda p=policy: build_counted(p, True))
        rows.append({
            "n": n,
            "visible_words": total,
            "bits": total.bit_length(),
            "seconds": round(elapsed, 4),
            "distinct_sets": automaton.distinct_sets(),
            "count_entries": len(automaton._count),
            "delta_entries": len(automaton._delta),
            "peak_members": automaton.peak_members,
            "candidate_space": policy.words(),
            "compact_seconds": round(compact_elapsed, 4),
            "compact_sets": compact.distinct_sets(),
            "compact_count_entries": len(compact._count),
            "compact_peak_members": compact.peak_members,
            "same_count": total == compact_total,
        })

    crosscheck = []
    for n in (6, 8, 10):
        policy = scale_policy(n)
        automaton = SetAutomaton(policy)
        oracle = enumerate_pairs(policy.alphabet, n, policy.caps,
                                 policy.handoff, policy.require_handoff)
        crosscheck.append({"n": n, "automaton": automaton.denominator,
                           "oracle": len(oracle),
                           "pairs": sum(len(b) for b in oracle.values()),
                           "agree": automaton.denominator == len(oracle)})

    # policy B: the 12-shift, four-volunteer instance fixed in PREREG
    b = POLICY_B
    (ab, b_total), b_seconds = timed(lambda: build_counted(b))
    (bc, b_compact_total), b_compact_seconds = timed(
        lambda: build_counted(b, True))
    b_oracle, b_oracle_seconds = timed(lambda: enumerate_pairs(
        b.alphabet, b.horizon, b.caps, b.handoff, b.require_handoff))

    # single-draw cost on the n=32 instance, using the compact counting state
    big = SetAutomaton(scale_policy(32), compact=True)
    rng = rng_for("H4/draw32")
    total_big = big.denominator
    _, decode_seconds = timed(lambda: [big.decode(rng.randrange(total_big))
                                       for _ in range(2_000)])

    return {
        "family": rows,
        "compact_family": compact_rows,
        "crosscheck": crosscheck,
        "policy_b": {
            "name": b.name,
            "visible_words": b_total,
            "pairs": sum(len(x) for x in b_oracle.values()),
            "histogram": multiplicity_histogram(b_oracle),
            "seconds": round(b_seconds, 4),
            "compact_seconds": round(b_compact_seconds, 4),
            "compact_visible_words": b_compact_total,
            "oracle_seconds": round(b_oracle_seconds, 4),
            "distinct_sets": ab.distinct_sets(),
            "count_entries": len(ab._count),
            "compact_sets": bc.distinct_sets(),
            "crosscheck_agree": ab.denominator == len(b_oracle),
        },
        "decode_microseconds_n32": round(decode_seconds / 2_000 * 1e6, 2),
    }


# --------------------------------------------------------------------------- main
def main() -> None:
    started = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    results = {
        "meta": {
            "issue": "limitlessprojects/Go-Hybrid#4896",
            "master_seed": MASTER,
            "draws_per_config": DRAWS,
            "python": sys.version.split()[0],
            "started": started,
        },
        "H1": h1_independent_enumeration(),
    }
    print("H1:", results["H1"]["rosters"], results["H1"]["pairs"],
          results["H1"]["histogram"], "sweep_ok =", results["H1"]["sweep_agree"])

    results["H2"] = h2_samplers()
    for key, row in results["H2"].items():
        if "config" not in row:
            continue
        print(f"H2 {key}: chi2_uniform={row['chi2_uniform']} p={row['p_uniform']:.3g} "
              f"ratio={row['ratio_ABABABAB_over_ABACACBA']} "
              f"H(reveal)={row['entropy_reveal_exact']} "
              f"agree={row['repeat_agreement_exact']}")
    print("H2 pooled ratio:", results["H2"]["posthoc_pooled_pair_ratio"])

    results["H3"] = h3_contract()
    print("H3: decode_identical =", results["H3"]["decode_identical"],
          "det_ratio =", results["H3"]["det_ratio_ABABACAC"],
          "path_ratio =", results["H3"]["path_ratio_ABABACAC"])

    results["H4"] = h4_scale()
    for row in results["H4"]["family"]:
        print(f"H4 n={row['n']:>2} words={row['visible_words']} bits={row['bits']} "
              f"t={row['seconds']}s sets={row['distinct_sets']} "
              f"| compact t={row['compact_seconds']}s sets={row['compact_sets']} "
              f"same={row['same_count']}")
    for row in results["H4"]["compact_family"]:
        print(f"H4c n={row['n']:>2} words={row['visible_words']} bits={row['bits']} "
              f"t={row['seconds']}s sets={row['sets']}")

    out = ROOT / "out"
    out.mkdir(exist_ok=True)
    (out / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    (out / "tables.md").write_text(render_tables(results), encoding="utf-8")
    print("wrote", out / "results.json", "and", out / "tables.md")


# ------------------------------------------------------------------------ tables
def fmt_p(p: float) -> str:
    if p < 1e-300:
        return "<1e-300"
    if p < 1e-4:
        return f"{p:.2e}"
    return f"{p:.4f}"


ROW_KEYS = ("config", "space_size", "chi2_uniform", "p_uniform", "target_counts",
            "freq_ABABABAB", "freq_ABACACBA")


def _require(rows, where: str) -> None:
    """Fail with the row name and the missing keys instead of a bare KeyError."""
    for row in rows:
        missing = [k for k in ROW_KEYS if k not in row]
        if missing:
            raise KeyError(f"{where}: row {row.get('config', '<no config>')!r} "
                           f"is missing {missing}")


def render_tables(r: dict) -> str:
    h2_rows = [row for row in r["H2"].values() if "config" in row]
    _require(h2_rows, "H2")
    _require([r["H3"]["det_plain"], r["H3"]["det_split"],
              r["H3"]["path_plain"], r["H3"]["path_split"]], "H3")
    lines = ["# 自动生成的表（run_experiments.py）", ""]

    h1 = r["H1"]
    hist_ok = h1["histogram"] == {1: 43, 2: 84, 3: 23, 4: 1}
    lines += ["## H1 独立枚举 vs issue 声称", "",
              "| 量 | issue 声称 | 本 spike 实测 | 一致 |", "|---|---|---|---|",
              f"| 可见 roster 数 | 151 | {h1['rosters']} | "
              f"{'是' if h1['rosters'] == 151 else '否'} |",
              f"| 合法 (roster,handoff) 对 | 284 | {h1['pairs']} | "
              f"{'是' if h1['pairs'] == 284 else '否'} |",
              "| multiplicity 直方图 | 1×43, 2×84, 3×23, 4×1 | "
              + ", ".join(f"{k}×{v}" for k, v in h1["histogram"].items())
              + f" | {'是' if hist_ok else '否'} |",
              "",
              f"- 第三个实现（全部 3^8 个词 + 过滤）与 DFS 枚举逐项相同：{h1['product_agrees']}",
              f"- 长度 0–9 全部长度上，确定化计数 = 独立穷举数，且 rank 0..N-1 解码 == 字典序列表："
              f"{h1['sweep_agree']}",
              ""]

    lines += ["### 长度 0–9 的逐长度对照", "",
              "| n | oracle roster 数 | 确定化计数 | 合法对 | 逐 rank 解码一致 |",
              "|---|---|---|---|---|"]
    for row in h1["sweep"]:
        lines.append(f"| {row['n']} | {row['oracle']} | {row['automaton']} | "
                     f"{row['pairs']} | {'是' if row['agree'] else '否'} |")
    lines.append("")

    lines += ["## H2 三个实现的抽样偏差（每格 100,000 次，固定种子）", "",
              "| 实现 | 抽样空间 | χ² vs 均匀(151) | p | χ² vs ∝multiplicity | p | "
              "max\\|p̂−1/151\\| | 99.9% CI 外 |", "|---|---|---|---|---|---|---|---|"]
    for row in h2_rows:
        lines.append(
            f"| {row['config']} | {row['space_size']} | {row['chi2_uniform']:.1f} | "
            f"{fmt_p(row['p_uniform'])} | {row['chi2_multiplicity_model']:.1f} | "
            f"{fmt_p(row['p_multiplicity_model'])} | {row['max_abs_dev_vs_uniform']:.5f} | "
            f"{row['rosters_outside_999_wilson']} |")
    lines.append("")

    lines += ["| 实现 | f(ABABABAB) | f(ABACACBA) | 比值 95% CI | H(reveal\\|roster) | "
              "重复抽取 reveal 一致率 |", "|---|---|---|---|---|---|"]
    for row in h2_rows:
        lo, hi = row["ratio_ci95"]
        lines.append(
            f"| {row['config']} | {row['freq_ABABABAB']:.5f} | {row['freq_ABACACBA']:.5f} | "
            f"{row['ratio_ABABABAB_over_ABACACBA']:.3f} [{lo:.3f}, {hi:.3f}] | "
            f"{row['entropy_reveal_exact']:.4f} | {row['repeat_agreement_exact']:.4f} |")
    lines.append("")

    pooled = r["H2"].get("posthoc_pooled_pair_ratio")
    if pooled:
        lines += [f"- **事后合并**（impl1 + impl1b，共 200,000 次，同一精确律）："
                  f"f(ABABABAB)={pooled['counts']['ABABABAB']}，"
                  f"f(ABACACBA)={pooled['counts']['ABACACBA']}，"
                  f"比值 {pooled['ratio']:.3f} "
                  f"[{pooled['ci95'][0]:.3f}, {pooled['ci95'][1]:.3f}]，"
                  f"覆盖 4：{pooled['covers_4']}", ""]

    h3 = r["H3"]
    dp, ds = h3["det_plain"], h3["det_split"]
    pp, ps = h3["path_plain"], h3["path_split"]
    det_ratio, path_ratio = h3["det_ratio_ABABACAC"], h3["path_ratio_ABABACAC"]
    lines += ["## H3 契约测试：同一可见语言，私有分支写两遍", "",
              f"- 可见 roster 数：重构前 {h3['visible_count_plain']}，重构后 "
              f"{h3['visible_count_split']}",
              f"- 接受路径数：重构前 {h3['path_count_plain']}，重构后 {h3['path_count_split']}",
              f"- {h3['decoded_rosters']} 个 rank 的解码结果逐项相同：{h3['decode_identical']}",
              "",
              "| roster | 路径数 重构前 | 路径数 重构后 |", "|---|---|---|"]
    for w, (a, b) in h3["per_roster_paths"].items():
        lines.append(f"| {w} | {a} | {b} |")
    lines += ["",
              "| 实现 | f(ABABACAC) 重构前 | 重构后 | 比值 前/后 (95% CI) |",
              "|---|---|---|---|"]
    lines.append(f"| 确定化 | {dp['target_counts']['ABABACAC'] / DRAWS:.5f} | "
                 f"{ds['target_counts']['ABABACAC'] / DRAWS:.5f} | "
                 f"{det_ratio[0]:.3f} [{det_ratio[1]:.3f}, {det_ratio[2]:.3f}] |")
    lines.append(f"| path-counting | {pp['target_counts']['ABABACAC'] / DRAWS:.5f} | "
                 f"{ps['target_counts']['ABABACAC'] / DRAWS:.5f} | "
                 f"{path_ratio[0]:.3f} [{path_ratio[1]:.3f}, {path_ratio[2]:.3f}] |")
    lines.append("")

    lines += ["### reveal 的条件分布（固定 roster = ABACABAB，各 100,000 次）", "",
              "「vs 自身律」检验采样器有没有照自己声明的律抽；「vs 另一策略律」把同一份样本拿去"
              "对**重构前/后另一版策略**的律做检验（plain 行对 split 律，split 行对 plain 律）——"
              "这就是「等价重构有没有改变可观察的 reveal 分布」的检验。", "",
              "| 变体 | 精确律 | 实测计数 | χ² vs 自身律 | p | χ² vs 另一策略律 | p |",
              "|---|---|---|---|---|---|---|"]
    for label, row in h3["reveal_conditional"].items():
        law = ", ".join(f"{k}:{v:.3f}" for k, v in row["exact_law"].items())
        obs = row.get("observed")
        obs_s = ", ".join(f"{k}:{v}" for k, v in obs.items()) if obs else "确定性（恒为最左边界）"
        stat = f"{row['chi2_vs_exact']:.1f}" if "chi2_vs_exact" in row else "—"
        p = fmt_p(row["p_vs_exact"]) if "p_vs_exact" in row else "—"
        stat_o = (f"{row['chi2_vs_other_policy_law']:.1f}"
                  if "chi2_vs_other_policy_law" in row else "—")
        p_o = fmt_p(row["p_vs_other_policy_law"]) if "p_vs_other_policy_law" in row else "—"
        lines.append(f"| {label} | {law} | {obs_s} | {stat} | {p} | {stat_o} | {p_o} |")
    lines.append("")

    h4 = r["H4"]
    lines += ["## H4 规模曲线（策略 C：ABCD，C≤2, D≤1，相邻不同）", "",
              "「完整状态」= 私有状态含交接年龄；「压缩状态」= 年龄压成布尔（只用于计数，"
              "witness 由单词回放恢复）。两者的可见计数必须相同（`same_count`）。", "",
              "| n | 可见 roster 数 | 位数 | 耗时 完整 (s) | 耗时 压缩 (s) | "
              "子集数 完整 | 子集数 压缩 | count 表项 压缩 | 集合峰值 | 候选词 4^n | 同计数 |",
              "|---|---|---|---|---|---|---|---|---|---|---|"]
    for row in h4["family"]:
        lines.append(
            f"| {row['n']} | {row['visible_words']} | {row['bits']} | {row['seconds']} | "
            f"{row['compact_seconds']} | {row['distinct_sets']} | {row['compact_sets']} | "
            f"{row['compact_count_entries']} | {row['compact_peak_members']} | "
            f"{row['candidate_space']} | {'是' if row['same_count'] else '否'} |")
    fam = h4["family"]
    if len(fam) >= 2:
        first, last = fam[0], fam[-1]
        lines += ["",
                  f"- n={first['n']} → {last['n']}：完整状态耗时 ×{last['seconds'] / first['seconds']:.0f}，"
                  f"压缩状态 ×{last['compact_seconds'] / first['compact_seconds']:.0f}；"
                  f"同期可见 roster 数 ×{last['visible_words'] / first['visible_words']:.0f}，"
                  f"候选词空间 ×{last['candidate_space'] / first['candidate_space']:.3g}"]

    lines += ["", "### 事后诊断：只保留「是否已选交接」的计数状态（非预注册）", "",
              "| n | 可见 roster 数 | 位数 | 计数耗时 (s) | 子集数 | count 表项 | 集合峰值 | 4^n |",
              "|---|---|---|---|---|---|---|---|"]
    for row in h4["compact_family"]:
        lines.append(f"| {row['n']} | {row['visible_words']} | {row['bits']} | "
                     f"{row['seconds']} | {row['sets']} | {row['count_entries']} | "
                     f"{row['peak_members']} | {row['candidate_space']} |")
    cf = h4["compact_family"]
    if len(cf) >= 2:
        lines += ["",
                  f"- 压缩状态：n={cf[0]['n']} → {cf[-1]['n']}，耗时 ×"
                  f"{cf[-1]['seconds'] / cf[0]['seconds']:.2f}，子集数 "
                  f"{cf[0]['sets']} → {cf[-1]['sets']}，可见 roster 数 ×"
                  f"{cf[-1]['visible_words'] / cf[0]['visible_words']:.0f}"]

    lines += ["", "### n ≤ 10 时确定化计数 vs 独立穷举", "",
              "| n | 确定化 | oracle | 合法对 | 一致 |", "|---|---|---|---|---|"]
    for row in h4["crosscheck"]:
        lines.append(f"| {row['n']} | {row['automaton']} | {row['oracle']} | "
                     f"{row['pairs']} | {'是' if row['agree'] else '否'} |")
    b = h4["policy_b"]
    lines += ["", "### 策略 B（12 班、4 人，C≤3, D≤2）", "",
              f"- 可见 roster 数 {b['visible_words']}，合法对 {b['pairs']}，"
              f"multiplicity 直方图 {b['histogram']}",
              f"- 计数耗时 完整 {b['seconds']} s / 压缩 {b['compact_seconds']} s；"
              f"独立穷举耗时 {b['oracle_seconds']} s；"
              f"子集数 {b['distinct_sets']} / {b['compact_sets']}；与穷举一致 {b['crosscheck_agree']}",
              f"- n=32 单次 rank 解码 {h4['decode_microseconds_n32']} µs"]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    main()
