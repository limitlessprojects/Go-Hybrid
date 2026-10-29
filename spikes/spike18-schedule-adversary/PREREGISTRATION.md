# W-018: independent schedule adversary

Registered before running this experiment, against upstream main
`1f0ff978f4deff78c77bca96db0e3e46f32de71b`.

This is an independently written follow-up to
[proposal #378](https://github.com/limitlessprojects/Go-Hybrid/issues/378),
specifically milestone 2 (mutated witnesses, exhaustive small-state checks,
and runtime FIFO instrumentation). It does not implement `.hy` lowering,
replace the original proposal/prototype, or claim their authorship.

Hypotheses and falsifiers:

1. A replay checker accepts exactly those submitted witnesses that fire every
   actor, respect all input/output rates and capacities, return to the initial
   marking, and truthfully report repetitions, high-water marks and FIFO payload
   bytes. Any disagreement with an independent prefix-count oracle on the
   enumerated small graphs falsifies this claim.
2. Checked schedules can emit sequential, fixed-array Go kernels from a closed
   uint64 source/scale/sum/sink registry. Four hand-scheduled fixtures (pipeline,
   multirate, join, initialized feedback) must match hand-derived output
   formulas and an independently implemented FIFO interpreter for 100 periods.
   Any mismatch or runtime queue-guard failure falsifies this claim.
3. Successful generated `Step` calls allocate zero times in `AllocsPerRun`
   (10,000 measured runs per fixture). This is an empirical statement about
   these fixtures, not a proof about arbitrary actors or Hybrid programs.
4. Runtime guards are tested with negative controls: corrupting an emitted
   queue's token count must panic for underflow/overflow. A guard that never
   fires on its negative control is not supporting evidence.

Scope: finite manually supplied schedules, positive integer rates, uint64
tokens with modulo-2^64 arithmetic, explicit initial values, and closed actor
semantics. The checker must reject unknown operations and malformed witnesses.
Self-loops consume before producing. Payload budgets exclude indices, actor
state, struct padding, stacks, code and the Go runtime. A witness demonstrates
one feasible periodic schedule, not minimal buffers, complete scheduling,
general deadlock freedom, or parallel throughput.

No speedup target or comparison with other submissions is preregistered. The
small-graph enumeration is bounded evidence; the independent oracle must not
call the replay checker's transition implementation.

AI assistance: OpenAI Codex; no independent human review is claimed.
