"""Offline tests: newly protected PRs must survive a queued sweep."""
import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('refresh_sweeper', Path(__file__).with_name('queue_sweeper.py'))
sweeper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sweeper)


class SweeperRefreshTests(unittest.TestCase):
    def sweep(self, latest, status=200, dry=False):
        listed = {'number': 1, 'state': 'open', 'labels': [],
                  'created_at': '2020-01-01T00:00:00Z', 'user': {'login': 'fixture'}, 'title': 'fixture'}
        calls = []
        def api(path, method='GET', body=None):
            calls.append((path, method, body))
            if '/pulls?' in path:
                return 200, [listed]
            if path.endswith('/pulls/1') and method == 'GET':
                return status, latest
            if method == 'PATCH':
                return 200, dict(listed, state='closed')
            if method == 'POST':
                return 201, {}
            return 200, {}
        with patch.object(sys, 'argv', ['queue_sweeper.py', *(['--dry'] if dry else [])]), \
                patch.object(sweeper, 'TOK', 'offline-fixture'), \
                patch.object(sweeper, 'api', side_effect=api), \
                patch.object(sweeper, 'config_protected_numbers', return_value=set()), \
                patch.object(sweeper.time, 'sleep'), contextlib.redirect_stdout(io.StringIO()):
            sweeper.main()
        return calls

    def fresh(self, **changes):
        return dict(number=1, state='open', labels=[], created_at='2020-01-01T00:00:00Z', **changes)

    def assert_no_writes(self, latest, status=200):
        calls = self.sweep(latest, status)
        self.assertFalse([c for c in calls if c[1] != 'GET'], calls)

    def test_every_new_protection_label_prevents_close(self):
        for label in sweeper.PROTECTED_LABELS:
            with self.subTest(label=label):
                latest = self.fresh()
                latest['labels'] = [{'name': label}]
                self.assert_no_writes(latest)

    def test_already_closed_pr_is_not_closed_or_commented_again(self):
        latest = self.fresh()
        latest['state'] = 'closed'
        self.assert_no_writes(latest)

    def test_unavailable_detail_is_not_permission_to_close(self):
        for status in (0, 403, 404, 429, 500):
            with self.subTest(status=status):
                self.assert_no_writes(None, status)

    def test_malformed_detail_is_not_permission_to_close(self):
        for latest in (None, [], {}, dict(self.fresh(), labels=None),
                       dict(self.fresh(), labels=[{}]), dict(self.fresh(), number=2)):
            with self.subTest(latest=latest):
                self.assert_no_writes(latest)

    def test_old_unprotected_pr_still_closes_with_reply(self):
        calls = self.sweep(self.fresh())
        self.assertEqual([c[1] for c in calls if c[1] != 'GET'], ['PATCH', 'POST'])
        detail = next(i for i, c in enumerate(calls) if c[0].endswith('/pulls/1') and c[1] == 'GET')
        close = next(i for i, c in enumerate(calls) if c[1] == 'PATCH')
        self.assertLess(detail, close)

    def test_dry_run_has_no_writes_or_per_pr_refresh(self):
        calls = self.sweep(self.fresh(), dry=True)
        self.assertTrue(all(c[1] == 'GET' for c in calls))
        self.assertFalse(any(c[0].endswith('/pulls/1') for c in calls))


if __name__ == '__main__':
    unittest.main()
