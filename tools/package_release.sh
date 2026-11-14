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

# --- isolation: build crate-locally, smoke ONLY what the tarball carries ---
# tools/env.sh exports a shared CARGO_TARGET_DIR. Left set, cargo writes the
# fresh hyb and libhyrt.a there while step 3 copies the crate-local paths
# (target/release/, or target/$HYB_RUST_TARGET/release/ when cross-compiling),
# so a stale archive ships and is then touched "fresh".
# hyb's exe-relative runtime_dir() and arch-matched ensure_staticlib() do not
# close this: runtime_dir() still prefers HYB_RUNTIME_DIR when it is set,
# staticlib_candidates() still searches CARGO_TARGET_DIR ahead of the
# crate-local lib, and cargo_bin() still prefers HYB_CARGO_BIN. A smoke that
# inherits any of them can pass against a runtime, staticlib, or cargo that
# is not in the tarball.
unset CARGO_TARGET_DIR HYB_RUNTIME_DIR HYB_CARGO_BIN

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
HYRT_LIB="$REPO_ROOT/runtime/rust/hyrt/target/release/libhyrt.a"
if [ -n "$RUST_TARGET" ]; then
    HYRT_LIB="$REPO_ROOT/runtime/rust/hyrt/target/$RUST_TARGET/release/libhyrt.a"
fi
# Strip embedded LLVM/GNU LTO bitcode and command-line sections (__LLVM,__bitcode,
# __LLVM,__cmdline, __GNU_LTO, .llvmbc, .llvmcmd, .llvm.lto*, .gnu.lto_*,
# .gnu.debuglto_*, SHT_LLVM_LTO) in place so Go 1.26.11+ / 1.27.4+ cgo link-time
# checks accept the prebuilt archive inside the release tarball (#4470).
if [ -f "$HYRT_LIB" ] && command -v python3 >/dev/null 2>&1; then
    python3 - "$HYRT_LIB" <<'PY'
import struct, sys
from pathlib import Path

def is_bad_sec(name: bytes) -> bool:
    s = name.split(b"\x00", 1)[0].strip()
    return (
        s in (b".llvmbc", b".llvmcmd", b"__bitcode", b"__cmdline")
        or s.startswith((b".llvmbc.", b".llvmcmd.", b".llvm.lto", b".gnu.lto_", b".gnu.debuglto_"))
    )

def is_bad_seg(name: bytes) -> bool:
    s = name.split(b"\x00", 1)[0].strip()
    return s in (b"__LLVM", b"__GNU_LTO")

def scrub_macho(buf: memoryview) -> None:
    if len(buf) < 28:
        return
    m = struct.unpack_from("<I", buf, 0)[0]
    if m in (0xFEEDFACE, 0xFEEDFACF):
        end, is64 = "<", (m == 0xFEEDFACF)
    elif m in (0xCEFAEDFE, 0xCFFAEDFE):
        end, is64 = ">", (m == 0xCFFAEDFE)
    else:
        return
    hdr_sz = 32 if is64 else 28
    if len(buf) < hdr_sz:
        return
    filetype, ncmds, sizeofcmds = struct.unpack_from(end + "III", buf, 12)
    if filetype != 1:
        return
    cmd_end = min(len(buf), hdr_sz + sizeofcmds)
    off = hdr_sz
    for _ in range(ncmds):
        if off + 8 > cmd_end:
            break
        cmd, cmdsize = struct.unpack_from(end + "II", buf, off)
        if cmdsize < 8 or off + cmdsize > cmd_end:
            break
        if (is64 and cmd == 0x19 and cmdsize >= 72) or (not is64 and cmd == 0x1 and cmdsize >= 56):
            seg_hdr = 72 if is64 else 56
            sec_sz = 80 if is64 else 68
            segname = bytes(buf[off + 8 : off + 24])
            bad_seg = is_bad_seg(segname)
            if bad_seg:
                buf[off + 8 : off + 24] = b"\x00" * 16
            nsects = struct.unpack_from(end + "I", buf, off + (64 if is64 else 48))[0]
            for i in range(nsects):
                s = off + seg_hdr + i * sec_sz
                if s + sec_sz > off + cmdsize:
                    break
                sectname = bytes(buf[s : s + 16])
                s_segname = bytes(buf[s + 16 : s + 32])
                if bad_seg or is_bad_seg(s_segname) or is_bad_sec(sectname):
                    buf[s : s + 16] = b"__debug_noop\x00\x00\x00\x00"
                    buf[s + 16 : s + 32] = b"__DWARF\x00\x00\x00\x00\x00\x00\x00\x00\x00"
                    if is64:
                        struct.pack_into(end + "QIIIII", buf, s + 40, 0, 0, 0, 0, 0, 0x02000000)
                    else:
                        struct.pack_into(end + "IIIIII", buf, s + 36, 0, 0, 0, 0, 0, 0x02000000)
        off += cmdsize

