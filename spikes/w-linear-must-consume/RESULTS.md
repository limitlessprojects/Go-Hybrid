# Wildcard Spike Results — Linear Must-Consume Obligations & Escape-Certified Stack Regions

**Author:** `@cyzcz` | **Verifiers:** `python3 spikes/w-linear-must-consume/linear_obligation_verifier.py --check --live-go` & `python3 -m unittest discover -s spikes/w-linear-must-consume/tests`

## 1. Pre-Registered Gates & Falsifiers (`H1`–`H3`, `F1`–`F2`)

| ID | Gate | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **H1** | Static `@[linear_must_consume]` obligation checker (`HYB-E3701`) admits branch-complete consumption (`01`) and rejects early `?` try-return leaks and partial-branch drops (`02`) | `01` accepted (`0` diags); `02` rejected with `2x HYB-E3701` (line 6 early `?` exit + line 5 unconsumed exit) | **PASS** |
| **H2** | Go `cmd/compile` escape-analysis verification (`go build -gcflags="-m=2"`) on `@[no_escape_loop]` (`RunLinearStackLoop`) vs `sync.Pool` (`RunSyncPoolLoop`) | `RunLinearStackLoop` inlines (`cost 79 <= 80`) with `0` heap escapes; `RunSyncPoolLoop` exceeds inline budget (`cost 201 > 80`) and `new(Packet) escapes to heap` at `linear_region.go:63:13` | **PASS** |
| **H3** | Throughput & allocation benchmark (`256` iterations/run, Xeon Platinum 8457C, `go test -bench=. -benchmem -race`) | `BenchmarkLinearStackLoop`: **`2,885 ns/op`** (`0 B/op`, `0 allocs/op`) vs `BenchmarkSyncPoolLoop`: **`6,127 ns/op`** (**`2.12x` speedup**, zero atomic/interface overhead) | **PASS** |
| **F1** | Double-consume negative control (`TestLinearHandleExactlyOnce`) | Second `ConsumeClose()` deterministically rejected with `HYB-E3003` | **FIRED** |
| **F2** | Dynamic interface-sink escape falsifier (`RunFalsifierF2InterfaceEscape` + `HYB-E3702`) | Passing `&pkt` to `FormatterSink.Observe(any)` forces `cmd/compile` `parameter v leaks to {heap}`; caught statically by `HYB-E3702` | **FIRED** |
