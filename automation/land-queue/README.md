# Land queue — fix bundles the autoland pipeline lands autonomously

Each subdirectory is one fix bundle. The `autoland` workflow
(`.github/workflows/autoland.yml`) verifies it end-to-end and lands it with
no human awake — at `SHEPHERD_MODE=live` only (the repo variable is the kill
switch; the default is dry-run, which executes baseline checks and lands
nothing).

## Bundle format

```
automation/land-queue/<slug>/
  meta.json   {"issue": <number>, "expected_code": "HYB-Exxxx",
               "basis": "<one line: where the fix came from>"}
  fix.patch   raw git-format diff: the fix + its tests (+ corpus fixture if
              one belongs in tests/checker-corpus/ — register it inside the
              patch: fixture + VERDICTS.md + README.md + census surfaces)
  repro.hy    the issue's minimal repro — the PRE-FIX tree must ACCEPT it
```

## The gates (all must pass; any failure = BLOCKED, nothing lands)

1. **Baseline** — the unpatched tree ACCEPTs the repro (a REJECT means the
   issue is stale or already fixed; the bundle is never applied).
2. **Patch** — `git apply` succeeds; the patched tree REJECTs the repro with
   the bundle's `expected_code` (an ACCEPT means the patch is ineffective).
3. **Battery** — `cargo test --workspace`, corpus harness, census,
   differential loop FULL.
4. **Land-assert** — the push is confirmed by `git ls-remote` (origin/main
   must contain the landing before anything is reported landed).
5. **Record** — `meta.json` flips to `status: landed` + the landing sha;
   the shepherd change log (`logs/SESSION.jsonl` on the
   `automation/session-log` branch) records the landing or the block with
   the raw evidence.

## Trust boundaries (honest, stated)

- **What is autonomous:** verification, battery, landing, push-assert,
  issue close (via `Fixes #N`), logging. A prepared bundle lands itself.
- **What is not:** authoring the fix and its tests. That stays with the
  agent sessions (and humans) — a wrong checker rule that passes its own
  repro can still over-reject legal programs; the battery + census catch
  the mechanical part, judgment stays human-reviewed in the session.
- **A stale or ineffective bundle never pushes.** Failure verdicts land in
  the change log with the same evidence discipline as landings.
