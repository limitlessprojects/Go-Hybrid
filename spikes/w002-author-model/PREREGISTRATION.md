# W-002 author follow-up: bounded capability experiment

Base: `284ab73e693cbdf821055d612df1e3f0eeea5924` (2026-11-02).

This record is committed before executing the experiment. It supplements
proposal #279; it does not modify the compiler or claim corpus parity.

## Questions and expected outcomes

1. Moving an owned allocation consumes its source. A second move/read fails.
2. Copying immutable shared data is allowed; writing it fails.
3. A read borrow prevents both owner mutation and transfer until its explicit
   lexical end. A borrowed closure keeps that restriction until it is dropped.
4. Moving captures into a closure consumes them at closure construction, not
   only when spawning. The closure is affine: calling or spawning consumes it.
5. A closure carrying a read borrow may run locally but may not spawn. Moving
   an outer borrow into a closure must not disguise it as an owned capture.
6. Duplicate captures and destination-name collisions reject atomically:
   failed checking must leave the pre-statement context intact.
7. Allocation and scalar literals infer capabilities without annotations.
   Copying an owned name without explicit move/borrow is rejected. Unknown
   effect/callee summaries fail closed. This is a deliberate compatibility
   cost; zero annotations do not mean zero ownership operations.

The test suite will exercise these semantic cases, including controls after
borrow/closure lifetime end. Mutation checks will deliberately disable the
live-borrow transfer guard and duplicate-capture guard; their witness programs
must then be incorrectly accepted. Report actual test counts after running.

## Boundaries and kill criteria

The input is a small Python operation API, not `.hy` text. The context stores
capabilities, allocation identity, and lexical borrow lifetime. That bookkeeping
does not disappear by calling it a type. There is no mutable alias graph, but
identity/lifetime reasoning remains essential. No results may be presented as
48/48 or current-corpus compatibility, actual race freedom, production graph
deletion, diagnostic-code compatibility, or a measured compiler-size reduction.

Unsupported: branches/joins, loops, projections, mutation through exclusive
borrows, recursive/unknown calls, Go escape hatches, and actual closure bodies.
The closure-body checker must eventually verify the same rules in a nested
context; this experiment verifies capture/transfer rules only. Reject the
unqualified 'local types replace all flow reasoning' claim if these excluded
forms require resource identity, joins, or effect summaries. Publish that limit
as a result rather than relaxing a test.
