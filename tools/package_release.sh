#!/bin/sh
# package_release.sh — reproducible release packaging (v0.1.0's tarball was
# assembled ad-hoc; this script is the lesson, not a repeat).
#
# Produces: dist/hybrid-<ver>-<os>-<arch>.tar.gz (+ .sha256)
#
# Layout (the contract install.sh relies on):
#   hybrid-<ver>/hyb                          the CLI binary (prebuilt)
#   hybrid-<ver>/INSTALL.md                   install notes (Rust-FREE path)
#   hybrid-<ver>/runtime/go/...               the Go binding packages
#   hybrid-<ver>/runtime/rust/hyrt/...        bridge sources (contrib path)
#   hybrid-<ver>/runtime/rust/hyrt/target/release/libhyrt.a
#          ^^^ PREBUILT staticlib, mtime >= every source — the freshness
#              check in hyb's ensure_staticlib() then reuses it and the
#              user NEVER needs a Rust toolchain. (Discussion #1, ask 2:
#              "If I have to build Rust first, I'll wait.")
#
# Usage: tools/package_release.sh v0.1.0
set -eu

VER="${1:?usage: package_release.sh <version-tag, e.g. v0.1.0>}"
# The version is a single path component in both the staging directory
# (removed with rm -rf below) and the archive name. Reject path separators
# and other non-filename characters before starting any build or write.
[ "$#" -eq 1 ] || { echo "invalid version: expected exactly one version tag" >&2; exit 2; }
case "$VER" in
    *[!a-zA-Z0-9._+-]* | .* | -* | *..*)
        echo "invalid version: use only ASCII letters, digits, '.', '_', '+', or '-'" >&2
        exit 2
        ;;
esac
VERNUM="${VER#v}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Cross-compile overrides (fleet release-matrix): HYB_RUST_TARGET (e.g.
# aarch64-unknown-linux-gnu / x86_64-pc-windows-gnu) + HYB_CC (the cross
# linker) + HYB_PKG_OS/ARCH + HYB_QEMU (the runner prefix for a foreign-arch
# smoke, e.g. "qemu-aarch64 -L /usr/aarch64-linux-gnu").
RUST_TARGET="${HYB_RUST_TARGET:-}"
CROSS_CC="${HYB_CC:-}"
QEMU_RUNNER="${HYB_QEMU:-}"
PKG_OS="${HYB_PKG_OS:-$(uname -s | tr '[:upper:]' '[:lower:]')}"
ARCH="${HYB_PKG_ARCH:-$(uname -m)}"
case "$ARCH" in
    x86_64) ARCH=amd64 ;;
    aarch64) ARCH=arm64 ;;
    arm64) ARCH=arm64 ;;
esac
DIST="$REPO_ROOT/dist"
NAME="hybrid-${VER}-${PKG_OS}-${ARCH}"
DIR="$DIST/hybrid-${VER}"
# cargo's per-target linker env var, derived from the triple:
# aarch64-unknown-linux-gnu -> CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER
if [ -n "$RUST_TARGET" ]; then
    TARGET_VAR="CARGO_TARGET_$(echo "$RUST_TARGET" | tr 'a-z-' 'A-Z_')_LINKER"
    CROSS_ENV="$TARGET_VAR=$CROSS_CC"
else
    CROSS_ENV=""
fi

echo "==> packaging $NAME"

# --- 0. toolchain sanity (the pins are the contract; HYB-E9001 enforces) ---
command -v cargo >/dev/null || { echo "cargo not on PATH" >&2; exit 1; }
command -v go >/dev/null || { echo "go not on PATH" >&2; exit 1; }

# --- 1. build the staticlib CRATE-LOCALLY (env.sh's shared CARGO_TARGET_DIR
#        redirects manual builds — the stale-archive trap, worklog session 8).
#        The lib must land at runtime/rust/hyrt/target/release/.
#        Cross mode: --target $RUST_TARGET puts it under target/<triple>/ —
#        step 3 re-lays it into the packaged tree either way. ---
echo "==> building hyrt staticlib (crate-local)"
( cd "$REPO_ROOT/runtime/rust/hyrt" && \
  if [ -n "$RUST_TARGET" ]; then \
      env CARGO_BUILD_JOBS=2 $CROSS_ENV cargo build --release --target "$RUST_TARGET" --quiet; \
  else \
      CARGO_BUILD_JOBS=2 cargo build --release --quiet; fi )

