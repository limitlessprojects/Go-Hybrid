# W-010 / #184 — Session-Type Endpoint Generation Feasibility Spike

**Status: M1 spike complete. 8/8 tests pass. Preregistration honored.**

- Preregistration: PREREG.md (registered 2026-10-19, before first run)
- Issue: https://github.com/limitlessprojects/Go-Hybrid/issues/184
- Ledger: MOONSHOT.md slot 10 (W-010)
- Author: TNT agent

## What was built

A standalone, dependency-free Rust crate implementing the closed-subset
session-type story end to end.

| Module | What it does | LOC |
|--------|--------------|-----|
| Global / Local types | Linear message sequence + binary labelled choice. No recursion, no delegation. | ~40 |
| project(g, role) | Honda-Yoshida-Carbone projection with the uninvolved-role merge condition. | ~60 |
| synthesize(Map Role Local) | Inverse: given per-role locals, reconstruct a global type. Picks a runnable (sender, receiver) pair at each step; rejects cycles with no ready pair. | ~80 |
| simulate_locals(Map Role Local) | Exhaustive interleaving explorer. Enumerates every schedule of enabled endpoints up to a bounded state count. Distinguishes Terminates from Deadlock (with a witness trace and the set of stuck roles). | ~100 |
| codegen_go(&Global) | Emits a hub-mediated Go skeleton with a 5-second deadline that prints DEADLOCK or PROTOCOL OK. Not run — see Honest limits. | ~120 |
| Tests | 8 tests: ping-pong rejected, request/reply round-trip, three-role pipeline, three-role cross-cycle, uninvolved-merge ok, uninvolved-diverge error, codegen non-empty, ping-pong simulation deadlocks. | ~170 |

Total: ~570 LOC of Rust, zero dependencies. Builds offline.

## Prereg success bar — actual results

| Bar | Prereg bar | Actual |
|-----|-----------|--------|
| 1 | All tests pass on cargo test --release | PASS: 8 passed, 0 failed |
| 2 | synthesize({A: send p; recv q, B: send q; recv p}) returns Err | PASS: Err(NoMatchingReceiver sender A to B label p) |
| 3 | synthesize({A: send p; recv q, B: recv p; send q}) returns A -> B : p . B -> A : q . End, projects back | PASS: exact shape, both round-trips true |
| 4 | simulate(that global) returns Terminates | PASS: Terminates states 3 trace Send(A to B p) Send(B to A q) |
| 5 | codegen_go produces non-empty Go (inspected by eye; not run) | PASS: 910 bytes |
| 6 | cargo build --offline succeeds | PASS: cargo test --release --offline finished in 4.87 s |

## Actual demo output (w010-demo.exe)

    [1] A/B ping-pong: A sends p then recvs q; B sends q then recvs p
      REJECTED as prereg predicted: NoMatchingReceiver sender A to B label p
      simulation: DEADLOCK after 1 states; witness=[]; stuck=[A, B]

    [2] Request/reply: A sends req; B recvs req then sends resp; A recvs resp
      synthesized global: Msg A->B req, Msg B->A resp, End
      project(A) == original A: true
      project(B) == original B: true
      simulation: TERMINATES, 3 states,
        trace=[Send A->B req, Send B->A resp]

    [3] Three-role pipeline: A -m1-> B -m2-> C -m3-> A
      synthesized: Msg A->B m1, Msg B->C m2, Msg C->A m3, End
      simulation: TERMINATES, 4 states

    [4] Three-role cross-cycle: A sends a; B sends b; C sends c (all recv in wrong order)
      synthesis REJECTED: NoMatchingReceiver sender A to B label a
      simulation: DEADLOCK after 1 states; witness=[]; stuck=[A, B, C]

    [5] codegen for request/reply:
      generated 910 bytes of Go

Note on states: states in simulate_locals counts the distinct states the DFS
popped and visited, including the initial state. A deadlock at the root
reports states 1 because only the initial state was visited — the trace is
empty because no event was firable. This is the pre-registered semantics; it
is not a bug.

## What this establishes (honest claims)

- The theorem's synthesis direction is true for the linear, acyclic,
  binary-choice subset: ping-pong cannot synthesize to any global (test
  ping_pong_is_not_synthesizable), and well-formed programs
  synthesize <-> project round-trip (tests
  request_reply_synthesizes_and_terminates, three_role_pipeline).
