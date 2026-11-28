import unittest

from dominance import (Block, Graph, Instruction as I, InvalidSSA,
                       TWO_ITERATIONS, alias, execute, experiment, loop, verify)


class DominanceTests(unittest.TestCase):
    def assert_invalid(self, graph, message):
        with self.assertRaisesRegex(InvalidSSA, message):
            verify(graph)

    def test_duplicate_definitions(self):
        self.assert_invalid(Graph({"entry": Block((I("alloc", "a"),
                                                   I("alloc", "a")))}),
                            "duplicate definition")

    def test_use_before_definition(self):
        self.assert_invalid(Graph({"entry": Block((I("read", args=("a",)),
                                                   I("alloc", "a")))}),
                            "non-dominating use")

    def test_undefined_operand(self):
        self.assert_invalid(Graph({"entry": Block((I("read", args=("a",)),))}),
                            "undefined value")

    def test_branch_local_does_not_dominate_join(self):
        graph = Graph({
            "entry": Block((), ("left", "right")),
            "left": Block((I("alloc", "a"),), ("join",)),
            "right": Block((), ("join",)),
            "join": Block((I("read", args=("a",)),)),
        })
        self.assert_invalid(graph, "non-dominating use")

    def test_phi_operands_are_edge_uses(self):
        graph = Graph({
            "entry": Block((), ("left", "right")),
            "left": Block((I("alloc", "a"),), ("join",)),
            "right": Block((I("alloc", "b"),), ("join",)),
            "join": Block((I("phi", "p", incoming=(("left", "a"),
                                                       ("right", "b"))),
                           I("consume", args=("p",)))),
        })
        verify(graph)
        for arm in ("left", "right"):
            self.assertFalse(execute(graph, ("entry", arm, "join")).violations)

    def test_phi_rejects_operand_from_wrong_predecessor(self):
        graph = Graph({
            "entry": Block((), ("left", "right")),
            "left": Block((I("alloc", "a"),), ("join",)),
            "right": Block((I("alloc", "b"),), ("join",)),
            "join": Block((I("phi", "p", incoming=(("left", "b"),
                                                       ("right", "a"))),)),
        })
        self.assert_invalid(graph, "non-dominating phi edge")

    def test_phi_requires_exact_predecessor_set(self):
        graph = loop()
        graph.blocks["header"] = Block((I("phi", "map1", incoming=(
            ("entry", "map0"),)),), ("body", "exit"))
        self.assert_invalid(graph, "phi predecessor mismatch")

    def test_phi_cannot_appear_after_normal_instruction(self):
        graph = loop()
        graph.blocks["header"] = Block((I("read", args=("map0",)),
            I("phi", "map1", incoming=(("entry", "map0"),
                                       ("body", "map1")))), ("body", "exit"))
        self.assert_invalid(graph, "invalid phi placement")

    def test_malformed_cfg_and_instruction(self):
        for graph, message in (
            (Graph({}), "missing entry"),
            (Graph({"entry": Block((), ("missing",))}), "unknown successor"),
            (Graph({"entry": Block(), "dead": Block()}), "unreachable block"),
            (Graph({"entry": Block((I("magic", "a"),))}), "unknown opcode"),
            (Graph({"entry": Block((I("alloc"),))}), "invalid destination"),
        ):
            with self.subTest(message=message):
                self.assert_invalid(graph, message)

    def test_mutually_exclusive_consumptions_are_safe(self):
        graph = Graph({
            "entry": Block((I("alloc", "a"),), ("left", "right")),
            "left": Block((I("consume", args=("a",)),)),
            "right": Block((I("consume", args=("a",)),)),
        })
        for arm in ("left", "right"):
            self.assertEqual(execute(graph, ("entry", arm)).consumptions, 1)

    def test_loop_dominance_is_not_ownership(self):
        graph = loop()
        self.assertEqual(verify(graph)["body"], {"entry", "header", "body"})
        once = ("entry", "header", "body", "header", "exit")
        self.assertFalse(execute(graph, once).violations)
        twice = execute(graph, TWO_ITERATIONS)
        self.assertEqual(twice.consumptions, 1)
        self.assertRegex(twice.violations[0], "consume of consumed storage")
        fresh = execute(loop(True), TWO_ITERATIONS)
        self.assertFalse(fresh.violations)
        self.assertEqual((fresh.allocations, fresh.consumptions), (2, 2))

    def test_alias_and_clone_have_different_storage(self):
        shared = execute(alias(), ("entry",))
        self.assertRegex(shared.violations[0], "write of consumed storage")
        fresh = execute(alias(True), ("entry",))
        self.assertFalse(fresh.violations)
        self.assertEqual(fresh.allocations, 2)

    def test_phi_assignments_are_simultaneous(self):
        graph = Graph({
            "entry": Block((I("alloc", "a0"), I("alloc", "b0")), ("header",)),
            "header": Block((
                I("phi", "a1", incoming=(("entry", "a0"), ("body", "b1"))),
                I("phi", "b1", incoming=(("entry", "b0"), ("body", "a1"))),
            ), ("body", "exit")),
            "body": Block((), ("header",)),
            "exit": Block((I("consume", args=("a1",)),
                           I("consume", args=("b1",)))),
        })
        path = ("entry", "header", "body", "header", "exit")
        self.assertFalse(execute(graph, path).violations)
        self.assertEqual(execute(graph, path).consumptions, 2)

    def test_execution_rejects_invalid_paths(self):
        for path in ((), ("header",), ("entry", "exit"), ("entry", "header")):
            with self.subTest(path=path), self.assertRaises(ValueError):
                execute(loop(), path)

    def test_registered_experiment(self):
        results = experiment()
        self.assertTrue(all(case["ssa_valid"] for case in results.values()))
        self.assertEqual(sum(bool(case["violations"])
                             for case in results.values()), 2)


if __name__ == "__main__":
    unittest.main()
