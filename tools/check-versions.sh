#!/usr/bin/env bash
# tools/check-versions.sh — version-drift guard (Lisette check-go-versions.sh pattern).
#
# tools/env.sh is the SINGLE SOURCE OF TRUTH for the toolchain pins
# (PLAN.md Decision Log: "pin go1.26.9 / rust 1.99.0; do not float").
# Every other place that names a version must agree with it:
#   1. README.md quickstart line
#   2. hyb's compiled-in pin defaults (compiler/crates/hyb/src/main.rs)
#   3. PLAN.md environment row
# and the INSTALLED toolchains (when present) must match the pins exactly —
# the same contract `hyb build` enforces at runtime as HYB-E9001.
#
# Usage: tools/check-versions.sh          (exit 0 = consistent, 1 = drift)
#   tools/check-versions.sh --no-toolchain   (skip the installed-version probe,
#                                             for CI runners without the pins)
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# --- 1. Source of truth -----------------------------------------------------
# shellcheck disable=SC1091
source tools/env.sh >/dev/null 2>&1 || { echo "FAIL: cannot source tools/env.sh"; exit 1; }
: "${PINNED_GO:?PINNED_GO missing from tools/env.sh}"
: "${PINNED_RUST:?PINNED_RUST missing from tools/env.sh}"
GO_VER="${PINNED_GO#go}"   # 1.26.9 (README spells it without the leading `go`)

DRIFT=0
fail() { echo "FAIL: $*"; DRIFT=1; }
pass() { echo "ok:   $*"; }

# Versions are matched as LITERALS with a version boundary on both sides.
# A bare `grep -q "Go 1.26.9"` treats each `.` as "any character" and also
# matches a longer version (`Go 1.26.90`, or `Go 1.26.12` when the pin is
# 1.26.1), so a stale artifact could pass the drift check.
re_escape() { printf '%s' "$1" | sed 's/[][\.^$*+?(){}|/]/\\&/g'; }
# has_version FILE PREFIX VERSION: FILE contains PREFIX+VERSION (literal),
# not glued to a longer word before it and not followed by more version
# characters (a sentence-ending `.` is fine).
has_version() {
    local file="$1" prefix="$2" version="$3"
    local pat
    pat="(^|[^0-9A-Za-z_.])$(re_escape "$prefix$version")([^0-9A-Za-z_.]|\\.([^0-9]|$)|$)"
    grep -qE "$pat" "$file"
}

echo "pins: $PINNED_GO / rust $PINNED_RUST (tools/env.sh)"

# --- 2. Cross-reference every version-bearing artifact ----------------------
has_version README.md "Go " "$GO_VER" \
    && pass "README.md names Go $GO_VER" \
    || fail "README.md does not name Go $GO_VER"
has_version README.md "Rust " "$PINNED_RUST" \
    && pass "README.md names Rust $PINNED_RUST" \
    || fail "README.md does not name Rust $PINNED_RUST"

grep -qF "\"$PINNED_GO\"" compiler/crates/hyb/src/main.rs \
    && pass "hyb default pin matches ($PINNED_GO)" \
    || fail "hyb default pin (compiler/crates/hyb/src/main.rs) != $PINNED_GO"
grep -qF "\"$PINNED_RUST\"" compiler/crates/hyb/src/main.rs \
    && pass "hyb default pin matches (rust $PINNED_RUST)" \
    || fail "hyb default pin (compiler/crates/hyb/src/main.rs) != rust $PINNED_RUST"
grep -qF "const DEFAULT_PIN: &str = \"$PINNED_RUST\"" compiler/crates/hyb/build.rs \
    && pass "hyb drift-guard default matches (rust $PINNED_RUST)" \
    || fail "hyb build.rs DEFAULT_PIN != rust $PINNED_RUST (the guard would refuse honest builds)"

# PLAN.md: the environment row itself, not any historical mention in the log.
grep -qxF -- "- Go: **$PINNED_GO**" <(sed -E 's/^(- Go: \*\*[^*]*\*\*).*/\1/' PLAN.md) \
    && pass "PLAN.md environment row names $PINNED_GO" \
    || fail "PLAN.md does not name $PINNED_GO"
grep -qxF -- "- Rust: **$PINNED_RUST**" <(sed -E 's/^(- Rust: \*\*[^*]*\*\*).*/\1/' PLAN.md) \
    && pass "PLAN.md environment row names rust $PINNED_RUST" \
    || fail "PLAN.md does not name rust $PINNED_RUST"

# --- 3. Installed toolchains match the pins (skippable on bare CI) ----------
if [ "${1:-}" != "--no-toolchain" ]; then
    if command -v go >/dev/null 2>&1; then
        GO_PROBE_STATUS=0
        INSTALLED_GO="$(go version 2>/dev/null | awk '{print $3}')" || GO_PROBE_STATUS=$?
        if [ -z "$INSTALLED_GO" ]; then
            pass "go shim unusable — skipping (hyb build enforces at runtime)"
        elif [ "$GO_PROBE_STATUS" -ne 0 ]; then
            fail "go version probe failed (exit $GO_PROBE_STATUS)"
        elif [ "$INSTALLED_GO" = "$PINNED_GO" ]; then
            pass "installed go == $PINNED_GO"
        else
            fail "installed go ($INSTALLED_GO) != $PINNED_GO"
        fi
    else
        pass "go not installed — skipping (hyb build enforces at runtime)"
    fi
    if command -v rustc >/dev/null 2>&1; then
        RUST_PROBE_STATUS=0
        INSTALLED_RUST="$(rustc --version 2>/dev/null | awk '{print $2}')" || RUST_PROBE_STATUS=$?
        if [ -z "$INSTALLED_RUST" ]; then
            # A rustup shim with no default toolchain (bare CI images) — treat
            # as not installed; hyb build enforces the pin at runtime.
            pass "rustc shim has no default toolchain — skipping (hyb build enforces)"
        elif [ "$RUST_PROBE_STATUS" -ne 0 ]; then
            fail "rustc version probe failed (exit $RUST_PROBE_STATUS)"
        elif [ "$INSTALLED_RUST" = "$PINNED_RUST" ]; then
            pass "installed rustc == $PINNED_RUST"
        else
            fail "installed rustc ($INSTALLED_RUST) != $PINNED_RUST"
        fi
    else
        pass "rustc not installed — skipping (hyb build enforces at runtime)"
    fi
fi

# --- verdict ----------------------------------------------------------------
if [ "$DRIFT" -eq 0 ]; then
    echo "versions: CONSISTENT"
    exit 0
else
    echo "versions: DRIFT DETECTED — update tools/env.sh FIRST, then the artifacts above, in one commit"
    exit 1
fi
