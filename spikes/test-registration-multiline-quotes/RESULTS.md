# Test-registration guard: multiline quoted shell text

Prepared with AI assistance for GitHub account `GogaBabaev`, for consideration
under the useful-improvements track in `BOUNTY.md`. No award is claimed.

## Reproduction

Base: `67ba97f5078408f56edcb403f7b6dcd3db2a06c3`; upstream HEAD still matched
when checked after local validation on 2026-11-27.

The following valid shell script prints two Python invocations. It executes
neither of them:

```sh
printf '%s\n' "
python3 -m unittest discover -s tools -p test_hidden.py
python3 tests/test_hidden.py
"
python3 tests/test_live.py
```

The original guard lexes each physical line independently. It skips the
opening and closing lines because their quotes are unfinished, then credits
both lines inside the string as test registrations. With three fixture files
present, it exits 0 and reports `OK (3 test file(s), 1 discovery pattern(s))`.

The subprocess regression creates two hidden scripts that immediately raise
`RuntimeError('HIDDEN TEST EXECUTED')`, plus a passing live control. `/bin/sh
-eu` exits 0 and prints the commands; executing each hidden script directly
fails with the sentinel. The fixed guard exits 1 and names both hidden files.

This is a static CI-tooling correctness defect, not a compiler soundness
finding. No omitted test in the current upstream workflow is claimed.

## Change and scope

Within each literal `run` block (or raw command snippet), keep incomplete
quoted input together until shell lexing completes. A multiline command is
opaque and cannot provide registrations, preserving the existing single-line
invocation contract. Reset this state at every Actions step boundary.

Regression controls cover single/double quotes, raw snippets/literal workflow
blocks, escaped quotes, comment quotes, real commands after a closing quote,
and independent steps. Existing inline YAML scalar parsing is unchanged.
This does not add a general shell interpreter or solve independent heredoc,
conditional-execution or exit-status-analysis limitations.

Related reports: #596 / #600 addressed single-line comments/echoes; #3297
addresses heredoc payloads, a separate shell construct. Bounded issue/PR
searches for registration with multiline/quoted returned no matching report
of multiline shell strings; novelty remains subject to maintainer review.

## Executed validation

macOS 26.6.2 arm64; Python 3.9.6. No dependencies installed.

```sh
python3 -B -m unittest discover -s tools -p test_check_test_registration.py
python3 -B tools/check_test_registration.py
python3 -B tools/check_conflict_markers.py
git diff --check
```

- New tests against the unmodified guard: 57 methods, 7 failing assertions
  including subcases; no errors. Baseline CLI falsely exits 0.
- Patched suite: 57/57 pass; no errors or skips.
- Live registration guard: OK, 40 test files / 36 discovery patterns.
- Conflict-marker and whitespace checks: clean.
- Full compiler/workspace/corpus/differential suites and hosted Actions were
  not run. Rust is unavailable locally. Hosted CI remains required for merge.

## Reward status

This is a candidate contribution to a competitive prize program, not a fixed
fee task. Acceptance, ranking and payout have not been confirmed. The program
mentions USDC on Base as a preferred candidate rail, not a confirmed transfer.
