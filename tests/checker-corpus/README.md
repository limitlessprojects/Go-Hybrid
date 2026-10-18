# tests/checker-corpus — adversarial programs with pre-registered verdicts

**Current registered corpus: 77 programs** — ACCEPT 21 · ACCEPT-MANAGED 2 ·
REJECT 54 — each with a machine-readable `// EXPECTED-VERDICT:` header and a
row in [VERDICTS.md](VERDICTS.md). The 18 originally registered here were
written BEFORE the Phase-1 checker existed; that registration grew to 20
during the 2026-10-12 integration, and every case after it was added
additively, still hand-derived from the rule text before it was run.

Two additive registrations are worth naming because they moved cases around.
The 2026-10-14 select-send registration (mutable-map rejection, a positive
string/channel control, and fail-closed rejection of an unresolved name)
landed as corpus **32–34** after a renumber, and a further case pinned the
documented default-arm syntax and its nonblocking fallback printing exactly
`idle`. Those four files were initially numbered 21–24 and then 25–28 on the
contributor's branch; the 2026-10-15 maintainer integration renumbered them
to 32–35 (slots 25–31 were taken by the audit/PR wave) without changing
their expected results. Their expected results were recorded before running
the new fixtures.

The prose below is the original pre-registration statement, kept as written
on its date: the corpora sizes it names (18, 20) are what the corpus held
then, not today's 77. The dated tripwire baseline is preserved as historical
for the same reason.

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

`78-shadowed-unused-import-reject.hy` — filacocta (PR #535): an explicit
import whose only apparent use names a shadowing local must still reject
as HYB-E1002 (the emitted Go would carry an unused import). Behavior
landed on main via the #475/#430 buildable work; the fixture pins the
surface with author credit (tests extracted to
`hy-check/tests/import_shadowing_guards.rs`).

With this registration the corpus is **78 programs: ACCEPT 21 ·
ACCEPT-MANAGED 2 · REJECT 55**.
