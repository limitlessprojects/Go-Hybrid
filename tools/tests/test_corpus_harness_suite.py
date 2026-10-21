"""Regression test suite entry under tests/ for corpus_harness (#118)."""

import os
import sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from tools.test_corpus_harness import *  # noqa: E402,F401,F403
import unittest  # noqa: E402


if __name__ == "__main__":
    unittest.main()
