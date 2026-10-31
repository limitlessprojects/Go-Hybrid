"""Offline approval tests: a commit can have several independent workflows."""
import argparse
import contextlib
import io
import unittest
from unittest.mock import Mock, patch

import triage_bot as triage


class WorkflowRunIdentityTests(unittest.TestCase):
    def approve(self, runs, maximum=20, dry=False):
        client = Mock(calls=0, remaining=1000)
        client.paginate.side_effect = [
            [{'number': 7, 'head': {'sha': 'current'}}], runs,
        ]
        client.req.return_value = (201, None)
        output = io.StringIO()
        args = argparse.Namespace(max=maximum, dry=dry)
        with patch.object(triage, 'load_cfg', return_value={'repo': 'fixture/repo'}), \
                patch.object(triage, 'load_token', return_value='offline-fixture'), \
                patch.object(triage, 'Gh', return_value=client), \
                contextlib.redirect_stdout(output):
            triage.cmd_approve(args)
        return client, output.getvalue()

    def run_record(self, number, name='ci', sha='current'):
        return {'id': number, 'head_sha': sha, 'name': name}

    def test_every_workflow_for_current_head_is_approved(self):
        client, output = self.approve([self.run_record(1, 'ci'),
                                      self.run_record(2, 'job-control'),
                                      self.run_record(3, 'preflight')])
        self.assertEqual(client.req.call_count, 3)
        self.assertIn('approved 3/3', output)

    def test_duplicate_run_records_are_approved_once(self):
        record = self.run_record(1)
        client, _ = self.approve([record, record])
        self.assertEqual(client.req.call_count, 1)

    def test_old_heads_and_unrelated_runs_are_not_approved(self):
        client, _ = self.approve([self.run_record(1, sha='old'), self.run_record(2)])
        self.assertEqual(client.req.call_count, 1)
        self.assertIn('/runs/2/approve', client.req.call_args.args[1])

    def test_quota_counts_independent_runs_on_same_head(self):
        client, output = self.approve([self.run_record(i) for i in range(1, 5)], maximum=2)
        self.assertEqual(client.req.call_count, 2)
        self.assertIn('approved 2/4', output)

    def test_dry_run_reports_every_candidate_without_writing(self):
        client, output = self.approve([self.run_record(1), self.run_record(2)], dry=True)
        client.req.assert_not_called()
        self.assertEqual(output.count('DRY would approve'), 2)


if __name__ == '__main__':
    unittest.main()
