import shutil
import subprocess
import sys
from pathlib import Path
import pytest

SPIKE_DIR = Path(__file__).resolve().parent.parent
if str(SPIKE_DIR) not in sys.path:
    sys.path.insert(0, str(SPIKE_DIR))

from ssa_lowering_verifier import SSAVerifier


def test_spec_exists():
    spec_path = SPIKE_DIR / "docs" / "SPEC_DIRECT_SSA_LOWERING.md"
    assert spec_path.exists(), "SPEC_DIRECT_SSA_LOWERING.md must exist"
    content = spec_path.read_text(encoding="utf-8")
    assert "Direct Go SSA/IR Lowering" in content
    assert "HYB-E3862-SSA-MALFORMED" in content
    assert "HYB-E3862-STACK-OVERFLOW" in content
    assert "StackCertificate" in content


def test_results_exists():
    results_path = SPIKE_DIR / "RESULTS.md"
    assert results_path.exists(), "RESULTS.md must exist"
    content = results_path.read_text(encoding="utf-8")
    assert "P1" in content
    assert "P2" in content
    assert "P3" in content
    assert "F1" in content
    assert "F2" in content


def test_fixtures_corpus():
    verifier = SSAVerifier()
    corpus_dir = SPIKE_DIR / "hy"
    summary = verifier.verify_corpus(corpus_dir)
    assert summary["passed"] is True, f"Corpus verification failed: {summary}"
    assert summary["fixtures_count"] == 3

    # Check 02 rejected with exact code
    res02 = next(r for r in summary["results"] if r["file"] == "02-ssa-malformed-phi-rejected.hy")
    assert res02["status"] == "REJECTED"
    assert res02["code"] == "HYB-E3862-SSA-MALFORMED"

    # Check 03 passed and has lexical shadow immunity
    res03 = next(r for r in summary["results"] if r["file"] == "03-lexical-shadow-immune.hy")
    assert res03["status"] == "PASS"
    assert res03.get("lexical_shadow_immune") is True


def test_go_ssa_work():
    if not shutil.which("go"):
        pytest.skip("go binary not found in PATH; skipping Go test suite (runs in GitHub Actions CI)")

    work_dir = SPIKE_DIR / "work"
    cmd = ["go", "test", "-v", "./..."]
    proc = subprocess.run(cmd, cwd=str(work_dir), capture_output=True, text=True)
    assert proc.returncode == 0, f"go test failed:\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
