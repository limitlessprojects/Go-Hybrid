# STATUS (for the human — 15 lines max)
- Contributor runner diagnostics: captured timeout output retained; Mini 13/13 runner (baseline 4 failures), 53 Python tooling, 591 Rust, corpus 52/52, differential 52/52. Upstream CI/review pending; no award claimed.

- Phase: **MVP DONE + v0.1.0 released + community OPEN** — session-13: **both P0 queue leaders landed (#306, #374) + the first wildcard spike is race-verified (PR #379, W-015)**
- **#306 landed** (duongynhi000005-oss, fixes #305): if-init eval order aligned with go/spec + same-stmt/package-handle consume & loans (E3003 A1b/A7, E3101 A2, E3102 A3) — 7/7 issue repros reject with exact family codes, 5/5 legal twins accept; loans.rs merge conflict resolved preserving both root_span and main's check_call_const_writes
- **#374 landed** (Anekenonso, fixes **#361 P0**): wrapped-move expressions (`box{m: move m}.m`, `[]map[string]int{move m}[0]`, `(move s)[0:2]`), GC-field moves through wrappers (E3004 O5.4), may-share rebind origin materialization, and const-composite `move` now report precise E3006 — 4/4 previously-accepting shapes now reject; 5/5 positive controls clean
- **W-015 SPIKE (PR #379, branch spike/w015-split-fork-join)**: `split s at mid into left, right { ... }` blocks — 3-index full slice expressions parse; E3105 exclusivity + E3106 split-out; forks are fresh roots (no alias/may_share edges); disjointness = Go's own capacity-clip guarantee, zero runtime
- **Race verification (the M1 evidence)**: emitted Go under `go run -race` x3 — ZERO reports, output `1 14`; corpus 53 + 5 unit pins; VERDICTS.md conflict markers from cbe3f4f resolved (both registration rows kept)
- Suites at close: cargo workspace **596/0** · corpus **53/53** · differential **FULL (16+37)** · main CI green on the #374 merge (8a114b67)
- Wildcard M1 verdicts still due **2026-11-01** in discussion #365 — W-015 now has a running artifact to weigh; W-005 (message isolation) is the recommended next spike
- Wave-2 landing queue continues: #230 (nested closures), #251 (named results), #244, #240/#245, #247, #296, #311/#302, #234, then the const family and emit-lowering batch; new arrivals #366–#377 (sarkermy1125-ship-it tools/fuzz batch), #373 (loop fresh-copy), #364 (docs)
- Standing work: old-queue rebase assists (#67/#88/#93/#106/#197/#198/#99/#223), tools/kaggle batch, CI registration checks (#325/#330/#333)
- Last review gate: G3 PASSED 2026-10-13; community review is a standing input to the queue
