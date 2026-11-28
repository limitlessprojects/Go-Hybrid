import importlib.util
import subprocess
import sys
from pathlib import Path


def load_verifier():
    script = Path(__file__).resolve().parent.parent / "escape_freeze_verifier.py"
    spec = importlib.util.spec_from_file_location("escape_freeze_verifier", script)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["escape_freeze_verifier"] = mod
    spec.loader.exec_module(mod)
    return mod


def test_all_12_conformance_cases_pass():
    mod = load_verifier()
    results = mod.run_corpus()
    assert len(results) == 12
    for r in results:
        assert r.passed, f"Case {r.case_id} failed: expected {r.expected_verdict} ({r.expected_code}), got {r.actual_verdict} ({r.diagnostics})"


def test_post_escape_write_on_alias_peer_rejected_with_hyb_e3014():
    mod = load_verifier()
    ck = mod.EscapeFreezeChecker()
    diags, frozen = ck.check_trace([
        mod.StatementEvent("decl", 1, binding="m", may_share=True),
        mod.StatementEvent("alias", 2, binding="alias_m", source="m"),
        mod.StatementEvent("foreign_call", 3, callee="http.HandleFunc", closure_reads=("m",)),
        mod.StatementEvent("write", 4, binding="alias_m"),
    ])
    assert "m" in frozen and "alias_m" in frozen
    assert len(diags) == 1
    assert diags[0][0] == "HYB-E3014"
    assert diags[0][1] == 4


def test_pre_escape_writes_followed_by_read_only_escape_accepted():
    mod = load_verifier()
    ck = mod.EscapeFreezeChecker()
    diags, frozen = ck.check_trace([
        mod.StatementEvent("decl", 1, binding="table", may_share=True),
        mod.StatementEvent("write", 2, binding="table"),
        mod.StatementEvent("write", 3, binding="table"),
        mod.StatementEvent("foreign_call", 4, callee="http.HandleFunc", closure_reads=("table",)),
    ])
    assert "table" in frozen
    assert diags == []


def test_cli_check_exits_zero():
    script = Path(__file__).resolve().parent.parent / "escape_freeze_verifier.py"
    proc = subprocess.run([sys.executable, str(script), "--check"], capture_output=True, text=True, check=False)
    assert proc.returncode == 0
    assert "12/12 conformance cases passed" in proc.stdout
