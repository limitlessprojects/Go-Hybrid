#!/usr/bin/env python3
"""Offline tests for tools/package_release.sh.

The script runs in a throwaway copy of the layout it reads, with fake
`cargo` / `go` shims. No Rust or Go toolchain, network, or real release
build is involved. The fake cargo honours CARGO_TARGET_DIR and `--target`
the way real cargo does. The fake hyb it emits records the environment the
smoke test hands it.

What the current script must guarantee:

- A clean build ships the artifacts cargo just wrote, a matching sha256,
  and a staticlib whose mtime is at least as new as the crate sources
  (ensure_staticlib's freshness check).
- CARGO_TARGET_DIR must not redirect those artifacts away from the
  crate-local paths step 3 copies. Otherwise the stale files seeded at
  target/release/ ship.
- `HYB_RUST_TARGET` ships that triple's build, re-laid at
  runtime/rust/hyrt/target/release/, not the native tree.
- The rustless smoke executes hyb from the extracted tarball. Its PATH has
  no directory containing cargo or cargo.exe. It also must not inherit
  HYB_RUNTIME_DIR, HYB_CARGO_BIN, or CARGO_TARGET_DIR: hyb still prefers
  those over exe-relative runtime discovery and the tarball's staticlib.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools" / "package_release.sh"
VERSION = "v9.9.9-test"
PKG_OS = "linux"
PKG_ARCH = "amd64"

# The hyb the fake cargo emits. `build <src> -o <out>` writes a hello program
# and logs the env vars the real CLI consults (runtime_dir, cargo_bin,
# staticlib_candidates) plus PATH, so the test can see what the smoke leaked.
FAKE_HYB = r"""#!/bin/sh
# BUILD_MARK
log="${HYB_TEST_LOG:?}"
printf '{"runtime": "%s", "cargo_bin": "%s", "target_dir": "%s", "exe": "%s", "path": "%s"}\n' \
    "${HYB_RUNTIME_DIR-<unset>}" "${HYB_CARGO_BIN-<unset>}" \
    "${CARGO_TARGET_DIR-<unset>}" "$0" "$PATH" >> "$log"
out=""
while [ $# -gt 0 ]; do
    if [ "$1" = "-o" ]; then out="$2"; shift; fi
    shift
done
printf '#!/bin/sh\necho "hello, Hybrid!"\n' > "$out"
chmod +x "$out"
"""

# Fake cargo: writes under ${CARGO_TARGET_DIR:-./target}, and under
# target/<triple>/release when `--target` is set.
FAKE_CARGO = r"""#!/bin/sh
target="${CARGO_TARGET_DIR:-$PWD/target}"
triple=""
pkg=""
while [ $# -gt 0 ]; do
    case "$1" in
        --target) triple="$2"; shift ;;
        -p) pkg="$2"; shift ;;
    esac
    shift
done
dir="$target/${triple:+$triple/}release"
mkdir -p "$dir"
mark="FRESH-HYB${triple:+ $triple}"
if [ "$pkg" = "hyb" ]; then
    sed "s/BUILD_MARK/$mark/" "$HYB_TEST_FAKE_HYB" > "$dir/hyb"
    chmod +x "$dir/hyb"
else
    printf 'FRESH-LIB%s\n' "${triple:+ $triple}" > "$dir/libhyrt.a"
