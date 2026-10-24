#!/usr/bin/env bash
# tools/env.sh — pinned environment for every session (source me).
# Per SPEC.md §2: pin Go and Rust; do not float. Record changes in PLAN.md Decision Log.

export GO_TARBALL_ROOT="/home/z/tools/go"
export RUSTUP_HOME="/home/z/.rustup"
export CARGO_HOME="/home/z/.cargo"

export PATH="/home/z/tools/go/bin:/home/z/.cargo/bin:$PATH"

# Pinned toolchains (verified 2026-10-11, sandbox: 2 CPU / 4 GB RAM / 9.9 GB disk)
export PINNED_GO="go1.26.9"        # installed at /home/z/tools/go
export PINNED_RUST="1.99.0"        # via rustup, stable 2026-09-28

# Resource-governor defaults (SPEC.md §2): at most -j2 everywhere
export CARGO_BUILD_JOBS=2
export GOFLAGS="-p=2"
export MAKEFLAGS="-j2"
export CARGO_TARGET_DIR="/home/z/go-hybrid-target"   # one shared target dir
export CARGO_PROFILE_DEV_DEBUG=0
export RUSTFLAGS="-C link-arg=-Wl,--no-keep-memory"  # kinder linking on 4 GB RAM

# Kaggle CLI (venv) — token at ~/.kaggle/access_token (KGAT_..., chmod 600)
export KAGGLE_CLI="/home/z/.venv/bin/kaggle"

# Heavy-job lock (SPEC.md §2 Resource Governor): wrap builds/tests/benchmarks with
#   flock /tmp/heavy.lock nice -n10 timeout <secs> <command>
export HEAVY_LOCK="/tmp/heavy.lock"

# Verify pins quickly: `source tools/env.sh && tools/env.sh --check`
check() {
  local status=0
  local go_out rust_out

  go_out="$(go version 2>/dev/null)" || go_out=""
  if [[ -z "$go_out" ]]; then
    echo "WARN: go missing (expected $PINNED_GO)"
    status=1
  elif [[ " $go_out " != *" $PINNED_GO "* ]]; then
    echo "WARN: go != $PINNED_GO (got: $go_out)"
    status=1
  fi

  rust_out="$(rustc --version 2>/dev/null)" || rust_out=""
  if [[ -z "$rust_out" ]]; then
    echo "WARN: rustc missing (expected $PINNED_RUST)"
    status=1
  elif [[ " $rust_out " != *" $PINNED_RUST "* ]]; then
    echo "WARN: rustc != $PINNED_RUST (got: $rust_out)"
    status=1
  fi

  df -h / 2>/dev/null | tail -1 || true
  free -m 2>/dev/null | head -2 || true
  return "$status"
}

# ==== inline check when invoked directly ====
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  case "${1:-}" in
    --check)
      check
      exit $?
      ;;
    "")
      echo "Go:      $(go version 2>/dev/null || echo MISSING)"
      echo "Rust:    $(rustc --version 2>/dev/null || echo MISSING)"
      echo "Cargo:   $(cargo --version 2>/dev/null || echo MISSING)"
      echo "Kaggle:  $($KAGGLE_CLI --version 2>/dev/null || echo MISSING)"
      echo "Cores:   $(nproc 2>/dev/null || echo 1)   Disk: $(df -h / 2>/dev/null | tail -1 | awk '{print $4}' || echo UNKNOWN) free"
      ;;
    *)
      echo "usage: tools/env.sh [--check]" >&2
      exit 2
      ;;
  esac
fi
