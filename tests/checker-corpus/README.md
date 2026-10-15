# tests/checker-corpus — adversarial programs with pre-registered verdicts

38 pseudo-Hybrid (`.hy`) programs written BEFORE the Phase-1 checker exists,
Current registered corpus: **28 programs**. The original pre-checker
registration described below started with 18 and grew to 20 during the
2026-10-12 integration. An additive 2026-10-14 registration in VERDICTS.md
adds the select-send clauses (landed as corpus 32–34 after the renumber): mutable-map rejection, a positive
string/channel control, and fail-closed rejection of an unresolved name.
A further additive registration pins case 28, the documented default-arm
syntax and a nonblocking fallback that prints exactly `idle`.
Their expected results were recorded before running the new fixtures.
These four files were initially numbered 21–24 and then 25–28 on the contributor's branch; the 2026-10-15 maintainer integration renumbered them to 32–35 (slots 25–31 were taken by the audit/PR wave) without changing their expected results.
A further additive 2026-10-15 registration pins cases **36–38**, the wrapper
tail of the call-result alias family (issue #160): a nested-call alias
(`alias := same(same(agg))`) as REJECT, its accept-side false-positive guard as
ACCEPT, and the `append`-within-capacity variant (a literal field holding an
`append` result that shares the source backing array) as REJECT. Expected
results were recorded before the fix; none of the three contains `managed`.
The dated original tripwire baseline below is preserved as historical.

18 pseudo-Hybrid (`.hy`) programs written BEFORE the Phase-1 checker exists,

per the human owner's 2026-10-12 ruling (QUESTIONS.md Q1): the adversarial
programs are written first and treated as the spec. Coverage: the named
tripwire cases (shared map across tasks; worker pool mutating a shared
struct — unguarded, mutex-guarded, managed shapes), the three G0-named
adversarial programs (shared-mutable-map, sender-keeps-using-after-send,
borrow-returned-from-function), rule exercises for A2/A3/A5/A6/A7, and
positive controls guarding against over-strictness (per-task fan-in, clean
`move` sends, atomic/channel/immutable sharing). VERDICTS.md has the table.

Pre-registration discipline: every verdict was hand-derived from the (c)+(b)
sendability ruling and the spike-6 A1–A8 strict readings before any checker
existed. Verdicts change only by logged amendment in VERDICTS.md — never
silently. A checker/corpus disagreement is a bug in one of them: decide
which, record it, never quietly re-run until green.

How the future checker consumes this: each `.hy` header carries one
machine-readable `// EXPECTED-VERDICT:` line (ACCEPT / ACCEPT-MANAGED /
REJECT <rule-id>) plus human-readable reasoning (value layers, check
sites, loan windows). The Phase-1 harness parses the line, runs the
checker, and diffs; for ACCEPT-MANAGED the managed LINT output is part of
the expected result. Contested programs say so in the line's trailing
note, so they can be pinned separately from settled ones.

Tripwire rule: more than ~25% of this corpus needing `managed` reopens G0
(QUESTIONS.md Q1 item 5). Baseline at pre-registration: 11.1%
verdict-level / 16.7% as-written — definitions and recompute rules in
VERDICTS.md §Tripwire baseline.
