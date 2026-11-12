#!/usr/bin/env python3
"""
test_ssa_compiler.py — Unit test suite for W-3888 Direct Go SSA Lowering Spike.
"""

import unittest
import sys
import os

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from ssa_compiler import SSALoweringEngine, SSAFunction, SSAInstruction

class TestSSALowering(unittest.TestCase):
    def setUp(self):
        self.engine = SSALoweringEngine()

    def test_pipeline_lowering_topology(self):
        fn = self.engine.lower_pipeline("test_pipeline", has_task_spawn=True)
        self.assertEqual(fn.name, "test_pipeline")
        self.assertEqual(len(fn.blocks), 4)
        
        block_labels = [b.label for b in fn.blocks]
        self.assertEqual(block_labels, ["entry", "loop.head", "loop.body", "exit"])

    def test_h3_zero_escape_annotation(self):
        fn = self.engine.lower_pipeline("test_esc_none", has_task_spawn=True)
        entry_block = fn.blocks[0]
        alloc_instrs = [i for i in entry_block.instructions if i.op == "Alloc"]
        self.assertTrue(len(alloc_instrs) > 0)
        self.assertIn("hy:esc_none", alloc_instrs[0].comment)

    def test_task_spawn_concurrency_op(self):
        fn = self.engine.lower_pipeline("test_task", has_task_spawn=True)
        entry_block = fn.blocks[0]
        go_instrs = [i for i in entry_block.instructions if i.op == "Go"]
        self.assertEqual(len(go_instrs), 1)
        self.assertEqual(go_instrs[0].args[0], "fn_ref3")

    def test_channel_ops_present(self):
        fn = self.engine.lower_pipeline("test_chan", has_task_spawn=True)
        entry_block = fn.blocks[0]
        body_block = fn.blocks[2]
        exit_block = fn.blocks[3]
        
        self.assertTrue(any(i.op == "MakeChan" for i in entry_block.instructions))
        self.assertTrue(any(i.op == "Send" for i in body_block.instructions))
        self.assertTrue(any(i.op == "Close" for i in exit_block.instructions))

if __name__ == "__main__":
    unittest.main()
