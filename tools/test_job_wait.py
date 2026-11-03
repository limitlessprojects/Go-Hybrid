"""Offline regression tests for the overall Kaggle wait budget."""

import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import types
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location("job_wait", Path(__file__).with_name("job.py"))
job = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(job)


class Clock:
    def __init__(self):
        self.now = 0
        self.sleeps = []

    def read(self):
        return self.now

    def sleep(self, seconds):
        self.sleeps.append(seconds)
        self.now += seconds


class KaggleWaitDeadlineTests(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        # Keep both clocks available so the short-budget tests also reproduce
        # against the original wall-clock implementation.
        fake_time = types.SimpleNamespace(
            monotonic=self.clock.read, time=self.clock.read, sleep=self.clock.sleep
        )
        patcher = mock.patch.object(job, "time", fake_time)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.output = io.StringIO()
        redirect = contextlib.redirect_stdout(self.output)
        redirect.__enter__()
        self.addCleanup(redirect.__exit__, None, None, None)

    def test_short_budget_caps_poll_timeout_and_sleep(self):
        with mock.patch.object(job, "sh", return_value=(0, '"KernelWorkerStatus.RUNNING"')) as poll:
            self.assertEqual(job.kaggle_wait(0.01, "owner/kernel"), 2)
        self.assertAlmostEqual(poll.call_args.kwargs["timeout"], 0.6)
        self.assertEqual(poll.call_count, 1)
        self.assertEqual(self.clock.sleeps, [0.6])
        self.assertAlmostEqual(self.clock.now, 0.6)

    def test_hung_poll_consumes_remaining_budget_and_returns_timeout(self):
        def hung(command, timeout):
            self.clock.now += timeout
            raise subprocess.TimeoutExpired(command, timeout)

        with mock.patch.object(job, "sh", side_effect=hung):
            self.assertEqual(job.kaggle_wait(0.01, "owner/kernel"), 2)
        self.assertAlmostEqual(self.clock.now, 0.6)
        self.assertEqual(self.clock.sleeps, [])
        self.assertIn("TIMEOUT", self.output.getvalue())

    def test_transient_poll_timeout_can_recover_within_budget(self):
        replies = iter([None, "RUNNING", "COMPLETE"])

        def poll(command, timeout):
            state = next(replies)
            if state is None:
                self.clock.now += timeout
                raise subprocess.TimeoutExpired(command, timeout)
            return 0, '"KernelWorkerStatus.{}"'.format(state)

        with mock.patch.object(job, "sh", side_effect=poll) as status:
            self.assertEqual(job.kaggle_wait(2, "owner/kernel"), 0)
        self.assertEqual(status.call_count, 3)
        self.assertEqual(self.clock.now, 70)

    def test_stale_complete_after_timeout_does_not_count_as_new_run(self):
        def poll(command, timeout):
            if self.clock.now == 0:
                self.clock.now += timeout
                raise subprocess.TimeoutExpired(command, timeout)
            return 0, '"KernelWorkerStatus.COMPLETE"'

        with mock.patch.object(job, "sh", side_effect=poll):
            self.assertEqual(job.kaggle_wait(1.1, "owner/kernel"), 2)
        self.assertAlmostEqual(self.clock.now, 66)

    def test_wall_clock_changes_do_not_affect_wait_budget(self):
        with mock.patch.object(job.time, "time", side_effect=AssertionError("wall clock used")), \
             mock.patch.object(job, "sh", return_value=(1, "temporary status failure")):
            self.assertEqual(job.kaggle_wait(0.01, "owner/kernel"), 2)
        self.assertAlmostEqual(self.clock.now, 0.6)


if __name__ == "__main__":
    unittest.main()
