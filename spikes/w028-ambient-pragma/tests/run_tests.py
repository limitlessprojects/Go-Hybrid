#!/usr/bin/env python3
"""Run all unit tests for W-028 ambient pragma verifier."""
import sys
import unittest
from pathlib import Path

# Add parent directory to path
spike_root = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(spike_root.parent))

if __name__ == "__main__":
    loader = unittest.TestLoader()
    suite = loader.discover(str(spike_root / "tests"), pattern="test_*.py")
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    sys.exit(0 if result.wasSuccessful() else 1)
