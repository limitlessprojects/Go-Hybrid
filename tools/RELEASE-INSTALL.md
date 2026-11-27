# Hybrid — install notes

## What's in here

- `hyb` — the Hybrid compiler CLI (prebuilt, this platform)
- `runtime/` — the Go binding packages `hyb build` assembles into your
  workspace, plus the bridge sources **and the prebuilt bridge staticlib**
  (`runtime/rust/hyrt/target/release/libhyrt.a`)

## Requirements

- **Go 1.26.9** (pinned; `hyb` enforces the pin — HYB-E9001 on drift).
  You are a Go developer; this is already your toolchain.
- A C linker (ordinary Go/cgo requirement).
- **No Rust toolchain needed.** The bridge staticlib ships prebuilt; `hyb`
  reuses it unless you modify the bridge sources yourself (contributors
  only — rebuild with Rust 1.99.0, pinned).

## Setup (two lines)

```bash
export HYB_RUNTIME_DIR="$PWD/hybrid-<ver>/runtime"
export PATH="$PWD/hybrid-<ver>:$PATH"

hyb build program.hy -o app && ./app
```

`HYB_RUNTIME_DIR` must point at this release's `runtime/` directory (it
defaults to a source checkout only).

Or use the one-command installer, which wires both lines for you
(Linux and macOS only — `install.sh` exits "unsupported OS" on Windows;
use the manual `HYB_RUNTIME_DIR`/`PATH` setup above there):

```bash
curl -fsSL https://raw.githubusercontent.com/limitlessprojects/Go-Hybrid/main/install.sh | sh
```

## Verify the claim, don't trust it

The full acceptance suite lives in the repo (`make examples` — all ten
target programs byte-exact; `make differential`; checker corpus 20/20).
Read the honest-claims paragraph in SPEC-LANGUAGE.md §9.2 (or
GUARANTEES.md, the line-by-line claims table) before relying on the safety
checks in production.
