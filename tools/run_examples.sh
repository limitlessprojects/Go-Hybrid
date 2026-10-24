#!/usr/bin/env bash
# tools/run_examples.sh — MVP-0 walking-skeleton acceptance.
#
# Builds the Go-emittable examples through the FULL pipeline
# (parse → Tier-0 check → emit → go build) and diffs their runtime output
# against each example's expected.txt. Example 02 exercises both the happy
# path (stdout = bytes verbatim) and the E6 `?`-exit contract (the binary is
# built as `cat` so the <prog>: prefix matches the pinned expected output).
#
# Usage: tools/run_examples.sh [01 02 03]   (default: all emittable)
# Exit:  0 = all green, 1 = any mismatch.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-/home/z/go-hybrid-target}"
HYB="${HYB_BIN:-$TARGET_DIR/debug/hyb}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

FAIL=0
PASS=""

note() { printf '%s\n' "$*"; }

show_run_failure() {
    local name="$1"
    if [ -f "$WORK/$name.diff" ]; then
        sed 's/^/    /' "$WORK/$name.diff"
    fi
    if [ -s "$WORK/$name.err" ]; then
        note "    stderr:"
        sed 's/^/    /' "$WORK/$name.err"
    fi
}

run_example() {
    local dir="$1"
    local name
    name="$(basename "$dir")"
    local prog="$dir/program.hy"
    local bin="$WORK/$name"

    note "--- $name"
    # 09 is package mode (no main fn) — it builds its own workspace below.
    if [ "$name" != "09-go-imports-hybrid" ]; then
        if ! "$HYB" build "$prog" -o "$bin" >"$WORK/$name.build.log" 2>&1; then
            note "FAIL $name (build)"
            sed 's/^/    /' "$WORK/$name.build.log"
            FAIL=1
            return
        fi
    fi

    case "$name" in
    01-hello|03-worker-pool|04-pipeline-backpressure|05-json-transform|06-http-server|07-lru-generics|08-graph-ownership)
        # Runtime failure is diagnosed as such (run exit N + captured output),
        # not mislabeled as an output diff; diffing only makes sense at exit 0.
        local rc
        "$bin" >"$WORK/$name.out" 2>"$WORK/$name.err"
        rc=$?
        if [ "$rc" -ne 0 ]; then
            note "FAIL $name (run exit $rc)"
            sed 's/^/    /' "$WORK/$name.out"
            show_run_failure "$name"
            FAIL=1
            return
        fi
        if diff -u "$dir/expected.txt" "$WORK/$name.out" >"$WORK/$name.diff"; then
            note "PASS $name (stdout == expected.txt)"
            PASS="$PASS $name"
        else
            note "FAIL $name (output diff)"
            show_run_failure "$name"
            FAIL=1
        fi
        ;;
    10-rust-crate-import)
        # The bridge program: lines 1–3 are exact (digest prefixes verified
        # against an independent sha3 implementation); line 4's TIMING is a
        # budget, not a fixed number (example comment) — the runner masks
        # the milliseconds field before diffing. The checksum IS exact
        # (deterministic per pinned crate version + fold algorithm).
        local rc
        "$bin" >"$WORK/$name.out" 2>"$WORK/$name.err"
        rc=$?
        if [ "$rc" -ne 0 ]; then
            note "FAIL $name (run exit $rc)"
            sed 's/^/    /' "$WORK/$name.out"
            show_run_failure "$name"
            FAIL=1
            return
        fi
        sed -E 's/^1M hashes in [0-9]+ms,/1M hashes in 0ms,/' "$WORK/$name.out" >"$WORK/$name.masked"
        if diff -u "$dir/expected.txt" "$WORK/$name.masked" >"$WORK/$name.diff"; then
            note "PASS $name (digests exact; timing masked; checksum exact)"
            PASS="$PASS $name"
        else
            note "FAIL $name (output diff)"
            show_run_failure "$name"
            FAIL=1
        fi
        ;;
    09-go-imports-hybrid)
        # THE ADOPTION STORY: a plain Go program imports a Hybrid-compiled
        # package (hyb --package emits the Go source + runtime pieces; the
        # consumer needs NO Hybrid toolchain to build, only Go).
        local ws="$WORK/09-ws"
        mkdir -p "$ws/hybrid"
        if ! "$HYB" build "$prog" --package hybrid --out-dir "$ws/hybrid" \
             --binding-prefix example.com/app/hybrid >"$WORK/$name.pkg.log" 2>&1; then
            note "FAIL $name (package build)"
            sed 's/^/    /' "$WORK/$name.pkg.log"
            FAIL=1
            return
        fi
        printf 'module example.com/app\n\ngo 1.26\n' >"$ws/go.mod"
        cp "$dir/main.go.txt" "$ws/main.go"
        # Same contract as the standard examples: a non-zero `go run` is a
        # run failure with its output, not an output diff.
        local rc
        (cd "$ws" && go run . >"$WORK/$name.out" 2>"$WORK/$name.err")
        rc=$?
        if [ "$rc" -ne 0 ]; then
            note "FAIL $name (run exit $rc)"
            sed 's/^/    /' "$WORK/$name.out"
            show_run_failure "$name"
            FAIL=1
            return
        fi
        if diff -u "$dir/expected.txt" "$WORK/$name.out" >"$WORK/$name.diff"; then
            note "PASS $name (Go consumer imports the Hybrid package; rust/regex through the boundary)"
            PASS="$PASS $name"
        else
            note "FAIL $name (output diff)"
            show_run_failure "$name"
            FAIL=1
        fi
        ;;
    02-cli-cat)
        # E6 exit contract needs <prog> = basename(argv[0]) — build as `cat`.
        local catbin="$WORK/catbin/cat"
        mkdir -p "$WORK/catbin"
        if ! "$HYB" build "$prog" -o "$catbin" >"$WORK/$name.build.log" 2>&1; then
            note "FAIL $name (cat build)"
            sed 's/^/    /' "$WORK/$name.build.log"
            FAIL=1
            return
        fi
        printf 'hello from the file\nsecond line\n' >"$WORK/in.txt"
        local ok=1
        # happy path: stdout == input bytes verbatim, exit 0
        if "$catbin" "$WORK/in.txt" >"$WORK/$name.out" 2>"$WORK/$name.err" && \
           diff -u "$WORK/in.txt" "$WORK/$name.out" >/dev/null; then
            note "PASS $name happy path (bytes verbatim, exit 0)"
        else
            note "FAIL $name happy path"
            ok=0
        fi
        # error path: stderr "<prog>: open <missing>: no such file...", exit 1
        local missing="$WORK/definitely-not-here.txt"
        "$catbin" "$missing" >"$WORK/$name.out2" 2>"$WORK/$name.err2"
        local rc=$?
        local want="cat: open $missing: no such file or directory"
        if [ "$rc" -eq 1 ] && [ "$(cat "$WORK/$name.err2")" = "$want" ]; then
            note "PASS $name error path (E6: stderr + exit 1)"
        else
            note "FAIL $name error path (rc=$rc, stderr: $(cat "$WORK/$name.err2"))"
            ok=0
        fi
        if [ "$ok" -eq 1 ]; then PASS="$PASS $name"; else FAIL=1; fi
        ;;
    *)
        note "FAIL $name (unsupported acceptance example; no runtime oracle)"
        FAIL=1
        ;;
    esac
}

if [ $# -eq 0 ]; then
    set -- 01-hello 02-cli-cat 03-worker-pool 04-pipeline-backpressure 05-json-transform 06-http-server 07-lru-generics 08-graph-ownership 09-go-imports-hybrid 10-rust-crate-import
fi

for name in "$@"; do
    dir="$ROOT/examples/$name"
    if [ ! -f "$dir/program.hy" ]; then
        note "FAIL $name (no program.hy at $dir)"
        FAIL=1
        continue
    fi
    run_example "$dir"
done

note ""
if [ "$FAIL" -eq 0 ]; then
    note "examples: ALL PASS ($PASS)"
    exit 0
else
    note "examples: FAILURES (passed:$PASS)"
    exit 1
fi
