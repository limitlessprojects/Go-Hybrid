import copy
import unittest
from planner import Actor, Edge, MASK, emit_go, plan, reference, verify


def fixtures():
    return [
        ([Actor('source', 'source'), Actor('scale', 'scale', 3), Actor('sink', 'sink')],
         [Edge('source', 'scale'), Edge('scale', 'sink')]),
        ([Actor('p', 'source'), Actor('join', 'sum'), Actor('sink', 'sink')],
         [Edge('p', 'join', 2, 3), Edge('join', 'sink')]),
        ([Actor('left', 'source'), Actor('right', 'source'), Actor('join', 'sum'), Actor('sink', 'sink')],
         [Edge('left', 'join'), Edge('right', 'join'), Edge('join', 'sink')]),
        ([Actor('double', 'scale', 2), Actor('sum', 'sum')],
         [Edge('double', 'sum'), Edge('sum', 'double', initial=(1,))]),
    ]


class PlannerTests(unittest.TestCase):
    def test_pipeline_hand_computed(self):
        a, e = fixtures()[0]
        c = plan(a, e)
        self.assertEqual(c, {'schedule': ['source', 'scale', 'sink'], 'capacities': [1, 1]})
        self.assertEqual(reference(a, e, c, 50), (50, 3825, [[], []]))

    def test_multirate_hand_computed(self):
        a, e = fixtures()[1]
        c = plan(a, e)
        self.assertEqual(verify(a, e, c)['firings'], {'p': 3, 'join': 2, 'sink': 2})
        self.assertEqual(c['capacities'], [6, 2])
        self.assertEqual(reference(a, e, c, 1), (3, 12, [[], []]))
        self.assertEqual(reference(a, e, c, 50)[1], 22650)

    def test_join_hand_computed(self):
        a, e = fixtures()[2]
        self.assertEqual(reference(a, e, plan(a, e), 2), (4, 10, [[], [], []]))

    def test_feedback_wraps_as_uint64(self):
        a, e = fixtures()[3]
        self.assertEqual(reference(a, e, plan(a, e), 50), (0, 0, [[], [1 << 50]]))
        self.assertEqual(reference(a, e, plan(a, e), 64), (0, 0, [[], [0]]))

    def test_reject_uninitialized_cycle(self):
        a, e = fixtures()[3]
        with self.assertRaisesRegex(ValueError, 'no enabled'):
            plan(a, [Edge('double', 'sum'), Edge('sum', 'double')])

    def test_reject_inconsistent_rates(self):
        a, _ = fixtures()[3]
        with self.assertRaisesRegex(ValueError, 'inconsistent'):
            plan(a, [Edge('double', 'sum', 2, 1), Edge('sum', 'double', initial=(1,))])

    def test_reject_invalid_rates(self):
        a, _ = fixtures()[0]
        for rate in (0, -1, 1.5, True, 4097):
            with self.subTest(rate=rate), self.assertRaises(ValueError):
                plan(a, [Edge('source', 'scale', rate), Edge('scale', 'sink')])

    def test_reject_unknown_effect(self):
        with self.assertRaisesRegex(ValueError, 'unmodeled'):
            plan([Actor('x', 'foreign')], [])

    def test_reject_forged_schedule(self):
        a, e = fixtures()[0]
        c = plan(a, e)
        c['schedule'] = ['sink', 'scale', 'source']
        with self.assertRaisesRegex(ValueError, 'underflow'):
            verify(a, e, c)

    def test_reject_incomplete_period(self):
        a, e = fixtures()[0]
        c = plan(a, e)
        c['schedule'].pop()
        with self.assertRaisesRegex(ValueError, 'complete periodic'):
            verify(a, e, c)

    def test_reject_buffer_and_budget_lies(self):
        a, e = fixtures()[1]
        c = plan(a, e)
        with self.assertRaisesRegex(ValueError, 'budget'):
            verify(a, e, c, budget_bytes=63)
        self.assertEqual(verify(a, e, c, budget_bytes=64)['token_payload_bytes'], 64)
        for value in (1, 7):
            forged = copy.deepcopy(c)
            forged['capacities'][0] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                verify(a, e, forged)

    def test_reject_initial_overflow(self):
        a, e = fixtures()[3]
        e[1] = Edge('sum', 'double', initial=(1, 2))
        c = plan(a, e)
        c['capacities'][1] = 1
        with self.assertRaisesRegex(ValueError, 'initial overflow'):
            verify(a, e, c)

    def test_reject_invalid_tokens_and_endpoints(self):
        a, e = fixtures()[3]
        for bad in (-1, MASK + 1, '0'):
            with self.subTest(token=bad), self.assertRaises(ValueError):
                plan(a, [e[0], Edge('sum', 'double', initial=(bad,))])
        with self.assertRaisesRegex(ValueError, 'unknown endpoint'):
            plan(a, [Edge('missing', 'double')])

    def test_actor_labels_not_emitted(self):
        a = [Actor('source; panic("injected")', 'source'), Actor('sink', 'sink')]
        e = [Edge(a[0].name, 'sink')]
        self.assertNotIn('injected', emit_go(a, e, plan(a, e), 'Safe'))

    def test_every_fixture_is_periodic_after_many_periods(self):
        for a, e in fixtures():
            c = plan(a, e)
            before = [len(x.initial) for x in e]
            self.assertEqual([len(q) for q in reference(a, e, c, 1000)[2]], before)


if __name__ == '__main__':
    unittest.main()
