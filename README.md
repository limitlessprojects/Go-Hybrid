<div align="center">

# **Hybrid**

**Just add a `.hy` file. It's still Go — Hybrid is what Go can safely do.**

*If you know Go, you already know Hybrid: keep your codebase, keep your tooling,
and let the compiler prove your new files race-free — one file at a time.*

[![release](https://img.shields.io/badge/release-v0.1.0-blue)](https://github.com/limitlessprojects/Go-Hybrid/releases)
[![license: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue)](LICENSE)
[![CI](https://github.com/limitlessprojects/Go-Hybrid/actions/workflows/ci.yml/badge.svg)](https://github.com/limitlessprojects/Go-Hybrid/actions/workflows/ci.yml)
[![status: MVP done](https://img.shields.io/badge/MVP-done%20%7C%20G3%20passed-brightgreen)](gates/G3.md)
[![discuss](https://img.shields.io/badge/discuss-feedback%20%26%20Q%26A-8b5cf6)](https://github.com/limitlessprojects/Go-Hybrid/discussions/1)

[Quick start](#quick-start) · [The guarantee](#what-exactly-does-this-compiler-guarantee-about-my-code) · [Design](#how-it-works) · [Evidence](#evidence-measured) · [Roadmap](PLAN.md) · [Feedback thread](https://github.com/limitlessprojects/Go-Hybrid/discussions/1)

</div>

---

## What it is

Hybrid is a **safety-checked superset of Go**. Write `.hy` files beside your
`.go` files wherever you want compiler-enforced ownership and cross-task
sharing rules; a checker (written in Rust) verifies them and transpiles your
`.hy` code to **plain, readable Go**. Your existing Go stays untouched, your
Go callers don't change, and one build command produces one ordinary binary —
no runtime, no VM, no FFI in the output.

```hy
// checkout.hy — a checked file, dropped into an existing Go module
import "rust/regex"

fn validate_email(raw string) (string, error) {
    re := regex.must_compile(`^[\w.+-]+@[\w-]+\.[\w.]+$`)
    if !re.is_match(raw) {
        return "", errors.new("invalid email")
    }
    return raw, nil
}
```

```go
// main.go — your existing Go code, unchanged
email, err := hybrid.ValidateEmail(raw)
```

```sh
hyb build checkout.hy -o myapp    # one command: check → emit Go → go build
```

## Why Hybrid

- **It's still Go.** The output is Go source riding the Go compiler, runtime,
  GC and scheduler. There is no new runtime to operate and no new deployment
  story — the binary is a normal Go binary.
- **Races become compile errors.** In a `.hy` file, sharing mutable state
  across tasks without an ownership transfer, a channel, or the auditable
  `managed` hatch is rejected before `go build` ever runs — with a fix-it in
  the message, not a 3 a.m. page.
- **Incremental.** Adopt file-by-file inside an existing Go module. No
  rewrite, no big-bang migration, no new build culture.
- **One build command.** `hyb build` drives the pinned Go + Rust toolchains
  for you. Consumers of a Hybrid *package* need no Hybrid toolchain at all
  (example 09: a plain `go run` imports a Hybrid-compiled package).
- **Rust crates, Go ergonomics.** A pinned set of Rust crates (serde_json,
  regex, sha3…) is reachable with Go-shaped error handling and
  ownership; the boundary is checked like everything else.

## What *exactly* does this compiler guarantee about my code?

The Tier-0 checker is **fail-closed at critical boundaries** (rule FC, O4.9):

- Every value crossing a `task` spawn or channel send is classified. Values
  the checker **can prove** safe — primitives, channels, immutable, owned
  `move` transfers, fresh literals with sendable parts — pass.
- Values the checker **cannot prove** — a mutable map shared across tasks, a
  borrowed pointer, or **anything whose type it cannot resolve** — are
  rejected (`HYB-E3001`/`E3002`/`E3005`) with diagnostics that name the value
  and teach the fix: ascribe the type, route through a channel, or take the
  hatch knowingly.
- **Nothing silently passes.** The escape hatch — `managed("reason", "category") { ... }` — is
  explicit, linted, greppable, and counted by `--report-managed`; CI can
  forbid it entirely with `--deny-managed`.
- Boundary statements **inside closures** are checked too; Go-API escapes
  (a closure handed to `net/http`) are outside the checked subset and
  [documented as such](SPEC-LANGUAGE.md), not hidden.

The sharp version of the contract: **everything the checker accepts was
proven; everything it can't prove is named.** The line-by-line claims table
— including the primitives you should ask about (`sync.Pool`,
`atomic.Pointer`, `context.Context`, finalizers, `unsafe`, cgo, channel
close) — lives in [GUARANTEES.md](GUARANTEES.md); the full rules with their
pre-registered evidence live in [SPEC-LANGUAGE.md §9](SPEC-LANGUAGE.md); the
honest-claims paragraph (what is *not* guaranteed — e.g. a `managed` region
is still Go's DRF-SC story with `-race` as the backstop) is binding there.

## How it works

`.hy` → **Rust checker** (parse → ownership/sendability/loan checks) → **Go
emission** → `go build`. Rust-crate calls ride a generated bridge: a Rust
`staticlib` linked into the same binary through assembly trampolines that
skip the cgo runtime.

| design question | answer | status |
|---|---|---|
| What can Hybrid emit that Go links? | Go source + `//line` diagnostics; emitted Go assembly for trampolines | ✅ [spike 2: 14/14](spikes/spike2-go-linking/RESULTS.md) |
| What does a Go→Rust call cost? | **2.8 ns** safe stack-switched trampoline vs **46.8 ns** cgo — 16.5× faster, 0 allocs | ✅ [spike 1](spikes/spike1-call-overhead/RESULTS.md) · [G2 bench: fast path 3.37 ns ≤ 5 ns on the live bridge](gates/G2.md) |
| Can consumers skip the toolchain? | Yes — generated Go + prebuilt artifact; a plain `go run` imports a Hybrid package | ✅ [example 09](examples/09-go-imports-hybrid/) |
| Can a scheduler-safe checked language work at all? | Bounded blocking pool, two-tier budgets, race-clean under `-race` | ✅ [spike 4: 100k-cell matrix](spikes/spike4-scheduling/RESULTS.md) |
| Ownership rules without visible lifetimes? | Tier-0 checker: 6 sendable classes, move/freeze, loans, fail-closed boundary | ✅ [checker corpus 127/122](tests/checker-corpus/) + [differential loop 40/40 + 80/80](tools/differential_loop.py) |
| Scheduling with blocking Rust calls | resolved: pool + Go-scheduler fallback, budgets pinned | ✅ spike 4 |

## Quick start

One command, **no Rust toolchain** (the bridge staticlib ships prebuilt):

```bash
curl -fsSL https://raw.githubusercontent.com/limitlessprojects/Go-Hybrid/main/install.sh | sh
```

Prefer building from source? The pins are the contract (Go 1.26.9 / Rust 1.99.0):

```bash
git clone https://github.com/limitlessprojects/Go-Hybrid.git && cd Go-Hybrid
make examples     # all ten target programs, byte-exact vs expected.txt
make test-quick   # workspace tests + checker corpus (109/109) + differential (36/36 + 73/73)
make bench        # G2 call-overhead bench on the LIVE bridge

hyb build examples/01-hello/program.hy -o hello && ./hello
hyb build --package mypkg --out-dir out --binding-prefix example.com/mypkg
```

The ten examples are the acceptance suite: worker pools with backpressure,
JSON transforms through serde_json, net/http servers, an LRU with generics,
graph ownership moves, a Rust-crate digest service, and a Hybrid package
consumed by plain Go — every one parses, checks, emits, builds and runs
byte-exact in CI.

**Windows note:** bridge programs build and run on Windows today. The
Win64 trampolines (`call_win64.s`) landed and are fleet-proven: every
release's Windows leg links the staticlib through mingw cgo and runs the
full runtime suite natively — real crossings through the real trampolines
(PLAN row 17; the fleet crashes that shaped the ABI work — struct-by-value
args and stack alignment — are recorded there). `hyb` accepts both
staticlib flavors (`libhyrt.a` from mingw builds, `hyrt.lib` from msvc
builds). The one requirement: a mingw-w64 gcc for the cgo link.

## Docker (CI)

A pinned-toolchain image (Go 1.26.9 + release `hyb` + the prebuilt runtime —
bridge programs included, no Rust needed at run time):

```bash
docker run --rm -v "$PWD:/work" -w /work   ghcr.io/limitlessprojects/go-hybrid:latest   hyb build ./program.hy -o /work/out && /work/out
```

Also usable as a CI `container:` — `HYB_RUNTIME_DIR` is wired. The image is
smoke-tested (pins + bridge-free + a serde_json bridge crossing) before it
is pushed; see `.github/workflows/docker-publish.yml`.

## Adding Hybrid to an EXISTING `go.mod` project (10 minutes)

No new build system, no separate module culture — `.hy` files live beside
your `.go` files and transpile to checked-in, reviewable, gofmt-clean Go
([discussion #3](https://github.com/limitlessprojects/Go-Hybrid/discussions/3),
design B). A complete working sample lives at
[`examples/gomod-integration/`](examples/gomod-integration/) — this is the
whole recipe:

```bash
# 1. vendor the runtime once (binding packages + prebuilt staticlib)
hyb vendor-runtime .

# 2. add two lines to your go.mod
#    require hybrid.program v0.0.0
#    replace hybrid.program => ./hybridrt

# 3. drop .hy files beside your .go files + one generate line in any Go file:
#    //go:generate hyb generate .

go generate ./...
go build ./... && go test ./...
```

The generated `<name>.hy.go` is plain Go: read it, diff it, review it.
**Delete the `.hy` file and your project is a plain Go module again** — the
pitch is literal. CI staleness gate: `hyb generate --check .` fails when a
`.hy` changed without regenerating, so untranspiled code can never silently
skip the checker.

Directory discovery recurses through real subdirectories, skipping child directory
symlinks (including links back to the project). A directory symlink explicitly
passed as a root and symlinks to individual `.hy` files remain supported.

## When *not* to use Hybrid

- You need **arbitrary existing Go code to run unmodified** inside the checked
  subset — `.hy` checking covers Hybrid files, not `.go` files (that boundary
  is Go's usual story, honestly scoped in the spec).
- You need **every Rust crate** on day one — the curated set grows per
  release.
- You need a **1.0 stability contract** — the language, ABI and CLI will
  still change; pin versions and read the changelog.
- Your hot path can't tolerate Go's GC pauses — put that path in Rust and
  call it from Hybrid; don't put it in Hybrid-shaped Go.

Staying in Go is excellent for most code; a full Rust rewrite is right when
you can afford it; cgo works today but leaves unsafe boundaries hand-audited
and two build systems hand-run. Hybrid's bet is *checked* boundaries and *one*
command.

## Roadmap

**MVP: DONE** (G0 → G1 → G2 → G3 all passed/ratified, 2026-10-13 — gates
record the claim-by-claim re-verification). Post-MVP queue, in order:
runtime sanitizers/fuzzing on the Actions fleet · worker-tier bench ·
per-callee stack budgets · **release engineering (v0.1.0 — this release)** ·
community/feedback · packaging polish. Checkbox-level detail with dates and
exit criteria: [PLAN.md](PLAN.md).

## 🙏 Feedback — and a kind ask

We're running a **public feedback thread** for exactly the questions this
release should answer: [What would make you trust — and adopt — a safety-checked
Go superset?](https://github.com/limitlessprojects/Go-Hybrid/discussions/1)
Short, blunt answers are the most useful kind; good advice gets merged into
the roadmap **with credit**. Q&A, ideas, and show-and-tell are open under
[Discussions](https://github.com/limitlessprojects/Go-Hybrid/discussions).

Before starting, ⭐ star the repository. It helps us grow and shows
you're committed to contributing!

## Contributing

[`good first issue`](https://github.com/limitlessprojects/Go-Hybrid/labels/good%20first%20issue)
issues are tagged. Docs, adversarial test corpora, diagnostic wording, and
example programs are all high-leverage right now. There is a **$500 bounty
pool** over the top 10 contributions each cycle: reproduced soundness bugs
and useful improvements alike, with severity tiers and a Hall of Fame —
the full rules are in [BOUNTY.md](BOUNTY.md). **The Wildcard Track adds
another $500 per cycle** for revolutionary, contrarian, high-risk ideas a
normal feature bounty would reject — no prototype required, breaking
changes and rewrites explicitly allowed, failures published as learnings
(and paid): ledger and solicitation calendar in [MOONSHOT.md](MOONSHOT.md).
The project follows an
evidence-first protocol: hypotheses are pre-registered before measurement,
failures are reported plainly, and tests are never weakened to pass — see
[AGENTS.md](AGENTS.md) for how the repo works. Security reports start at
[SECURITY.md](SECURITY.md); roles and credit live in
[CONTRIBUTOR_RECOGNITION.md](CONTRIBUTOR_RECOGNITION.md).

## Contributors

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<!-- ALL-CONTRIBUTORS-LIST:END -->

The All-Contributors bot keeps this table current. Ask it for credit with
`@all-contributors please add @yourname for code` (or `bug`, `doc`,
`security`, `test`) on your PR or issue — how each role is earned is in
[CONTRIBUTOR_RECOGNITION.md](CONTRIBUTOR_RECOGNITION.md).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option — the same formula as the Rust ecosystem. Unless you explicitly
state otherwise, any contribution intentionally submitted for inclusion in
this project by you shall be dual-licensed as above, without any additional
terms or conditions.