fi
"""


class PackageReleaseIsolationTests(unittest.TestCase):
    def setUp(self):
        if not shutil.which("sh") or not shutil.which("tar"):
            self.skipTest("needs sh and tar")
        if not shutil.which("sha256sum") and not shutil.which("shasum"):
            self.skipTest("package_release.sh needs sha256sum or shasum")
        tmp = tempfile.TemporaryDirectory(prefix="hybrid-package-test-")
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.repo = self.root / "repo"
        (self.repo / "tools").mkdir(parents=True)
        shutil.copy(SCRIPT, self.repo / "tools" / "package_release.sh")
        (self.repo / "tools" / "RELEASE-INSTALL.md").write_text("install notes\n")
        (self.repo / "runtime" / "go" / "hybridge").mkdir(parents=True)
        (self.repo / "runtime" / "go" / "hybridge" / "doc.go").write_text("package hybridge\n")
        hyrt = self.repo / "runtime" / "rust" / "hyrt"
        (hyrt / "src").mkdir(parents=True)
        (hyrt / "Cargo.toml").write_text("[package]\nname = \"hyrt\"\n")
        (hyrt / "src" / "lib.rs").write_text("// hyrt\n")
        (self.repo / "compiler").mkdir()
        (self.repo / "examples" / "01-hello").mkdir(parents=True)
        (self.repo / "examples" / "01-hello" / "program.hy").write_text("fn main() {}\n")

        # Old artifacts sit at the crate-local paths the script copies from.
        (hyrt / "target" / "release").mkdir(parents=True)
        (hyrt / "target" / "release" / "libhyrt.a").write_text("STALE-LIB\n")
        stale_hyb = self.repo / "compiler" / "target" / "release" / "hyb"
        stale_hyb.parent.mkdir(parents=True)
        stale_hyb.write_text(FAKE_HYB.replace("BUILD_MARK", "STALE-HYB"))
        stale_hyb.chmod(0o755)

        fake_hyb = self.root / "fake_hyb.sh"
        fake_hyb.write_text(FAKE_HYB)
        # cargo lives under a .cargo/bin dir so the smoke's PATH filter drops
        # it even when the toolchain is not named exactly that.
        cargo_bin = self.root / "home" / ".cargo" / "bin"
        cargo_bin.mkdir(parents=True)
        (cargo_bin / "cargo").write_text(FAKE_CARGO)
        (cargo_bin / "cargo").chmod(0o755)
        # The current filter also drops a directory that contains cargo.exe.
        win_cargo = self.root / "win-cargo"
        win_cargo.mkdir()
        (win_cargo / "cargo.exe").write_text("#!/bin/sh\nexit 0\n")
        (win_cargo / "cargo.exe").chmod(0o755)
        gobin = self.root / "gobin"
        gobin.mkdir()
        (gobin / "go").write_text("#!/bin/sh\nexit 0\n")
        (gobin / "go").chmod(0o755)

        self.cargo_bin = cargo_bin
        self.win_cargo = win_cargo
        self.gobin = gobin
        self.log = self.root / "hyb-env.log"
        self.dist_name = f"hybrid-{VERSION}-{PKG_OS}-{PKG_ARCH}.tar.gz"
        self.env = {
            "PATH": os.pathsep.join([
                str(win_cargo), str(cargo_bin), str(gobin), os.defpath,
            ]),
            "HOME": str(self.root / "home"),
            "TMPDIR": str(self.root),
            "HYB_TEST_LOG": str(self.log),
            "HYB_TEST_FAKE_HYB": str(fake_hyb),
            "HYB_PKG_OS": PKG_OS,
            "HYB_PKG_ARCH": PKG_ARCH,
        }

    def package(self, **extra):
        result = subprocess.run(
            ["sh", str(self.repo / "tools" / "package_release.sh"), VERSION],
            env={**self.env, **extra}, cwd=self.root,
            capture_output=True, text=True, timeout=60, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def tarball(self):
        return self.repo / "dist" / self.dist_name

    def shipped(self, member):
        with tarfile.open(self.tarball()) as tar:
            extracted = tar.extractfile(f"hybrid-{VERSION}/{member}")
            self.assertIsNotNone(extracted, member)
            return extracted.read().decode()

    def smoke_env(self):
        self.assertTrue(self.log.is_file(), "the smoke never ran hyb")
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_clean_env_ships_fresh_artifacts_checksum_and_fresh_lib(self):
        self.package()
        self.assertEqual(
            self.shipped("runtime/rust/hyrt/target/release/libhyrt.a"),
            "FRESH-LIB\n",
        )
        self.assertIn("FRESH-HYB", self.shipped("hyb"))
        self.assertNotIn("STALE-HYB", self.shipped("hyb"))
        self.assertEqual(self.shipped("INSTALL.md"), "install notes\n")

        blob = self.tarball().read_bytes()
        digest = hashlib.sha256(blob).hexdigest()
        recorded = (self.repo / "dist" / f"{self.dist_name}.sha256").read_text()
        self.assertIn(digest, recorded)
        self.assertIn(self.dist_name, recorded)

        with tarfile.open(self.tarball()) as tar:
            lib_mtime = None
            newest_source = 0
            prefix = f"hybrid-{VERSION}/runtime/rust/hyrt/"
            for member in tar.getmembers():
                if not member.name.startswith(prefix) or not member.isfile():
                    continue
                if member.name.endswith("/target/release/libhyrt.a"):
                    lib_mtime = member.mtime
                elif "/target/" not in member.name:
                    newest_source = max(newest_source, member.mtime)
        self.assertIsNotNone(lib_mtime)
        self.assertGreaterEqual(lib_mtime, newest_source)

    def test_shared_cargo_target_dir_does_not_ship_stale_artifacts(self):
        # env.sh exports CARGO_TARGET_DIR. Cargo then writes there, while the
        # script copies the crate-local paths seeded with STALE-* above.
        shared = self.root / "shared-target"
        self.package(CARGO_TARGET_DIR=str(shared))
        self.assertEqual(
            self.shipped("runtime/rust/hyrt/target/release/libhyrt.a"),
            "FRESH-LIB\n",
        )
        self.assertIn("FRESH-HYB", self.shipped("hyb"))
        self.assertNotIn("STALE-HYB", self.shipped("hyb"))
        # Step 3 copies these crate-local paths, so the fresh build has to
        # land here rather than only in the shared target dir.
        self.assertEqual(
            (self.repo / "runtime" / "rust" / "hyrt" / "target" / "release" / "libhyrt.a").read_text(),
            "FRESH-LIB\n",
        )

    def test_cross_target_ships_triple_artifacts_not_the_native_tree(self):
        triple = "aarch64-unknown-linux-gnu"
        # PKG_ARCH stays amd64, so this is the cross *build* path only: the
        # linux/arm64 bridge smoke (which needs qemu and example 05) stays off.
        self.package(HYB_RUST_TARGET=triple, HYB_CC="cc")
        self.assertEqual(
            self.shipped("runtime/rust/hyrt/target/release/libhyrt.a"),
            f"FRESH-LIB {triple}\n",
        )
        shipped_hyb = self.shipped("hyb")
        self.assertIn(f"FRESH-HYB {triple}", shipped_hyb)
        self.assertNotIn("STALE-HYB", shipped_hyb)

    def test_smoke_runs_tarball_hyb_without_external_overrides(self):
        # A caller runtime, an absolute cargo, or a shared target dir would
        # let the smoke pass on something the tarball does not contain.
        # Exe-relative discovery never runs while HYB_RUNTIME_DIR is set.
        elsewhere = self.root / "dev-runtime"
        elsewhere.mkdir()
        self.package(
            HYB_RUNTIME_DIR=str(elsewhere),
            HYB_CARGO_BIN=str(self.cargo_bin / "cargo"),
            CARGO_TARGET_DIR=str(self.root / "shared-target"),
        )
        runs = self.smoke_env()
        self.assertTrue(runs, "the smoke never ran hyb")
        for run in runs:
            self.assertEqual(run["runtime"], "<unset>", run)
            self.assertEqual(run["cargo_bin"], "<unset>", run)
            self.assertEqual(run["target_dir"], "<unset>", run)
            self.assertIn(f"/hybrid-{VERSION}/hyb", run["exe"])
            parts = run["path"].split(":")
            self.assertNotIn(str(self.cargo_bin), parts, run)
            self.assertNotIn(str(self.win_cargo), parts, run)
            self.assertIn(str(self.gobin), parts, run)
            for part in parts:
                if not part:
                    continue
                directory = Path(part)
                self.assertFalse(
                    (directory / "cargo").is_file() and os.access(directory / "cargo", os.X_OK),
                    part,
                )
                self.assertFalse(
                    (directory / "cargo.exe").is_file() and os.access(directory / "cargo.exe", os.X_OK),
                    part,
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
