# Relative runtime Cargo target paths

## Reproduction and fix

Base: `fca2f63afd3703581e6d1e097cb3b8616e5c6510`.

With `CARGO_TARGET_DIR='cache directory'`, `hyb vendor-runtime vendored`
looks for the staticlib under the caller's `cache directory/release`.
When rebuilding, it launches Cargo from `runtime/rust/hyrt` and previously
passed the relative value unchanged. Cargo successfully wrote the library
under `runtime/rust/hyrt/cache directory/release`, then `hyb` failed with
`no arch-matching hyrt staticlib found after cargo build`.

Resolve the explicit environment value to an absolute path before passing
it to Cargo. This does not require the target directory to exist, preserves
absolute values, and leaves Cargo's crate-local default when the variable
is unset. Staticlib selection and architecture checks are unchanged.

## Focused regression evidence

`compiler/crates/hyb/tests/vendor_runtime_target_dir.rs` runs the real CLI
with an isolated runtime and a fake Cargo executable. The executable
honors the target path and writes a host-architecture object fixture; this
tests the subprocess directory contract without compiling a whole runtime.
Every case also checks that a fresh library is reused on the next call.
Caller and target paths contain spaces. No process-global environment is
mutated by the tests.

Command: `cargo test --manifest-path compiler/Cargo.toml --locked -p hyb --test vendor_runtime_target_dir -j2`.

| Target setting | Base | Fixed |
| --- | --- | --- |
| `cache directory` | Fails after Cargo succeeds | Pass |
| `../shared target` | Fails after Cargo succeeds | Pass |
| Absolute directory | Pass | Pass |
| Variable unset | Pass | Pass |

The tests are Unix-only (their Cargo fixture is a shell script); the
production path resolution uses the standard library on all platforms.

## Real Cargo cross-check

On the fixed CLI, repeated the four cases using real Rust 1.99.0 Cargo and
a dependency-free fixture crate named `hyrt`, with `crate-type = ["staticlib"]`
and one exported function. Each first invocation successfully built and
vendored the real staticlib from the expected directory; each second
invocation reused it without running Cargo. No package download was needed.

Full workspace, corpus, differential and hosted CI validation are pending.
