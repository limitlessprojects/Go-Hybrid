import pathlib
import subprocess

SPIKE_DIR = pathlib.Path(__file__).resolve().parent.parent
WORK_DIR = SPIKE_DIR / "work"
HY_DIR = SPIKE_DIR / "hy"


def test_spike_layout_and_results():
    results = (SPIKE_DIR / "RESULTS.md").read_text(encoding="utf-8")
    assert "605" in results or "Replay" in results
    assert (HY_DIR / "01-replay-island-admitted.hy").is_file()
    assert (HY_DIR / "02-replay-island-unmediated-rejected.hy").is_file()
    assert (WORK_DIR / "island.go").is_file()
    assert (WORK_DIR / "island_test.go").is_file()


def test_hy_fixtures_admission_and_rejection():
    pos = (HY_DIR / "01-replay-island-admitted.hy").read_text(encoding="utf-8")
    neg = (HY_DIR / "02-replay-island-unmediated-rejected.hy").read_text(encoding="utf-8")
    assert "fn main()" in pos
    assert "HYB-E36" in neg or "replay_island" in neg


def test_replay_trace_binary_header_and_tamper_codes():
    src = (WORK_DIR / "island.go").read_text(encoding="utf-8")
    for code in ("HYB-E3601", "HYB-E3605", "HYB-E3606", "HYB-E3607", "HYB-E3608"):
        assert code in src


def test_go_replay_island_suite_passes():
    res = subprocess.run(
        ["go", "test", "./..."],
        cwd=str(WORK_DIR),
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert res.returncode == 0, f"go test failed:\n{res.stdout}\n{res.stderr}"