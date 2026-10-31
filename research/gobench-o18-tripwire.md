# O18 on real Go: 94.5% of GoBench spawn sites are unprovable under the strict A1 ruling

**Issue:** [#1890](https://github.com/limitlessprojects/Go-Hybrid/issues/1890) · **Status:** measurement + protocol; no compiler changes.

## 0. Headline

I ran the O18 tripwire shape census against the real-world ground truth the prompt names — GoBench's GoKer non-blocking suite (Yuan et al., CGO 2021) — over all **35 kernels** (etcd, Kubernetes, gRPC, CockroachDB, Moby, Istio, Knative serving).

| Metric | Value |
|---|---|
| kernels with at least one `go` statement | 34 / 35 |
| total `go` spawn sites | **73** |
| plain function spawns (`go f(...)`) — provable by O4.0 | 4 (**5.5%**) |
| method spawns (`go x.m(...)`) | 17 (**23.3%**) |
| closure spawns (`go func(){...}()`) | 52 (**71.2%**) |
| **spawn sites the checker cannot prove (method + closure)** | **69 / 73 = 94.5%** |
| kernels where every spawn site is unprovable | **30 / 34** |
| channel-send lines | 56 |

The O18 reversal trigger is "more than ~25% of spawn/send sites need `managed`". On real Go the answer is **94.5%**, and it is not close. Under the current fail-closed rule (O4.9), the strict (c)+(b) ruling does not merely cost a quarter of real programs — it forces essentially every real concurrency kernel through the `managed` hatch or a source rewrite.

## 1. Why this is measurement and not a hunch

`hyb` demonstrates the mechanism directly. Transliterating `etcd/4876` (61 lines) into `.hy` and running the locally built checker:

```
error[HYB-E3005]: `sws.send_loop` cannot be proven safe across tasks — its type did not resolve
  = why: the checker can only prove spawns of declared top-level `fn`s — `sws.send_loop`
    is a method value or a computed callee, which the spawn-callee proof (O4.0) cannot verify
  = fix: ... wrap the site in `managed("why this bypass is safe", "category") { ... }`
```

Both `go` statements in that kernel are method spawns; both are rejected before any sendability reasoning begins. That is the same fail-closed path that produces the 94.5%.

## 2. Protocol (committed before the count)

1. Dataset: `goodmorning-coder/gobench`, `gobench/goker/nonblocking/<project>/<issue>/*_test.go`, 35 kernels, fetched at master.
2. A **spawn site** is a line matching `^\s*go\s+` outside comments. Classification of the callee:
   - `closure` — `go func(...){...}(...)`;
   - `method` — `go receiver.name(...)` or `go pkg.Type.name(...)`;
   - `fn` — `go name(...)` with a bare identifier;
   - `other` — anything else (zero occurrences).
3. A site is **provable** iff the callee is a top-level `fn` under O4.0. Everything else is unprovable and requires `managed` or restructuring.
4. Channel sends counted as non-comment lines containing a send arrow. This number is reported but not used for the headline because sendability of the element type needs the transliteration, not the shape.
5. Reproducible script: `research/gobench-spawn-census.mjs`; raw per-kernel data: `research/gobench-spawn-census.json`.

## 3. Threats to validity

- **Shape is not semantics.** A closure can sometimes be lifted to a top-level `fn`, converting an unprovable site into a provable one. That rewrite is exactly the "restructure" cost the tripwire is meant to price, but the census does not model how much lifting is feasible; the true managed-fraction is therefore an upper bound, not an equality.
- **Kernels are minimized.** GoBench kernels are 1 KB extracts, not full programs; they preserve the race, not the surrounding idiom (interfaces, method values, callbacks). Real production Go likely skews *further* toward method and closure spawns, i.e. the 94.5% is conservative.
- **One kernel has no spawn** and is excluded from the per-kernel ratio but included in the kernel count.
- The census counts syntactic sites, not dynamic executions; a site inside a loop counts once.

## 4. What this implies for the A1 ruling

The prompt's question — "does the corpus overstate how cheap the strict ruling is?" — has a clear answer: **yes**. `tests/checker-corpus/` reports 10.0% / 15.0% managed on 20 programs written by the same team to pin the rule. On code nobody here wrote, the unprovable fraction is 94.5% of spawn sites, and 30 of 34 kernels have no provable spawn at all.

Two readings, both testable:

1. **The rule is right and Go is the problem** — then the `managed` hatch is not an escape hatch, it is the normal path, and the ledger should say so; the lint becomes the product.
2. **The rule is too strict at the spawn-callee boundary** — then the fix is in O4.0 (proof of method values and immediately-invoked closures), not in A1. That is a smaller, cheaper change than loosening sendability, and it is the one this data points at.

Either way the O18 trigger has fired on evidence. I would not spend the next cycle defending the corpus baseline without publishing this number next to it.

## 5. Next step (pre-registered)

Transliterate the 7 kernels with a *single* spawn site first — they isolate one decision each and are the cheapest way to separate the two readings above. For each: does the site survive as a provable spawn after the minimal lift, or does it need `managed`? Publish the per-kernel verdicts as they land; do not aggregate until all seven are done.
