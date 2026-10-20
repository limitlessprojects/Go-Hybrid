# Preregistered review of listed Wildcard proposals

Source proposals: #997 and #992 by magomedta; #957, #955, #952 and #951 by campbellarchiezh-debug. This independent review does not claim their M1 awards or that any proposal is implemented.

Before measurements, hypotheses:
- H997: changing an otherwise equivalent spawned Go closure into a named value-receiver task does not establish zero total allocations per task spawn. A reused worker/channel is a distinct workload and must be labelled separately.
- H992: a zero-sized generic phantom capability in ordinary emitted Go does not enforce affine use; copying and reusing a value remains legal unless the Hybrid checker or another validator enforces consumption.

Protocol: Go 1.26.9, Windows/amd64, GOMAXPROCS=2. Compare synchronous channel-joined closure and value-receiver spawns, plus a persistent-worker control. Measure AllocsPerRun and benchmem (three runs). Test a negative capability example that copies/reuses its value. These are Go runtime/design probes, not measurements of Hybrid-emitted code, Linux futexes, compiler certificates or p99 latency. No safety guarantee changes or compiler source edits.

#957 needs an agreed effect language and foreign-call boundary; #955 needs a Linux futex/interprocess validation target; #952 needs an agreed checked-effect IR and adversarial translation corpus; #951 needs a checker integration and Go export-data contract. None is a fixed $200/$250 implementation award. BOUNTY.md and MOONSHOT.md require milestone review and preserve the original proposer's credit. Compiler implementation of these four is not completed by this probe.

Protocol amendment before execution: the pinned Go 1.26.9 toolchain download did not finish within the local setup window. Use the already installed Go 1.27.0 windows/amd64 for an exploratory probe only. These results cannot satisfy the project's pinned 1.26.9 acceptance gate; that rerun remains required.

The pinned download subsequently completed before any measurements. The original Go 1.26.9 protocol is used after all; no fallback measurements were taken.

## Follow-up protocol (before new measurements)

- #952: generate 50 type-correct emission mutations (five families, ten constants each: dropped store-back, pointer replacing copy, shared mutable task capture, reordered channel sends, move source reused). Keep the proposed zero-sized witness declarations present in every file. Ask pinned Go 1.26.9 to build every mutant. The proposal's stated acceptance condition is rejection of all 50; any compiled unsafe/incorrect mutant falsifies the sufficiency of these detached witnesses. Never run race-inducing mutants.
- #992: retain the repeat-consumption negative control; extend compile controls before discussing deletion of the current checker. No runtime claim will substitute for compile-time rejection.
- #957: a deterministic schedule experiment must separate an explicit, closed effect state machine from arbitrary Go goroutines. Verify one million scheduled events and 10,000 replay runs. Reject non-admitted effects and corrupted replay certificates. This would be a bounded runtime model, not new Hybrid syntax or a compiler guarantee.
- #955: Windows can cross-compile a pure-Go Linux test binary, which can execute in the available Ubuntu-24.04 WSL environment. A real two-process mmap/futex test is necessary; shared-process tests or successful cross-compilation do not establish IPC correctness. Fixed 64-byte, pointer-free messages only; no general Tier-0 proof.
- #951: an actual toolexec hook must preserve compiler flags, version probes, exit status and assembler/linker invocation. A transport hook does not replace the ownership engine; unsupported ownership proof must be rejected rather than certified. The full Kubernetes/etcd and 12,000-line checker-port claim remains a separate, unfulfilled acceptance gate.
