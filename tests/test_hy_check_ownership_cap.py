import unittest
from tools.hy_check_ownership_cap import check_ownership

class TestOwnershipCap(unittest.TestCase):
    def test_simple_assignment(self):
        code = "x := 1"
        self.assertTrue(check_ownership(code))

    def test_complex_assignment(self):
        code = "x := []byte{}"
        self.assertTrue(check_ownership(code))

    def test_function_call(self):
        code = "x := some_func()"
        self.assertTrue(check_ownership(code))

    def test_closure_usage(self):
        code = "x := func() {}"
        self.assertTrue(check_ownership(code))

    def test_invalid_usage(self):
        code = "x := y"
        self.assertFalse(check_ownership(code))

if __name__ == '__main__':
    unittest.main()