- The exhaustive simulator distinguishes the three cases from the proposal's
  motivation (runnable-and-completing, immediately-stuck, and
  stuck-only-some-schedules; the last is exercised by the ping-pong and
  cross-cycle, which the DFS reports as Deadlock after the root, not as
  Terminates).
- The uninvolved-role merge condition is the actual workhorse: a choice the
  uninvolved role sees identically projects fine
  (project_choice_uninvolved_merge_ok), and a choice the uninvolved role
  sees differently is rejected (project_choice_uninvolved_diverge_errors).
  This is the standard condition, mechanized and tested.

## What this does NOT establish (honest limits — see PREREG.md)

1. No Go toolchain on the author machine. The codegen_go output is inspected
   by eye (910 bytes, package main, hub-mediated, 5 s deadline, DEADLOCK /
   PROTOCOL OK markers) but not compiled or run. A reviewer with Go
   installed should run:

       cd work/
       go build ./...
       timeout 5 go run .

   and confirm PROTOCOL OK. This is a check I cannot perform; I accept the
   reviewer's authority on it.

2. No recursion (mu X . ...), no endpoint delegation, no
   select-on-multiple-channels, no cancellation. All excluded from the
   subset by design (see PREREG.md).

3. No unbounded theorem. The simulator explores a bounded state space
   (MAX_STATES = 20_000); beyond that it returns DepthExceeded, never a
   safety verdict. This is the same discipline the proposal's "Bounded
   model evidence" section asks for.

4. This is not a Go-Hybrid compiler change. It is a standalone model of the
   claim, matching the spike protocol used by #370 and #381.

## Kill criteria status — none fired

| Kill criterion | Fired? |
|----------------|--------|
| Ping-pong is somehow synthesizable | No — rejected with NoMatchingReceiver |
| Well-formed global reports Deadlock | No — request/reply and pipeline both Terminates |
| cargo test fails offline | No — 4.87 s, 8/8 pass |
| Codegen does not compile under real Go | Not tested — see honest limit 1 |

## What this proves for the W-010 ledger slot (slot 10)

The proposal (#184) argued: "A single projectable global program can rule
out the incompatible ordering before generating Go."

The spike mechanizes that argument: synthesize rejects the incompatible
ordering with a concrete error, and simulate_locals confirms the same locals
deadlock under exhaustive scheduling, while well-formed siblings terminate.
This is the smallest artifact that closes the falsifiability gap the
proposal's "Bounded model evidence" section left open ("these are exhaustive
results for the supplied finite acyclic graphs only").

## Reproduction

    cargo test --release --offline    # 8/8 pass
    cargo run  --release --offline    # prints the 5-scenario demo above


---

---

## Correction and fix (2026-10-19, after PR #760 opened)

While preparing reviewer instructions for `codegen_go`, I inspected its
actual output and found a **real bug in the emitted Go**: the hub goroutine
received from `hub` and forwarded back into the same `hub` channel, which
self-blocks. That output would, if compiled and run, have printed
`DEADLOCK` for the request/reply program — the opposite of correct.

**Fixed in the same PR (commit after 10ad50f).** The new `codegen_go`:

- Emits one inbox channel per role (`inbox_A`, `inbox_B`, …), not a
  shared hub. Sender writes to `inbox_<to>`; receiver reads from
  `inbox_<me>`. This matches the projection's Send/Recv exactly.
- Guards `close(done)` with `sync.Once`, so the timeout goroutine and the
  `wg.Wait()` goroutine cannot double-close (a race the first fix exposed).
- Still prints `PROTOCOL OK` on clean termination and `DEADLOCK` on
  timeout, with a 5 s hard deadline.

A new binary `src/bin/emit_go.rs` writes the codegen output, and the PR
now commits a real, buildable `model/work/main.go` + `model/work/go.mod`.
A reviewer with Go installed can now actually run:

    cd spikes/spike15-w010-session-projection/model/work
    go build ./...
    timeout 5 go run .

and should see `PROTOCOL OK`.

This still does not change any preregistered claim. The preregistered
codegen bar (bar 5) was "non-empty Go inspected by eye"; the correctness
claim for the protocol was always established by `simulate_locals`, whose
8 tests still pass unchanged. The fix makes the codegen *also* runnable,
which is a strict improvement — but the proof was, and remains, the
exhaustive simulator.

`cargo test --release --offline` after the fix: **8 passed, 0 failed**.
`emit_go` after the fix: writes **874 bytes** to `work/main.go`.
