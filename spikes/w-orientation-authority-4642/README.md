# W-4642 spike: certified orientation authority for a mesh edit

- **Issue:** #4642 — *[wildcard] Bind an orientation decision to the vertices it is allowed to change*
- **Proposal credit:** `wd1314zn35-hue`
- **Status:** M2 learning artifact. Verdict and limits: [RESULTS.md](RESULTS.md).

The proposal argues that a robust-predicate sign is not the same thing as *authority* to
apply an edit: the sign can be right while the association between that sign and its input
sites lives only in the caller's head. This spike splits the proposal into its two halves
and tests each one separately.

| Half | File | What it does |
|---|---|---|
| Predicate | `orientation_authority.py` | conservative binary64 filter + exact integer fallback over coordinates bounded by 2^50, with the integer shoelace oracle as ground truth |
| Runtime authority | `orientation_authority.py` | `Turn` witness carrying (mesh, epoch, ordered triple, coordinate digest); `Mesh.commit` is the only write path and checks all four |
| Static authority | `witness_checker.py` | the rule as an indexed type checked intraprocedurally over a tiny Hybrid-shaped IR, including witnesses that cross helper boundaries |
| Cases | `cases/*.hyc` | ten labelled programs, four of them positive controls |
| Opaque-API control | `go-control/` | the proposal's own drop condition: a plain Go library with an unforgeable witness, measured end to end |
| Measurements | `measure.py` | every number quoted in RESULTS.md |

## Reproduction

```bash
python3 -m unittest discover -s spikes/w-orientation-authority-4642/tests -p "test_*.py" -v
python3 spikes/w-orientation-authority-4642/measure.py

cd spikes/w-orientation-authority-4642/go-control
go test ./...
go test ./mesh -run XXX -bench . -benchmem
```

Python 3.10+, standard library only. The Go module is dependency-free (`go 1.22` directive);
any modern toolchain builds it. No network, no compiler crate.

## The rule, in one table

| Construct | Type |
|---|---|
| `observe(m, i, j, k)` | `obs[m@e, (i, j, k)]` |
| `orientation(o)` | `turn[m@e, (i, j, k)]` |
| `insert(m, i, j, k, t)` | requires `t : turn[m@e_now, (i, j, k)]` |
| `edit(m)` | `e -> e+1` |
| `fn f(m: mesh, i, j, k: int) -> turn[m@m, (i, j, k)]` | the only way a witness crosses a helper boundary |

## Verdict, short version

- The predicate reproduces the proposal's own numbers exactly (46/46 unit-determinant
  triangles defeat the naive comparison; the filter falls back 46/46 times).
- The runtime gate rejects all five misuse controls — but only because the witness also
  carries a digest of the classified coordinates. Revision-only invalidation is not
  sufficient, and the proposal currently relies on revision only.
- The static rule needs *dependent* return indices on witness-returning helpers, and a
  new effect check for helpers that `edit` a mesh they were handed (`HYB-E4647`). Neither
  is in the proposal; the second one is a soundness hole in it.
- The proposal's own drop condition now has a measured answer: a plain Go library
  (`go-control/`) reproduces the runtime guarantee with no caller bookkeeping, at ~380 ns
  per commit (94% of it a SHA-256 digest). What it cannot reproduce is the compile-time
  half. That narrows the feature's justification to "static, and closed under code written
  later" — an argument about enforceability, not expressiveness.
