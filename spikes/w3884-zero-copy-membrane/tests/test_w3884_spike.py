import pathlib
import shutil
import subprocess
import sys
import pytest

SPIKE_DIR = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SPIKE_DIR))
import membrane_verifier as v

WORK_DIR = SPIKE_DIR / "work"
HY_DIR = SPIKE_DIR / "hy"


def test_spike_layout_and_artifacts():
    results = (SPIKE_DIR / "RESULTS.md").read_text(encoding="utf-8")
    assert "W-3884" in results or "3884" in results
    assert (HY_DIR / "01-zero-copy-membrane-admitted.hy").is_file()
    assert (HY_DIR / "02-region-escape-rejected.hy").is_file()
    assert (HY_DIR / "03-double-consume-rejected.hy").is_file()
    assert (WORK_DIR / "membrane.go").is_file()
    assert (WORK_DIR / "membrane_test.go").is_file()
    assert (WORK_DIR / "bench_test.go").is_file()
    assert (WORK_DIR / "escape_test.go").is_file()


def test_zero_cgo_or_unsafe_imports():
    src = (WORK_DIR / "membrane.go").read_text(encoding="utf-8")
    assert '"unsafe"' not in src
    assert 'import "C"' not in src
    assert '"reflect"' not in src


def test_pure_python_gates_and_hypotheses():
    p1, p2, p3, f_ok, fix_ok, metrics = v.verify_pure_python_gates()
    assert p1, "P1 failed: CGO or unsafe imports detected"
    assert p2, "P2 failed: missing phantom RegionCap or TransferBuffer definitions"
    assert p3, "P3 failed: missing 100MB 10,000 transition benchmark"
    assert f_ok, "Falsifiers failed: missing F1/F2 error codes or test hooks"
    assert fix_ok, "Fixtures failed: expected at least 2 .hy fixtures"


def test_hy_fixtures_syntax_and_error_codes():
    ok, msg = v.validate_hy_fixtures()
    assert ok, f"Fixture validation failed: {msg}"


def test_live_go_suite():
    if not shutil.which("go"):
        pytest.skip("go toolchain not present on PATH; skipped in local environment")

    res = subprocess.run(
        ["go", "test", "-v", "./..."],
        cwd=str(WORK_DIR),
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert res.returncode == 0, f"go test failed:\n{res.stdout}\n{res.stderr}"
