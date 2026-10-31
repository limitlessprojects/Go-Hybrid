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

# _env_has_token <string> <token> — exact whitespace-delimited token match.
# Replaces `rg -q "$PIN"` which (a) is absent on many images, (b) treats '.' as a
# regex wildcard, and (c) substring-matches, so "go1.26.90" satisfied a "go1.26.9" pin.
_env_has_token() {
  local t
  for t in $1; do
    [[ "$t" == "$2" ]] && return 0
  done
  return 1
}

# _env_version_of <string> — best-effort version token, for the failure message only.
_env_version_of() {
  local t out=""
  for t in $1; do
    case "$t" in
      go[0-9]*|[0-9]*.*.*) out="$t"; break ;;
    esac
  done
  printf '%s' "${out:-unknown}"
}

# Verify pins quickly: `source tools/env.sh && tools/env.sh --check`
# Returns non-zero if either pin is missing or mismatched (defect (c)).
check() {
  local rc=0 go_v rust_v
  go_v="$(go version 2>/dev/null)"
  rust_v="$(rustc --version 2>/dev/null)"

  if [[ -z "$go_v" ]]; then
    echo "WARN: go MISSING (want $PINNED_GO)"; rc=1
  elif ! _env_has_token "$go_v" "$PINNED_GO"; then
    echo "WARN: go != $PINNED_GO (found $(_env_version_of "$go_v"))"; rc=1
  else
    echo "OK:   go == $PINNED_GO"
  fi

  if [[ -z "$rust_v" ]]; then
    echo "WARN: rustc MISSING (want $PINNED_RUST)"; rc=1
  elif ! _env_has_token "$rust_v" "$PINNED_RUST"; then
    echo "WARN: rustc != $PINNED_RUST (found $(_env_version_of "$rust_v"))"; rc=1
  else
    echo "OK:   rustc == $PINNED_RUST"
  fi

  df -h / | tail -1
  free -m | head -2
  return "$rc"
}

# ==== inline check when invoked directly ====
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  case "${1:-}" in
    --check)
      shift
      if [[ $# -gt 0 ]]; then
        echo "env.sh: unexpected argument: $1" >&2
        echo "usage: tools/env.sh [--check]" >&2
        exit 2
      fi
      check
      exit $?
      ;;
    "")
      ;;
    *)
      echo "env.sh: unknown flag: $1" >&2
      echo "usage: tools/env.sh [--check]" >&2
      exit 2
      ;;
  esac
  echo "Go:      $(go version 2>/dev/null || echo MISSING)"
  echo "Rust:    $(rustc --version 2>/dev/null || echo MISSING)"
  echo "Cargo:   $(cargo --version 2>/dev/null || echo MISSING)"
  echo "Kaggle:  $($KAGGLE_CLI --version 2>/dev/null || echo MISSING)"
  echo "Cores:   $(nproc)   Disk: $(df -h / | tail -1 | awk '{print $4}') free"
fi
