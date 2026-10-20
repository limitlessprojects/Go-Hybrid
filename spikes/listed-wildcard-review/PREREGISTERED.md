# Preregistered review of listed Wildcard proposals

Source proposals: #997 and #992 by magomedta; #957, #955, #952 and #951 by campbellarchiezh-debug. This independent review does not claim their M1 awards or that any proposal is implemented.

Before measurements, hypotheses:
- H997: changing an otherwise equivalent spawned Go closure into a named value-receiver task does not establish zero total allocations per task spawn. A reused worker/channel is a distinct workload and must be labelled separately.
- H992: a zero-sized generic phantom capability in ordinary emitted Go does not enforce affine use; copying and reusing a value remains legal unless the Hybrid checker or another validator enforces consumption.

Protocol: Go 1.26.9, Windows/amd64, GOMAXPROCS=2. Compare synchronous channel-joined closure and value-receiver spawns, plus a persistent-worker control. Measure AllocsPerRun and benchmem (three runs). Test a negative capability example that copies/reuses its value. These are Go runtime/design probes, not measurements of Hybrid-emitted code, Linux futexes, compiler certificates or p99 latency. No safety guarantee changes or compiler source edits.

#957 needs an agreed effect language and foreign-call boundary; #955 needs a Linux futex/interprocess validation target; #952 needs an agreed checked-effect IR and adversarial translation corpus; #951 needs a checker integration and Go export-data contract. None is a fixed $200/$250 implementation award. BOUNTY.md and MOONSHOT.md require milestone review and preserve the original proposer's credit. Compiler implementation of these four is not completed by this probe.