# --- 2. build the CLI (same target as the staticlib — the hyb BINARY must
#        run on the target arch) ---
echo "==> building hyb (release)"
( cd "$REPO_ROOT/compiler" && \
  if [ -n "$RUST_TARGET" ]; then \
      env CARGO_BUILD_JOBS=2 $CROSS_ENV cargo build --release -p hyb --target "$RUST_TARGET" --quiet; \
  else \
      cargo build --release -p hyb --quiet; fi )
HYB_BIN="$REPO_ROOT/compiler/target/release/hyb"
if [ -n "$RUST_TARGET" ]; then
    HYB_BIN="$REPO_ROOT/compiler/target/$RUST_TARGET/release/hyb"
fi
# windows: cargo emits hyb.exe; the packaged name follows the OS convention
HYB_OUT_NAME=hyb
[ "$PKG_OS" = "windows" ] && HYB_OUT_NAME=hyb.exe
[ -f "$HYB_BIN" ] || HYB_BIN="${HYB_BIN}.exe"
[ -f "$HYB_BIN" ] || { echo "hyb binary not found after build" >&2; exit 1; }

# --- 3. assemble the tree ---
rm -rf "$DIR"
mkdir -p "$DIR"
cp "$HYB_BIN" "$DIR/$HYB_OUT_NAME"
cp "$REPO_ROOT/tools/RELEASE-INSTALL.md" "$DIR/INSTALL.md"
mkdir -p "$DIR/runtime"
cp -R "$REPO_ROOT/runtime/go" "$DIR/runtime/go"
cp -R "$REPO_ROOT/runtime/rust" "$DIR/runtime/rust"
# strip the crate's build dir except the prebuilt staticlib, then re-lay it
find "$DIR/runtime/rust" -type d -name target -exec rm -rf {} + 2>/dev/null || true
mkdir -p "$DIR/runtime/rust/hyrt/target/release"
if [ -n "$RUST_TARGET" ]; then
    cp "$REPO_ROOT/runtime/rust/hyrt/target/$RUST_TARGET/release/libhyrt.a" \
       "$DIR/runtime/rust/hyrt/target/release/libhyrt.a"
else
    cp "$REPO_ROOT/runtime/rust/hyrt/target/release/libhyrt.a" \
       "$DIR/runtime/rust/hyrt/target/release/libhyrt.a"
fi
# THE mtime contract: the shipped lib must be >= every source in the crate,
# or ensure_staticlib() rebuilds and a Rust-less install dies on first build.
find "$DIR/runtime/rust/hyrt" -name 'target' -prune -o -print | while read -r f; do
    [ -f "$f" ] && touch "$f"
done
touch "$DIR/runtime/rust/hyrt/target/release/libhyrt.a"

