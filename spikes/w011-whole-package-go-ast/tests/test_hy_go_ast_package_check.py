#!/usr/bin/env python3
"""Unit tests for W-011 (#186): Whole-Package Tier-0 Checking via Native Go AST & Export Data."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from tools import hy_go_ast_package_check as pkg_check


class TestW011WholePackageGoAstCheck(unittest.TestCase):
    def test_01_sibling_go_value_struct_resolves_without_false_e3005(self):
        files = {
            "types.go": "package worker\ntype JobConfig struct {\nID int\nName string\n}\n",
            "handler.go": "//hy:check\npackage worker\nfunc run() {\ncfg := JobConfig{ID: 7}\nhy.Task(func() { use(cfg) })\nuse(cfg)\n}\n",
        }
        res = pkg_check.check_package_go_files(files)
        self.assertEqual(res.verdict, "ACCEPT")
        self.assertFalse(res.resolved_types["JobConfig"].is_mutable_ref)

    def test_02_sibling_go_mutable_struct_requires_hy_move_and_rejects_a1b_uam(self):
        types_src = "package worker\ntype SharedCache struct {\nItems map[string]int\n}\n"
        res_unmoved = pkg_check.check_package_go_files({
            "types.go": types_src,
            "handler.go": "//hy:check\npackage worker\nfunc run() {\nc := SharedCache{}\nhy.Task(func() { use(c) })\n}\n",
        })
        self.assertEqual(res_unmoved.verdict, "REJECT")
        self.assertTrue(any("HYB-E3002" in d for d in res_unmoved.diagnostics))

        res_uam = pkg_check.check_package_go_files({
            "types.go": types_src,
            "handler.go": "//hy:check\npackage worker\nfunc run() {\nc := SharedCache{}\nhy.Task(func() { use(hy.Move(&c)) })\nprintln(c)\n}\n",
        })
        self.assertEqual(res_uam.verdict, "REJECT")
        self.assertTrue(any("HYB-E3001" in d for d in res_uam.diagnostics))

    def test_03_a1c_loop_backedge_move_and_a2_a3_loan_window_rejected(self):
        res_loop = pkg_check.check_package_go_files({
            "loop.go": "//hy:check\npackage worker\nfunc run() {\nbuf := make([]byte, 64)\nfor i := range items {\nhy.Task(func() { send(hy.Move(&buf)) })\n}\n}\n",
        })
        self.assertEqual(res_loop.verdict, "REJECT")
        self.assertTrue(any("HYB-E3003" in d for d in res_loop.diagnostics))

        res_loan = pkg_check.check_package_go_files({
            "loan.go": "//hy:check\npackage worker\nfunc run() {\nm := make(map[string]int)\nr := &m[k]\nm[k] = 42\nprintln(r)\n}\n",
        })
        self.assertEqual(res_loan.verdict, "REJECT")
        self.assertTrue(any("HYB-E3004" in d for d in res_loan.diagnostics))

    def test_04_triage_q1_fail_closed_on_unknown_directive_or_missing_export_type(self):
        res_dir = pkg_check.check_package_go_files({
            "bad.go": "//hy:check\n//hy:unknown_pragma\npackage p\n",
        })
        self.assertEqual(res_dir.verdict, "REJECT")
        self.assertTrue(any("HYB-E3005" in d for d in res_dir.diagnostics))

        res_unk = pkg_check.check_package_go_files({
            "missing.go": "//hy:check\npackage p\nfunc f() {\nx := ForeignOpaque{}\nhy.Task(func() { use(x) })\n}\n",
        })
        self.assertEqual(res_unk.verdict, "REJECT")
        self.assertTrue(any("HYB-E3005" in d for d in res_unk.diagnostics))

    def test_05_triage_q2_q3_export_data_and_capability_wrappers_compose(self):
        res = pkg_check.check_package_go_files({
            "cap.go": "//hy:check\npackage worker\nfunc run() {\ntok := hy.Own[Buffer]{}\ncfg := ExtImmutableConfig{}\nhy.Task(func() { consume(hy.Move(&tok), cfg) })\n}\n",
        }, export_data={"ExtImmutableConfig": False})
        self.assertEqual(res.verdict, "ACCEPT")


if __name__ == "__main__":
    unittest.main()
