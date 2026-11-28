# Recognizer revision probe packs (#4807)

## Hypothesis

A compiler-side pass can take several checked finite byte recognizers, prove
which revisions are language-equivalent, and emit the smallest probe pack that
separates every distinguishable pair. The result is only called minimal after
both the reachable product automaton and the exact pairwise cover are exhausted.

This implementation closes the falsification milestone left by the standalone
Python model: extraction starts at real `hy-syntax` AST nodes, the analysis runs
inside `hyb`, and the emitted witnesses are checked against separately generated
and compiled Go binaries.

## Surface

The first accepted subset is intentionally narrow:

```text
hyb distinguish [options] <name>=<revision.hy> ...
```

Each revision exports one checked function with this shape:

```hybrid
pub fn recognize(input []byte) bool {
    var state int = 0
    for b in input {
        if b == 65 { state = 1 }
    }
    return state == 1
}
```

Supported control is scalar `var`/`:=` state initialized before one
`for ... in input` loop, integer assignments/arithmetic, nested `if`/`else`,
and a Boolean return evaluated at EOF. The byte is available as `b`. States are
the reachable tuples of scalar integer values. Unsupported AST nodes, failed
checker verdicts, non-integer state, and state growth past the configured cap
fail closed with an error.

This is a bounded extractor, not a claim that arbitrary Hybrid control flow is
finite or analyzable. Counters that keep growing hit `--max-states-per-recognizer`
and are rejected rather than approximated.

## Exactness contract

For every candidate, extraction enumerates all 256 outgoing bytes from every
reachable state until the finite state graph closes or the cap fires. The joint
BFS then visits reachable candidate-state tuples and records one shortest,
lexicographically smallest input for each distinct answer vector.

Every inequivalent pair creates one set-cover bit. Each probe covers every pair
whose answer differs on that probe. The exact branch-and-bound search branches
on an uncovered pair with the fewest covering probes and compares solutions by:

1. number of probes;
2. total input bytes;
3. maximum probe length;
4. lexicographic probe sequence.

Certification is honest:

| Result | Meaning |
|---|---|
| `minimal` | Product graph exhausted and exact cover search exhausted |
| `upper_bound` | Product graph exhausted; cover search hit its node cap |
| `inconclusive` | Product graph hit its state cap; equivalence is unproven |

An `upper_bound` pack is still a verified cover. An `inconclusive` pack is
diagnostic evidence only and must not be used to assert equivalence.

## Fixtures and measured results

### Eight encoded grammars

`hy/grammar-{000..111}.hy` are eight real `.hy` revisions. Their shared finite
state records the independent `+`, `0`, and `_` flags; each revision accepts a
different three-bit answer code.

Observed:

```text
status:                 minimal
product states:         8
distinct answer vectors: 4
distinguishable pairs:  28
probes:                 3
total probe bytes:      4
probe inputs:           +0, 0, _
```

Three requests supply eight three-bit signatures, matching the information
lower bound.

### Four tags plus equivalent baseline

`tag-a.hy` through `tag-d.hy` accept exactly one distinct one-byte language.
`tag-base.hy` accepts nothing. `tag-base-extra-state.hy` toggles between two
states but is language-equivalent to the baseline.

Observed:

```text
status:                 minimal
distinct answer vectors: 5
distinguishable pairs:  14
probes:                 4
equivalence class:      base, base-extra-state
probe inputs:           A, B, C, D
```

This remains the useful counterexample to optimistic covers: every tag probe is
required to separate its one-byte revision from the baseline.

### Separate Go binaries

`compiler/crates/hyb/tests/distinguish_go.rs` takes the pack produced from the
eight `.hy` revisions, runs each revision through Hybrid's ordinary Go emitter,
builds eight independent Go executables, invokes all 24 candidate/probe pairs,
and compares every accept/reject answer with the emitted key.

Observed locally with Rust 1.99.0 and Go 1.27.4:

```text
8 separately built Go recognizers agreed with all emitted probes in 3.513s
```

Additional local gates:

```text
cargo +1.99.0 test --workspace --quiet
  PASS with HYB_ALLOW_TOOLCHAIN_DRIFT=1 (host Go 1.27.4; no pinned 1.26.9 installed)
cargo +1.99.0 clippy -p hy-distinguish --all-targets -- -D warnings
  PASS
cargo +1.99.0 clippy -p hyb --tests --no-deps -- -D warnings
  PASS
make check (RUSTUP_TOOLCHAIN=1.99.0, documented Go drift override)
  PASS: Python/tooling suites, corpus census, guarantees, registration,
  version pins, and workspace cargo check
```

Hosted CI remains the pinned Go 1.26.9 gate.

## Reproduce

From the repository root:

```bash
cd compiler
cargo test -p hy-distinguish
cargo test -p hyb --test distinguish
cargo test -p hyb --test distinguish_go -- --nocapture
```

Generate a pack manually:

```bash
target/debug/hyb distinguish \
  --out /tmp/grammar-pack.json \
  grammar-000=../spikes/w4807-recognizer-probe-pack/hy/grammar-000.hy \
  grammar-001=../spikes/w4807-recognizer-probe-pack/hy/grammar-001.hy \
  grammar-010=../spikes/w4807-recognizer-probe-pack/hy/grammar-010.hy \
  grammar-011=../spikes/w4807-recognizer-probe-pack/hy/grammar-011.hy \
  grammar-100=../spikes/w4807-recognizer-probe-pack/hy/grammar-100.hy \
  grammar-101=../spikes/w4807-recognizer-probe-pack/hy/grammar-101.hy \
  grammar-110=../spikes/w4807-recognizer-probe-pack/hy/grammar-110.hy \
  grammar-111=../spikes/w4807-recognizer-probe-pack/hy/grammar-111.hy
```

## Known boundaries

- Extraction covers the documented scalar finite-state subset only.
- Product-state exploration is exponential in candidate count; exact cover is
  NP-hard. Budgets are mandatory and their exhaustion never overstates proof.
- The pack identifies behavior only among the supplied revisions. A deployed
  implementation that matches every signature is identified within that finite
  candidate set, not as arbitrary code.
- A service that changes revision between probes violates the model.
- Alias-heavy state, helper calls, early returns, nested loops, channels,
  closures, and foreign calls are rejected by extraction.
- This is local evidence. Hosted CI, maintainer review, acceptance, and any
  bounty payment remain pending.

## AI-assistance disclosure

The implementation and fixtures were produced with AI assistance. The code is
MIT/Apache-2.0 under the repository's existing dual license. No third-party
implementation was copied.