# --- 4. tar + checksum ---
mkdir -p "$DIST"
( cd "$DIST" && tar czf "$NAME.tar.gz" "hybrid-${VER}" )
if command -v sha256sum >/dev/null; then
    ( cd "$DIST" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256" )
else
    ( cd "$DIST" && shasum -a 256 "$NAME.tar.gz" > "$NAME.tar.gz.sha256" )
fi

# --- 5. smoke test from the ACTUAL tarball with cargo REMOVED from PATH ---
# (the reply's bar: no Rust toolchain, ever, for users. The shipped
#  staticlib's mtime must win ensure_staticlib()'s freshness check.)
echo "==> smoke test from the tarball (PATH without cargo)"
SMOKE="$(mktemp -d)"
( cd "$SMOKE" && tar xzf "$DIST/$NAME.tar.gz" )
# BSD paste (macOS) can't read stdin without a file operand — filter in shell
RUSTLESS_PATH=""
OLDIFS="$IFS"; IFS="::"
for p in $PATH; do
    # Toolchains need not live under ~/.cargo/bin (e.g. custom CI installs).
    if [ ! -x "$p/cargo" ] && [ ! -x "$p/cargo.exe" ]; then
        RUSTLESS_PATH="${RUSTLESS_PATH:+$RUSTLESS_PATH:}$p"
    fi
done
IFS="$OLDIFS"
# cross linux/arm64: run the arm64 hyb under qemu (binfmt has NO sysroot —
# the -L flag is mandatory or even the CLI can't exec), build via GOARCH +
# the cross CC, execute under qemu. Since the aarch64 bridge port (PLAN row
# 16: call_arm64.s) the asset is FULL — so the smoke has TWO legs:
#   (a) bridge-free CLI smoke (01-hello), and
#   (b) a BRIDGE smoke (05-json-imports rust/serde_json): the emitted cgo
#       build links the tarball's prebuilt ARM64 libhyrt.a, assembles
#       call_arm64.s (GOARCH=arm64 selects it), and the run drives real
#       trampoline crossings (fast + pooled-deep paths) under qemu.
#       A linux/arm64 asset whose bridge smoke fails must not ship.
CROSS_GO_ENV=""
if [ "$PKG_OS/$ARCH" = "linux/arm64" ] && [ -n "$RUST_TARGET" ]; then
    CROSS_GO_ENV="GOARCH=arm64 CGO_ENABLED=1 CC=$CROSS_CC"
    QEMU_RUNNER="${QEMU_RUNNER:-qemu-aarch64}"
    # qemu-user-static (the fleet's package) ships *-static names only
    first="${QEMU_RUNNER%% *}"
    if ! command -v "$first" >/dev/null 2>&1; then
        rest="${QEMU_RUNNER#* }"
        if [ "$rest" != "$first" ] && command -v "${first}-static" >/dev/null 2>&1; then
            QEMU_RUNNER="${first}-static $rest"
        fi
    fi
fi
if ( cd "$SMOKE" && env PATH="$RUSTLESS_PATH" $CROSS_GO_ENV $QEMU_RUNNER \
        "$SMOKE/hybrid-${VER}/$HYB_OUT_NAME" build \
        "$REPO_ROOT/examples/01-hello/program.hy" -o "$SMOKE/hello" ) \
    && $QEMU_RUNNER "$SMOKE/hello" > "$SMOKE/out.txt" 2>&1 \
    && grep -q 'hello, Hybrid!' "$SMOKE/out.txt"; then
    echo "    smoke PASS ($PKG_OS/$ARCH${QEMU_RUNNER:+ under $QEMU_RUNNER}): $(head -1 "$SMOKE/out.txt")"
else
    echo "SMOKE FAILED ($PKG_OS/$ARCH)" >&2
    cat "$SMOKE/out.txt" 2>/dev/null >&2
    exit 1
fi

# --- 5b. bridge smoke (cross linux/arm64 and native macOS assets) ---
# Same tarball, same rustless PATH: a rust/serde_json-importing program
# must build and RUN correctly. macOS must exercise Mach-O archive discovery
# and the Darwin stack pool; a bridge-free hello cannot detect either failure.
if { [ "$PKG_OS/$ARCH" = "linux/arm64" ] && [ -n "$RUST_TARGET" ]; } || [ "$PKG_OS" = "darwin" ]; then
    echo "==> bridge smoke from the tarball (serde_json crossing on $PKG_OS/$ARCH)"
    if ( cd "$SMOKE" && env PATH="$RUSTLESS_PATH" $CROSS_GO_ENV $QEMU_RUNNER \
            "$SMOKE/hybrid-${VER}/$HYB_OUT_NAME" build \
            "$REPO_ROOT/examples/05-json-transform/program.hy" -o "$SMOKE/json" ) \
        && $QEMU_RUNNER "$SMOKE/json" > "$SMOKE/json.txt" 2>&1 \
        && grep -q '"ada"' "$SMOKE/json.txt"; then
        echo "    bridge smoke PASS ($PKG_OS/$ARCH trampolines exercised): $(tr '\n' ' ' < "$SMOKE/json.txt" | tr -s ' ')"
    else
        echo "BRIDGE SMOKE FAILED ($PKG_OS/$ARCH — the asset is FULL only if this passes)" >&2
        cat "$SMOKE/json.txt" 2>/dev/null >&2
        exit 1
    fi
fi
rm -rf "$SMOKE"

echo "==> done:"
ls -la "$DIST/$NAME.tar.gz" "$DIST/$NAME.tar.gz.sha256"
