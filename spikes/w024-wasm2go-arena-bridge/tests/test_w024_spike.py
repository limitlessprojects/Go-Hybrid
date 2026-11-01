import pathlib
import subprocess

SPIKE_DIR = pathlib.Path(__file__).resolve().parent.parent
WORK_DIR = SPIKE_DIR / "work"
HY_DIR = SPIKE_DIR / "hy"


def test_spike_layout_and_results():
    results = (SPIKE_DIR / "RESULTS.md").read_text(encoding="utf-8")
    assert "W-024" in results or "757" in results
    assert (HY_DIR / "01-zero-cgo-arena-bridge.hy").is_file()
    assert (HY_DIR / "02-view-escape-rejected.hy").is_file()
    assert (WORK_DIR / "arena.go").is_file()
    assert (WORK_DIR / "arena_test.go").is_file()


def test_zero_unsafe_or_cgo_imports_in_arena_bridge():
    src = (WORK_DIR / "arena.go").read_text(encoding="utf-8")
    assert '"unsafe"' not in src
    assert 'import "C"' not in src


def test_hy_fixtures_cover_admitted_and_rejected_cases():
    pos = (HY_DIR / "01-zero-cgo-arena-bridge.hy").read_text(encoding="utf-8")
    neg = (HY_DIR / "02-view-escape-rejected.hy").read_text(encoding="utf-8")
    assert "fn main()" in pos
    assert "HYB-E" in neg or "borrow" in neg.lower() or "escape" in neg.lower()


def test_go_arena_bridge_suite_passes():
    res = subprocess.run(
        ["go", "test", "./..."],
        cwd=str(WORK_DIR),
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert res.returncode == 0, f"go test failed:\n{res.stdout}\n{res.stderr}"