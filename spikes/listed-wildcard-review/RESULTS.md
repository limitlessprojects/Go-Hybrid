# Results (2026-10-20)

Go 1.26.9 windows/amd64, GOMAXPROCS=2; `go test -v -bench . -benchmem -count=3`. Protocol published first at b5e741830062ee6ce1cf28a4a002c513162249a9, toolchain availability recorded at a1b8bacf21f2671e2e27e0912994655877fcacf9. Full raw results: results.log.

- #997 probe: closure and named value-receiver goroutine spawns both measured 24 B/op and 1 allocation/op in all three benchmark runs; AllocsPerRun likewise measured 1.00 for both. The persistent-worker/channel control measured 0 B/op and 0 allocations/op, but changes scheduling semantics. Thus the proposed emitter rewrite alone does not establish zero allocation per spawned task. This says nothing about payload-only escape freedom; it is not a Hybrid compiler proof or a corpus result. Nanoseconds/op are machine/load-dependent, not a p99 claim.
- #992 probe: the generic zero-sized phantom witness accepted two moves from the same value. Plain Go's type checker did not enforce affine consumption. Additional Hybrid checking or validation remains necessary. This is a negative design control, not an implementation of capability passports.
- #957: effect sandboxing and replay certification remain unimplemented. An effect admission specification and adversarial foreign-Go-call fixtures must precede compiler changes.
- #955: interprocess rings remain unimplemented; Windows does not validate Linux futex behavior, crash recovery, shared-memory lifetime or multi-process races.
- #952: translation certificates remain unimplemented; agree on the checked-effect IR and reject corrupted emitter output in a negative corpus before claiming validation.
- #951: native Go plugin remains unimplemented; `-toolexec` transport alone cannot implement ownership/sendability checks or certify export-data semantics.

Authorship stays with magomedta (#997/#992) and campbellarchiezh-debug (#957/#955/#952/#951). No M1 award is claimed. None of these six has been completed as a compiler feature. The six issue IDs are absent from the checked main MOONSHOT ledger. Published program terms describe a single Wildcard winner per cycle, with $250 M1 and $250 M2; the issue list's per-item prices are not six assigned payments. Maintainer review is needed for any milestone/credit split.

Escape-analysis follow-up: `go test -run 'TestSpawnAllocations|TestPhantom' -gcflags=-m=2` also passed on Go 1.26.9. Raw compiler output in escape-analysis.log identifies escaping function literals for both source dispatch forms, including Go's implicit goroutine wrapper in the named-method case. It is compiler diagnostic evidence for these fixtures only, not a proof about every Hybrid-accepted program.
