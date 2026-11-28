# Contract probes for wildcard #1755

This independent audit tests three prerequisites of
[#1755](https://github.com/limitlessprojects/Go-Hybrid/issues/1755) with a real
native Go assembler, executable controls, and explicit permission arithmetic.
It is a learning artifact; it does not implement the proposed replacement
checker, translate Rust to Go assembly, or claim a latency improvement.

On **native Linux ARM64 with Go 1.26.9 and Python 3.9+**:

```sh
python3 spikes/w1755-contract-falsifiers/run.py --output /tmp/w1755-new-run
```

Choose an output directory that does not exist. The runner requires the pinned
native toolchain, disables automatic toolchain downloads, uses only Go's standard
library and Python's standard library, and retains commands, return codes,
stdout, stderr, disassembly, input hashes, and a JSON summary. It exits nonzero
if a control fails, a tool is missing, or a negative probe fails for an unrelated
reason. The invalid ABIInternal fixture is assembled separately, never linked
or executed. The NOSPLIT control uses a small bounded call chain; it does not
attempt to overflow a stack.

The path-filtered `w1755-contracts` workflow repeats the runner on a native ARM64
runner and retains the textual evidence. Hosted CI must still run after a PR is
opened. See [RESULTS.md](RESULTS.md) for the committed pre-registration, decision
rules, observations, and limits.

Reference contracts:

- [Pinned Go ABI specification](https://github.com/golang/go/blob/go1.26.9/src/cmd/compile/abi-internal.md):
  ABIInternal versus ABI0, transparent wrappers, and architecture-specific
  registers. ARM64's goroutine register is R28; amd64's is R14. Matching some
  argument registers does not establish the entire foreign-call contract.
- [Pinned assembler selector restriction](https://github.com/golang/go/blob/go1.26.9/src/cmd/asm/internal/asm/parse.go):
  ordinary external assembly cannot simply select ABIInternal.
- [Current bridge descriptor ledger](../../interfaces/descriptors.md): the
  fast path already runs on the caller's goroutine stack after a headroom
  check. Removing that check still requires a bound on worst-case live stack
  use, together with appropriate available headroom at the call site.
