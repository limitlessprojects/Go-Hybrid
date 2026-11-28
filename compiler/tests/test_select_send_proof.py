import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SOURCE = """\
package main

fn worker() {
}

fn main() {
    ch := make(chan int, 4)
    task worker()
    ch <- 10
    select {
        ch <- 20: {
            ch <- 30
        }
        <-ch: {}
        ch <- 40: {}
        default: {}
    }
}
"""

SEND_HEADERS = ["ch <- 10", "ch <- 20", "ch <- 30", "ch <- 40"]


class SelectSendProofTest(unittest.TestCase):
    def build_proof(self, source):
        hyb = os.environ.get("HYB", "hyb")
        if os.sep in hyb:
            hyb = str(Path(hyb).resolve())

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "main.hy").write_text(source, encoding="utf-8")
            subprocess.run(
                [hyb, "build", "--emit-proof", "main.hy"],
                cwd=root,
                check=True,
            )
            proofs = list(root.rglob("proof.json"))
            self.assertEqual(len(proofs), 1, proofs)
            return json.loads(proofs[0].read_text(encoding="utf-8"))

    def test_counts_order_and_source_spans(self):
        proof = self.build_proof(SOURCE)
        self.assertEqual(proof["task_sites"], 1)
        self.assertEqual(proof["send_sites"], 4)

        task, *sends = proof["witnesses"]
        self.assertEqual(task["site"], "task")
        self.assertNotIn("status", task)
        self.assertEqual([w["site"] for w in sends], ["send"] * 4)
        self.assertEqual([w["status"] for w in sends], ["proven"] * 4)

        # The fixture is ASCII, so string indexes are also byte offsets.
        for witness, header in zip(sends, SEND_HEADERS):
            with self.subTest(header=header):
                start = SOURCE.index(header)
                self.assertEqual(witness["span"]["start"], start)
                self.assertEqual(witness["span"]["end"], start + len(header))

    def test_each_select_header_hash_depends_on_its_sent_value(self):
        original = self.build_proof(SOURCE)["witnesses"]

        # Equal-width edits keep every span fixed. Each edit must change
        # exactly the hash of the corresponding select send, not the hash
        # of the standalone send in its body or of a neighboring arm.
        for header, replacement, witness_index in [
            ("ch <- 20:", "ch <- 21:", 2),
            ("ch <- 40:", "ch <- 41:", 4),
        ]:
            with self.subTest(header=header):
                changed = self.build_proof(
                    SOURCE.replace(header, replacement, 1)
                )["witnesses"]
                self.assertEqual(len(changed), len(original))
                for index, (before, after) in enumerate(zip(original, changed)):
                    if index == witness_index:
                        self.assertEqual(after["span"], before["span"])
                        self.assertNotEqual(after["node_hash"], before["node_hash"])
                    else:
                        self.assertEqual(after, before)


if __name__ == "__main__":
    unittest.main()
