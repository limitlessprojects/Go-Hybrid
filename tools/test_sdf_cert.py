import copy, os, sys, unittest
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "sdf"))
from sdf_cert import certify, check, Reject

def net(init=0, effects=None, token="i32"):
    return {"actors": {"A": {"effects": effects or []}, "B": {}, "C": {}},
            "edges": [{"src": "A", "dst": "B", "prod": 2, "cons": 3, "init": 0, "token": token},
                      {"src": "B", "dst": "C", "prod": 1, "cons": 1, "init": 0, "token": token},
                      {"src": "C", "dst": "A", "prod": 3, "cons": 2, "init": init, "token": token}]}

class T(unittest.TestCase):
    def test_roundtrip(self):
        n = net(init=6)
        c = certify(n)
        self.assertEqual(c["repetitions"], {"A": 3, "B": 2, "C": 2})
        self.assertEqual(check(n, c), c["bytes"])
    def test_deadlock(self):
        with self.assertRaises(Reject): certify(net(init=0))
    def test_inconsistent(self):
        n = net(6); n["edges"][1]["prod"] = 2
        with self.assertRaises(Reject): certify(n)
    def test_effects_and_nonscalar(self):
        with self.assertRaises(Reject): certify(net(6, effects=["io"]))
        with self.assertRaises(Reject): certify(net(6, token="string"))
    def test_checker_rejects_forged(self):
        n = net(6); c = certify(n)
        for mut in (lambda x: x["schedule"].reverse(),
                    lambda x: x["schedule"].pop(),
                    lambda x: x["high_water"].__setitem__(0, 1),
                    lambda x: x["high_water"].__setitem__(1, x["high_water"][1] + 5),
                    lambda x: x.__setitem__("bytes", 1)):
            f = copy.deepcopy(c); mut(f)
            with self.assertRaises(Reject): check(n, f)

if __name__ == "__main__":
    unittest.main()
