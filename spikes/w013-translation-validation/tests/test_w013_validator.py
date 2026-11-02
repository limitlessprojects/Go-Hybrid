import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from validator import CheckedEffectIR, IREffectOp, build_certificate, evaluate_w013_corpus, validate_translation


class TestW013TranslationValidator(unittest.TestCase):
    def setUp(self):
        self.summaries = {"audit.Log": "io_effect", "ledger.Commit": "state_effect", "math.Abs": "pure"}
        self.ir = CheckedEffectIR(
            fn_name="settle_account",
            ops=(
                IREffectOp("EFFECT_CALL", "audit.Log", '"begin"'),
                IREffectOp("STORE_BACK", "accounts[id]", "__hy_loan_0"),
                IREffectOp("MOVE_SEND", "out_ch", "receipt"),
                IREffectOp("MATCH_RELEASE", "__hy_match_tmp", ""),
                IREffectOp("EFFECT_CALL", "ledger.Commit", "id"),
                IREffectOp("EXIT", "return:ok", ""),
            ),
            allow_managed=False,
            expected_exit="return:ok",
        )
        self.valid_go = (
            "func settle_account(id string) string {\n"
            "    __hy_loan_0 := accounts[id]\n"
            '    audit.Log("begin")\n'
            "    accounts[id] = __hy_loan_0\n"
            "    out_ch <- receipt\n"
            "    __hy_release(__hy_match_tmp)\n"
            "    ledger.Commit(id)\n"
            "    return ok\n"
            "}"
        )

    def test_valid_lowering_certified(self):
        cert = build_certificate(self.ir, self.valid_go, self.summaries)
        verdict = validate_translation(self.ir, self.valid_go, cert, self.summaries)
        self.assertTrue(verdict.certified)
        self.assertEqual(verdict.code, "CERTIFIED")

    def test_tampered_certificate_rejected(self):
        cert = build_certificate(self.ir, self.valid_go, self.summaries)
        verdict = validate_translation(self.ir, self.valid_go.replace("return ok", "return tampered"), cert, self.summaries)
        self.assertFalse(verdict.certified)
        self.assertEqual(verdict.code, "TV-E001_DIGEST_MISMATCH")

    def test_all_six_falsifier_mutations_rejected(self):
        report = evaluate_w013_corpus()
        self.assertTrue(report["positive_certified"])
        self.assertTrue(report["all_mutations_rejected"])
        mv = report["mutation_verdicts"]
        self.assertEqual(mv["M1_drop_store_back"], "TV-E003_MISSING_STORE_BACK")
        self.assertEqual(mv["M2_reorder_store_back_across_call"], "TV-E004_EFFECT_ORDER_VIOLATION")
        self.assertEqual(mv["M3_duplicate_effect"], "TV-E005_DUPLICATE_EFFECT")
        self.assertEqual(mv["M4_redirect_exit"], "TV-E006_EXIT_REDIRECT")
        self.assertEqual(mv["M5_unauthorized_managed"], "TV-E007_UNAUTHORIZED_MANAGED")
        self.assertEqual(mv["M6_unmodeled_panic"], "TV-E002_UNSUPPORTED_CONSTRUCT")
        self.assertEqual(mv["M7_early_return_dead_effect"], "TV-E003_MISSING_STORE_BACK")
        self.assertEqual(mv["M8_wrong_target_function"], "TV-E002_UNSUPPORTED_CONSTRUCT")
        self.assertLess(report["tcb_ratio"], 0.30)

    def test_early_return_and_wrong_target_counterexamples(self):
        summaries = {"A": "io_effect"}
        ir = CheckedEffectIR("f", (IREffectOp("EFFECT_CALL", "A"), IREffectOp("EXIT", "return:void")), expected_exit="return:void")
        cases = {
            "positive": ("package fixture\nfunc A() {}\nfunc f(b bool) {\n A()\n return\n}\n", True, "CERTIFIED"),
            "early_return": ("package fixture\nfunc A() {}\nfunc f(b bool) {\n return\n A()\n return\n}\n", False, "TV-E004_EFFECT_ORDER_VIOLATION"),
            "wrong_target": ("package fixture\nfunc A() {}\nfunc g(b bool) {\n A()\n return\n}\n", False, "TV-E002_UNSUPPORTED_CONSTRUCT"),
        }
        for name, (source, want_certified, want_code) in cases.items():
            with self.subTest(name=name):
                verdict = validate_translation(ir, source, build_certificate(ir, source, summaries), summaries)
                self.assertEqual(verdict.certified, want_certified)
                self.assertEqual(verdict.code, want_code)


if __name__ == "__main__":
    unittest.main()
