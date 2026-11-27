# #4739: executable production ports with explicit escapes

Companion implementation of chyalls's expressiveness-census proposal,
not a new wildcard proposal or a self-awarded milestone. No compiler or
language rules changed. Both ports pass the **unchanged upstream tests**,
including cancellation, limits, duplicate suppression, Forget, panic and
Goexit. Both require escapes; neither is a pure Tier-0 port.

## Reproduce

With Go 1.26.9, Rust 1.99.0, Python 3.9+ and Git, from the repository root:

```sh
cargo build --locked --manifest-path compiler/Cargo.toml -p hyb -j2
python3 -B -m unittest discover -s spikes/w4739-expressiveness-census -p test_census.py -v
HYB_RUNTIME_DIR="$PWD/runtime" python3 -B spikes/w4739-expressiveness-census/census.py \
  --hyb compiler/target/debug/hyb --out /tmp/hybrid-census --count 3
```

Local evidence: Windows/amd64, checker source
82037c3da4f3b888eef2e21ee1cb5e347e74a0c4, no toolchain-drift override.
On Windows, use hyb.exe, set HYB_RUNTIME_DIR to the checkout's runtime,
and put Rust 1.99.0 and MinGW gcc on PATH for the race detector.
A focused workflow repeats the study on Linux.

The runner hashes its inputs, emits both packages, adds the inventoried Go
adapters, runs original and ported tests, and compares the exact
package/test-name/repetition inventory. Missing inputs, checker crashes,
invalid JSON, absent managed reports and failed behavior cannot become PASS.

## Observed results — 2026-11-28, Europe/Istanbul

| Port | Original physical lines | Hybrid lines | Go helper lines | Managed regions | Upstream tests + race |
| --- | ---: | ---: | ---: | ---: | --- |
| errgroup | 151 | 76 | 19 | 2 | PASS, count=3 |
| singleflight | 214 | 112 | 90 | 4 | PASS, count=3 |

Both baseline and candidate produced **53 passing events, 19 unique
test/subtest/example names**, with identical inventories. Go examples run
once even with count=3; other listed cases repeat three times. The 11
runner regressions pass. See [machine results](evidence/results.json),
[baseline events](evidence/baseline-tests.json) and
[candidate events](evidence/candidate-tests.json).

Removing managed headers restores E3001/E3013 for errgroup and E3001/E3005
for singleflight. Counts come from the checker. These regions preserve
Go's DRF-SC discipline; they are unchecked escapes, not ownership proofs.
Both classifications are **ESCAPED**.

### Actual edit accounting

| Port | Hybrid additions | Original deletions | Added Go helper lines | Total descriptive tax |
| --- | ---: | ---: | ---: | ---: |
| errgroup | 65 | 140 | 19 | 148.344% |
| singleflight | 101 | 203 | 90 | 184.112% |

Actual git diff counts additions/deletions. A replacement is counted once
as a deletion plus an addition, not a third modified line. Every Go helper
line is included in total tax. These cross-language diffs include syntax,
comments and API adaptations: they are **not causal checker-only costs**.
The unchanged-file regression has zero tax. Errgroup's frozen initial
sketch to accepted port is +15/-12; singleflight is exploratory and has no
pre-edit frozen naive file.

### The sweep measures lexical compatibility

The fixed [manifest](sweep-origins.json) contains 13 x/sync files, 26 sync
files and eight sync/atomic files from pinned upstream sources. All 47
were rejected with HYB-E0001 (5,011 diagnostics). Raw Go spelling fails
the lexical contract before ownership checking; this is **47 source files,
not 47 independent programs or ownership failures**. See
[sweep evidence](evidence/sweep.json). No file was replaced after its result.

## Preserved behavior and disclosed concessions

Errgroup's WithContext remains in Go because context is outside M5 and
its borrowed constructor cannot be returned as a Hybrid loan. Public API
adapters forward to emitted Hybrid methods; two managed regions cover
spawned closures and captured sync.Once writes. Cancellation and all
Go/TryGo/SetLimit/Wait behavior remain tested.

Singleflight retains the original outer mutex but replaces a pointer-valued
map field, forbidden by E3103, with foreign sync.Map callbacks. Result
extraction avoids returning a loan (E3104). Original panicError/stack/Goexit
helpers remain in Go. The DoChan callback sends original Result values
without an extra adapter goroutine. Four managed regions include its fatal
panic task. Additional callbacks/map operations are costs: **no speedup is
claimed**. The diagnostic history is in [EXPLORATION.md](EXPLORATION.md).

All upstream sources/tests retain original bytes and copyright notices;
upstream/LICENSE holds the BSD license. inputs.json records the module
version, commit, checksum, helper inventory and delivered source hashes.

## Verdict and review request

**INCONCLUSIVE for the nine-program claim.** Physical source sizes exceed
the proposed <150-line target; singleflight has no pre-edit naive freeze;
the independent reviewer/second porter and seven other programs remain.
The descriptive medians do not establish that Tier-0 is justified or
unjustifiable. We do not self-label UNPORTABLE or close #4739.

Ready for review: two executable, behavior-tested escaped ports, real edit
accounting, lexical sweep, managed reversal controls and a fail-closed
runner. Please review this as a companion implementation, identify the
subset to replicate independently, and clarify implementation/verification
credit and bounty eligibility. The proposal remains attributed to chyalls;
M1/M2 acceptance and payment are maintainer decisions.
