"""Offline regressions for redundant label-palette creation requests."""
import argparse
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import triage_bot as triage


class PaletteDeltaTests(unittest.TestCase):
    def apply(self, existing, dry=False):
        palette = {'triage:pareto': {'color': '111111', 'description': 'keep'},
                   'triage:stale-base': {'color': '222222', 'description': 'refresh'}}
        cfg = {'repo': 'fixture/repo', 'labels': palette}
        client = Mock(repo='fixture/repo', calls=0, remaining=1000)
        client.paginate.return_value = [{'name': name} for name in existing]
        with tempfile.TemporaryDirectory() as directory:
            scan = Path(directory) / 'scan.json'
            scan.write_text(json.dumps({'items': [{'number': 7, 'labels': ['triage:pareto'],
                                                  'current_labels': ['triage:pareto']}]}), encoding='utf-8')
            with patch.object(triage, 'load_cfg', return_value=cfg), \
                    patch.object(triage, 'load_token', return_value='offline-fixture'), \
                    patch.object(triage, 'Gh', return_value=client), \
                    contextlib.redirect_stdout(io.StringIO()):
                triage.cmd_labels(argparse.Namespace(dry=dry, from_scan=str(scan)))
        return client

    def test_existing_palette_and_current_pr_need_no_writes(self):
        client = self.apply(['triage:pareto', 'triage:stale-base'])
        client.post.assert_not_called()
        client.paginate.assert_called_once()

    def test_only_missing_palette_entry_is_created(self):
        client = self.apply(['triage:pareto'])
        self.assertEqual(client.post.call_count, 1)
        self.assertEqual(client.post.call_args.args[1]['name'], 'triage:stale-base')

    def test_empty_palette_creates_all_requested_entries(self):
        client = self.apply([])
        self.assertEqual(client.post.call_count, 2)
        self.assertTrue(all(call.args[0].endswith('/labels') for call in client.post.call_args_list))

    def test_unrelated_existing_labels_are_preserved(self):
        client = self.apply(['triage:pareto', 'triage:stale-base', 'maintainer:keep'])
        client.post.assert_not_called()
        client.patch.assert_not_called()

    def test_dry_run_neither_lists_palette_nor_writes_it(self):
        client = self.apply([], dry=True)
        client.post.assert_not_called()
        client.paginate.assert_not_called()


if __name__ == '__main__':
    unittest.main()
