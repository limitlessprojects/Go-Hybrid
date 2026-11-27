# Scope and experimental history

The initial errgroup sketch was frozen in commit 885008a9 before its
acceptance edits. Its first smoke check had HYB-E0002: the retained Go
`case` arm and same-line semicolon return were not Hybrid grammar. Removing
`case` and splitting the statements exposed the actual Tier-0 diagnostics:
HYB-E1001 (context outside M5), HYB-E3104 (returning a loan), HYB-E3001
(spawned closure), HYB-E3013 (foreign callback writing captured state), and
HYB-E4001 (nil assignment to a Hybrid channel field).

The accepted errgroup port moves WithContext to the Go adapter, uses a
zero channel variable for the unlimited case, and takes two explicit
managed interop regions. It does not remove cancellation or the limit
methods. All original upstream tests are retained byte for byte.

Singleflight was developed exploratorily after this initial registration;
there is **no pre-edit frozen naive singleflight file**. We therefore do
not claim that its total diff measures only checker-forced concessions,
or that this is a fully compliant preregistered two-port M1. An independent
porter must repeat it from the pinned production source to supply that
missing evidence. The accepted port and tests are useful executable
artifacts in the meantime; retrospective registration cannot fix this.

Observed singleflight barriers and the disclosed concessions:

| Barrier | Observation | Accepted port |
| --- | --- | --- |
| Multi-result function types | HYB-E0002 on `fn() (any, error)` | Hybrid spelling `fn() any error` |
| Map type operand | HYB-E0002 on `make(map[string]*call)` | An empty map literal reached the ownership check |
| Pointer-valued field | HYB-E3103 on `map[string]*call` and stores | Mutex retained, storage moved to foreign sync.Map through Go lookup/store/delete callbacks |
| Go name `Map` | Bare `sync.map` lexes as reserved `map` | `sync.map_` normalizes to Go `sync.Map`; this is a surface spelling, not an ownership proof |
| Borrowed interface result | HYB-E3104 returning `c.val` even via an extraction callback | A Go callback extracts the result; this path is explicitly outside Tier-0 |
| Fatal panic task | HYB-E3001 / HYB-E3005 | A dedicated managed spawn preserves fatal DoChan panic semantics |
| Go type assertions, stack and Goexit behavior | Outside the scoped checked surface | Original panicError/newPanicError helpers and runtime.Goexit remain in the Go adapter |
| Exported Go Result fields | Public API adapter needed | A callback sends original Result values into the original buffered channel; no adapter goroutine is added |

The shared map still has the original outer mutex, now around sync.Map.
This deliberately preserves the lifecycle instead of claiming a lock-free
or faster implementation. Extra callbacks and sync.Map operations are
costs, not optimizations demonstrated by this study. Both outputs are
ESCAPED. The four managed regions in singleflight include the nested fatal
panic spawn; the checker reports these counts, not this document.

The sweep manifest was fixed before its run: the 13 Go files in x/sync
v0.18.0, all 26 top-level sync Go files from Go 1.26.9, and all eight
sync/atomic Go files, 47 files total. These are **files, not 47 independent
programs**. They are fetched from pinned toolchain/module sources and
hash-checked; raw Go lexical rejection must never be called a measured
sendability/ownership failure. No source is replaced based on its result.

Production source sizes are 151 and 214 physical lines including comments
and blanks. This differs from the issue's proposed <150-line programs and
is reported rather than silently deleting behavior to fit the limit.
The nine-program decision thresholds are not applied to this exploratory
and independently unreviewed pair. Final census verdict: INCONCLUSIVE.
