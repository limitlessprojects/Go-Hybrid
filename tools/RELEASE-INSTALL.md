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

## Setup on Linux and macOS

```bash
export HYB_RUNTIME_DIR="$PWD/hybrid-<ver>/runtime"
export PATH="$PWD/hybrid-<ver>:$PATH"

hyb build program.hy -o app && ./app
```

`HYB_RUNTIME_DIR` must point at this release's `runtime/` directory (it
defaults to a source checkout only).

On Linux and macOS, the one-command installer sets up the launcher for you:

```bash
curl -fsSL https://raw.githubusercontent.com/limitlessprojects/Go-Hybrid/main/install.sh | sh
```

## Setup on Windows (PowerShell)

Extract the Windows release archive and open PowerShell in the extracted
directory containing `hyb.exe`. For builds, have the pinned Go toolchain and
MinGW-w64 GCC on PATH. The runtime path and PATH changes below apply to this
PowerShell session.

```powershell
$releaseDir = (Get-Location).Path
$env:HYB_RUNTIME_DIR = Join-Path $releaseDir 'runtime'
$env:Path = "$releaseDir;$env:Path"

hyb.exe build program.hy -o app.exe
if ($LASTEXITCODE -eq 0) { .\app.exe }
```

Run the build from the directory containing your program if it lives
outside the extracted release directory.

## Verify the claim, don't trust it

The full acceptance suite lives in the repo (`make examples` — all ten
target programs byte-exact; `make differential`; checker corpus 20/20).
Read the honest-claims paragraph in SPEC-LANGUAGE.md §9.2 (or
GUARANTEES.md, the line-by-line claims table) before relying on the safety
checks in production.