def scrub_elf(buf: memoryview) -> None:
    if len(buf) < 52 or bytes(buf[:4]) != b"\x7fELF":
        return
    cls, data = buf[4], buf[5]
    end = "<" if data == 1 else (">" if data == 2 else None)
    if not end or cls not in (1, 2):
        return
    is64 = cls == 2
    if is64 and len(buf) < 64:
        return
    shoff = struct.unpack_from(end + ("Q" if is64 else "I"), buf, 40 if is64 else 32)[0]
    shentsize, shnum, shstrndx = struct.unpack_from(end + "HHH", buf, 58 if is64 else 46)
    min_sh = 64 if is64 else 40
    if shentsize < min_sh or shnum == 0 or shoff + shnum * shentsize > len(buf):
        return
    str_off = str_sz = 0
    if shstrndx < shnum:
        sb = shoff + shstrndx * shentsize
        str_off, str_sz = struct.unpack_from(end + ("QQ" if is64 else "II"), buf, sb + (24 if is64 else 16))
    for i in range(shnum):
        sh = shoff + i * shentsize
        sh_name, sh_type = struct.unpack_from(end + "II", buf, sh)
        bad = sh_type in (0x6FFF4C0C, 0x6FFF4C01, 0x6FFF4C04)
        name_pos = None
        if str_sz and sh_name < str_sz and str_off + str_sz <= len(buf):
            name_pos = str_off + sh_name
            nul = bytes(buf[name_pos : str_off + str_sz]).find(b"\x00")
            if nul >= 0 and is_bad_sec(bytes(buf[name_pos : name_pos + nul])):
                bad = True
                if nul >= 4:
                    buf[name_pos : name_pos + 4] = b".hyb"
        if bad:
            struct.pack_into(end + "I", buf, sh + 4, 1)
            if is64:
                struct.pack_into(end + "QQQQ", buf, sh + 8, 0, 0, 0, 0)
            else:
                struct.pack_into(end + "IIII", buf, sh + 8, 0, 0, 0, 0)

def scrub_coff(buf: memoryview) -> None:
    if len(buf) < 20:
        return
    machine, nsects, _, sym_ptr, nsyms, opt_sz = struct.unpack_from("<HHIIIH", buf, 0)
    if machine not in (0x8664, 0xAA64, 0x014C):
        return
    sec_off = 20 + opt_sz
    if sec_off + nsects * 40 > len(buf):
        return
    str_off = sym_ptr + nsyms * 18 if sym_ptr and nsyms else 0
    str_sz = struct.unpack_from("<I", buf, str_off)[0] if str_off and str_off + 4 <= len(buf) else 0
    for i in range(nsects):
        sh = sec_off + i * 40
        raw_name = bytes(buf[sh : sh + 8])
        bad = False
        if raw_name.startswith(b"/"):
            try:
                soff = int(raw_name[1:].split(b"\x00", 1)[0].strip())
            except ValueError:
                soff = 0
            if str_sz >= 4 and 4 <= soff < str_sz and str_off + str_sz <= len(buf):
                p = str_off + soff
                nul = bytes(buf[p : str_off + str_sz]).find(b"\x00")
                if nul >= 0 and is_bad_sec(bytes(buf[p : p + nul])):
                    bad = True
                    if nul >= 4:
                        buf[p : p + 4] = b".hyb"
        elif is_bad_sec(raw_name):
            bad = True
            buf[sh : sh + 8] = b".hyb_lto"
        if bad:
            struct.pack_into("<III", buf, sh + 16, 0, 0, 0)
            struct.pack_into("<H", buf, sh + 32, 0)
            struct.pack_into("<I", buf, sh + 36, 0x02000800)

def scrub_obj(buf: memoryview) -> None:
    if len(buf) < 4:
        return
    m = struct.unpack_from(">I", buf, 0)[0]
    if m in (0xCAFEBABE, 0xCAFEBABF):
        if len(buf) < 8:
            return
        narch = struct.unpack_from(">I", buf, 4)[0]
        esize = 32 if m == 0xCAFEBABF else 20
        for i in range(narch):
            e = 8 + i * esize
            if e + esize > len(buf):
                break
            off, sz = struct.unpack_from(">QQ" if m == 0xCAFEBABF else ">II", buf, e + 8)
            if off + sz <= len(buf):
                scrub_obj(buf[off : off + sz])
        return
    scrub_macho(buf)
    scrub_elf(buf)
    scrub_coff(buf)

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
if data.startswith(b"!<arch>\n"):
    view = memoryview(data)
    pos = 8
    while pos + 60 <= len(data):
        raw_name = bytes(view[pos : pos + 16])
        try:
            size = int(bytes(view[pos + 48 : pos + 58]).strip())
        except ValueError:
            break
        body_start = pos + 60
        body_end = body_start + size
        if body_end > len(data):
            break
        pos = body_end + (size & 1)
        obj_start = body_start
        mname = raw_name.strip()
        if raw_name.startswith(b"#1/"):
            try:
                nlen = int(raw_name[3:].strip())
            except ValueError:
                continue
            if nlen > size:
                continue
            mname = bytes(view[body_start : body_start + nlen]).rstrip(b"\x00")
            obj_start = body_start + nlen
        if mname in (b"/", b"//", b"/SYM64/") or mname.startswith(b"__.SYMDEF"):
            continue
        scrub_obj(view[obj_start:body_end])
    path.write_bytes(data)
PY
fi

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
