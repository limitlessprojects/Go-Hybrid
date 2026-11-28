import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import chunk_transformer as ct


class TestExtraction(unittest.TestCase):
    def test_admits_the_narrow_affine_form(self):
        result = ct.extract_affine_loop(ct.ADMITTED_HY)
        self.assertTrue(result.admitted)
        self.assertEqual(result.spec.width_bits, 8)
        self.assertEqual(result.spec.a_expr, "(3+input[i])")
        self.assertEqual(result.spec.b_expr, "input[i]")

    def test_refuses_state_dependent_coefficient(self):
        result = ct.extract_affine_loop(ct.STATE_DEPENDENT_HY)
        self.assertFalse(result.admitted)
        self.assertIn("state", result.reason)

    def test_refuses_effectful_call_in_coefficient(self):
        result = ct.extract_affine_loop(ct.EFFECTFUL_CALL_HY)
        self.assertFalse(result.admitted)
        self.assertIn("function", result.reason)

    def test_refuses_unrecognized_loop_shape(self):
        result = ct.extract_affine_loop(ct.UNRECOGNIZED_SHAPE_HY)
        self.assertFalse(result.admitted)
        self.assertIsNone(result.spec)

    def test_refuses_non_loop_source_without_crashing(self):
        result = ct.extract_affine_loop("fn main() {}\n")
        self.assertFalse(result.admitted)
        self.assertIsNone(result.spec)

    def test_admits_every_word_width(self):
        for width in ct.WORD_BITS:
            src = ct.ADMITTED_HY.replace("uint8", width)
            result = ct.extract_affine_loop(src)
            self.assertTrue(result.admitted, width)
            self.assertEqual(result.spec.width_bits, ct.WORD_BITS[width])


class TestComposeAlgebra(unittest.TestCase):
    def test_compose_is_associative_not_commutative(self):
        modulus = 256
        rng_triples = [((2, 3), (5, 7), (11, 13)), ((9, 0), (253, 199), (2, 128))]
        for left, mid, right in rng_triples:
            grouped_left = ct.compose(ct.compose(left, mid, modulus), right, modulus)
            grouped_right = ct.compose(left, ct.compose(mid, right, modulus), modulus)
            self.assertEqual(grouped_left, grouped_right)
            if left != mid:
                self.assertNotEqual(ct.compose(left, mid, modulus), ct.compose(mid, left, modulus))

    def test_identity_is_neutral(self):
        modulus = 256
        summary = (7, 11)
        self.assertEqual(ct.compose(ct.IDENTITY, summary, modulus), summary)
        self.assertEqual(ct.compose(summary, ct.IDENTITY, modulus), summary)

    def test_chunk_summary_matches_serial_over_the_chunk(self):
        modulus = 256
        a_fn, b_fn = (lambda d: (3 + d) % modulus), (lambda d: d % modulus)
        digits = [1, 2, 3, 4, 5]
        for state0 in range(0, modulus, 17):
            expected = ct.serial_reference(digits, state0, a_fn, b_fn, modulus)
            summary = ct.summarize_chunk(digits, a_fn, b_fn, modulus)
            self.assertEqual(ct.apply_summary(summary, state0, modulus), expected)


class TestCostGate(unittest.TestCase):
    def test_small_inputs_fall_back_to_a_single_chunk(self):
        self.assertEqual(ct.choose_chunk_count(10, 8), 1)
        self.assertEqual(ct.choose_chunk_count(0, 8), 1)

    def test_requesting_one_chunk_is_always_serial(self):
        self.assertEqual(ct.choose_chunk_count(10_000, 1), 1)

    def test_large_inputs_honor_the_requested_chunk_count(self):
        self.assertEqual(ct.choose_chunk_count(1000, 4), 4)

    def test_split_contiguous_covers_every_digit_in_order(self):
        digits = list(range(200))
        chunks = ct.split_contiguous(digits, 5)
        self.assertEqual([d for chunk in chunks for d in chunk], digits)


class TestOrderedVsCompletionOrderCollector(unittest.TestCase):
    def test_ordered_collector_matches_serial_reference(self):
        modulus = 256
        a_fn, b_fn = (lambda d: (3 + d) % modulus), (lambda d: d % modulus)
        digits = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0]
        for state0 in (0, 1, 128, 255):
            expected = ct.serial_reference(digits, state0, a_fn, b_fn, modulus)
            got = ct.compile_chunked(digits, state0, a_fn, b_fn, modulus, requested_chunks=4)
            self.assertEqual(got, expected)

    def test_randomized_ordered_collector_always_matches(self):
        report = ct.run_randomized_verification(seed=1, trials=200)
        self.assertTrue(report["ordered_all_match"], report)

    def test_completion_order_mutant_diverges_on_most_multi_chunk_cases(self):
        report = ct.run_randomized_verification(seed=1, trials=200)
        self.assertGreater(report["multi_chunk_trials"], 0)
        self.assertGreater(report["multi_chunk_mutant_mismatches"], report["multi_chunk_trials"] // 2, report)

    def test_concrete_reversed_order_mismatch_witness(self):
        modulus = 256
        a_fn, b_fn = (lambda d: (3 + d) % modulus), (lambda d: d % modulus)
        digits = [(7 * i * i + 3 * i + 1) % 256 for i in range(180)]
        ordered = ct.compile_chunked(digits, 0, a_fn, b_fn, modulus, requested_chunks=4, mutate_collector=False)
        mutant = ct.compile_chunked(digits, 0, a_fn, b_fn, modulus, requested_chunks=4, mutate_collector=True)
        expected = ct.serial_reference(digits, 0, a_fn, b_fn, modulus)
        self.assertEqual(ordered, expected)
        self.assertNotEqual(mutant, expected)


class TestQuadraticAdmissionHazard(unittest.TestCase):
    def test_two_point_probe_cannot_predict_a_state_dependent_recurrence(self):
        for modulus in (256, 65536, 2 ** 32, 2 ** 64, 1009):
            predicted, actual = ct.quadratic_admission_hazard(modulus)
            self.assertNotEqual(predicted, actual, modulus)

    def test_extractor_already_refuses_the_quadratic_source_form(self):
        quadratic_hy = """
        fn broken(input []uint8, s0 uint8) uint8 {
            s := s0
            for i in 0..len(input) {
                s = s*s + input[i]
            }
            return s
        }
        """
        result = ct.extract_affine_loop(quadratic_hy)
        self.assertFalse(result.admitted)
        self.assertIn("state", result.reason)


class TestLiveGo(unittest.TestCase):
    def test_generated_go_matches_python_reference_and_mutant_diverges(self):
        report = ct.check_all(live_go=True)
        live = report["P5_live_go"]
        if live.get("skipped"):
            self.skipTest(live.get("reason", "go not available"))
        self.assertTrue(live["build_ok"], live)
        self.assertTrue(live["run_ok"], live)
        self.assertTrue(live["ordered_all_match"], live)
        self.assertGreater(live["mutant_mismatches"], 0, live)


class TestCheckAll(unittest.TestCase):
    def test_check_all_without_live_go(self):
        report = ct.check_all(live_go=False)
        self.assertTrue(report["verified"], report)

    def test_main_check_exits_zero(self):
        self.assertEqual(ct.main(["--check"]), 0)


if __name__ == "__main__":
    unittest.main()
