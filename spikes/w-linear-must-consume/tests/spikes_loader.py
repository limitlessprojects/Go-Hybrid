import importlib.util
import pathlib
import sys

root_dir = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location(
    "linear_obligation_verifier", root_dir / "linear_obligation_verifier.py"
)
mod = importlib.util.module_from_spec(spec)
sys.modules["linear_obligation_verifier"] = mod
spec.loader.exec_module(mod)
verify_hy_source = mod.verify_hy_source
