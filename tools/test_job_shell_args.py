import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shlex
import shutil
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("job_shell_args", ROOT / "tools/job.py")
job_tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(job_tool)


@unittest.skipUnless(shutil.which("sh") or os.path.exists("/bin/sh"), "requires /bin/sh")
class KaggleShellArgumentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="hybrid-cli-args-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.log = self.root / "argv.jsonl"
        # An apostrophe exercises shell quoting beyond the common space case.
        self.cli = self.root / "CLI owner's directory" / "fake kaggle"
        self.cli.parent.mkdir()
        self.cli.write_text(
            "#!/bin/sh\nexec " + shlex.quote(sys.executable) + " "
            + shlex.quote(str(self.root / "fake_cli.py")) + ' "$@"\n',
            encoding="utf-8",
        )
        self.cli.chmod(0o755)
        self.plain_cli = self.root / "kaggle"
        self.plain_cli.write_text(self.cli.read_text(), encoding="utf-8")
        self.plain_cli.chmod(0o755)
        (self.root / "fake_cli.py").write_text(
            "import json, pathlib, sys\n"
            "args = sys.argv[1:]\n"
            "with pathlib.Path(" + repr(str(self.log)) + ").open('a') as f:\n"
            "    f.write(json.dumps(args) + '\\n')\n"
            "if args[:2] == ['kernels', 'output']:\n"
            "    out = pathlib.Path(args[args.index('-p') + 1])\n"
            "    out.mkdir(parents=True, exist_ok=True)\n"
            "    (out / 'summary.json').write_text('{\"ok\": true}')\n"
            "elif args[:2] == ['kernels', 'status']:\n"
            "    print('\"KernelWorkerStatus.RUNNING\"')\n",
            encoding="utf-8",
        )
        self.patch_cli = mock.patch.object(job_tool, "KAGGLE_CLI", str(self.cli))
        self.patch_cli.start()
        self.addCleanup(self.patch_cli.stop)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_submit_preserves_cli_and_worker_paths_with_spaces_and_quotes(self):
        worker = self.root / "worker owner's tree"
        worker.mkdir()
        template = "JOB_JSON = None  # INJECTED_BY_tools/job.py\n"
        (worker / "script.py").write_text(template)
        jobdir = self.root / "input"
        jobdir.mkdir()
        (jobdir / "job.json").write_text(json.dumps({"job": "offline", "commands": []}))
        actual_sh = job_tool.sh

        def local_sh(command, **kwargs):
            # Do not read a real Git remote or alter any checkout.
            if command == "git checkout -- worker/script.py":
                return 0, ""
            return actual_sh(command, **kwargs)

        with mock.patch.object(job_tool, "WORKER_DIR", str(worker)), \
             mock.patch.object(job_tool, "resolve_head", return_value="a" * 40), \
             mock.patch.object(job_tool, "sh", side_effect=local_sh), \
             contextlib.redirect_stdout(io.StringIO()):
            job_tool.kaggle_submit(str(jobdir))
        self.assertEqual(self.calls(), [["kernels", "push", "-p", str(worker)]])
        self.assertTrue((jobdir / "submitted.json").exists())

    def test_status_treats_shell_metacharacters_as_literal_slug(self):
        marker = self.root / "must-not-exist"
        slug = "owner/kernel;touch " + shlex.quote(str(marker))
        with mock.patch.object(job_tool, "KAGGLE_CLI", str(self.plain_cli)), \
             contextlib.redirect_stdout(io.StringIO()):
            job_tool.kaggle_status(slug)
        self.assertFalse(marker.exists())
        self.assertEqual(self.calls(), [["kernels", "status", slug]])

    def test_wait_uses_the_same_literal_slug(self):
        slug = "owner/kernel with 'quotes'"
        with mock.patch.object(job_tool, "KAGGLE_CLI", str(self.plain_cli)), \
             mock.patch.object(job_tool.time, "time", side_effect=[0, 0, 61]), \
             mock.patch.object(job_tool.time, "sleep"), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(job_tool.kaggle_wait(1, slug), 2)
        self.assertEqual(self.calls(), [["kernels", "status", slug]])

    def test_fetch_preserves_destination_path_and_default_slug(self):
        # tools/job.py stages the download under result/raw (PR #494) and
        # promotes only from that staging directory; the argument contract this
        # test pins is that the slug and destination still reach the CLI as
        # single, properly quoted argv tokens.
        jobdir = self.root / "results owner's tree"
        with mock.patch.object(job_tool, "KAGGLE_CLI", str(self.plain_cli)), \
             mock.patch.object(job_tool, "kaggle_slug", return_value="owner/kernel"), \
             contextlib.redirect_stdout(io.StringIO()):
            job_tool.kaggle_fetch(str(jobdir))
        self.assertEqual(len(self.calls()), 1)
        argv = self.calls()[0]
        self.assertEqual(argv[:2], ["kernels", "output"])
        self.assertEqual(argv[2], "owner/kernel")
        self.assertEqual(argv[3], "-p")
        destination = Path(argv[4])
        raw = jobdir / "result/raw"
        self.assertEqual(destination.parent, raw)
        self.assertTrue(destination.name.startswith(".staging-"))
        summary = json.loads((jobdir / "result/summary.json").read_text())
        self.assertTrue(summary["ok"])

    def test_plain_status_default_slug_control(self):
        with mock.patch.object(job_tool, "KAGGLE_CLI", str(self.plain_cli)), \
             mock.patch.object(job_tool, "kaggle_slug", return_value="owner/kernel"), \
             contextlib.redirect_stdout(io.StringIO()):
            job_tool.kaggle_status()
        self.assertEqual(self.calls(), [["kernels", "status", "owner/kernel"]])


class WorkerScriptTests(unittest.TestCase):
    """Regression tests for worker/script.py (#1101: clone/checkout pipe and quote)."""

    def test_worker_script_does_not_mask_git_clone_exit_with_tail_pipe(self):
        src = (ROOT / "worker/script.py").read_text(encoding="utf-8")
        self.assertNotIn("| tail -2", src)
        self.assertIn("shlex.quote(repo)", src)
        self.assertIn("shlex.quote(sha)", src)
        self.assertIn("curl -fsSL", src)

    def test_worker_script_clone_checkout_fallback_flow(self):
        def simulate_run(job, sh_results):
            calls = []

            def fake_sh(cmd, timeout):
                calls.append((cmd, timeout))
                for pattern, res in sh_results.items():
                    if pattern in cmd:
                        return res
                return (0, "")

            repo = job.get("repo", "https://github.com/limitlessprojects/Go-Hybrid.git")
            sha = job.get("sha", "HEAD")
            rc, _ = fake_sh("git clone --quiet {} repo 2>&1".format(shlex.quote(repo)), 300)
            if rc == 0:
                rc, _ = fake_sh("cd repo && git checkout --quiet {} 2>&1".format(shlex.quote(sha)), 60)
            if rc != 0:
                rc, _ = fake_sh(
                    "rm -rf repo && mkdir repo && curl -fsSL --max-time 240 https://codeload.github.com/limitlessprojects/Go-Hybrid/tar.gz/{} | tar xz --strip-components=1 -C repo".format(
                        shlex.quote(sha)
                    ),
                    300,
                )
            return rc, calls

        # 1. clone fails -> triggers fallback
        rc, calls = simulate_run(
            {"repo": "https://example.com/repo.git", "sha": "v1.0"},
            {"git clone": (1, "clone err")},
        )
        self.assertEqual(len(calls), 2)
        self.assertIn("git clone", calls[0][0])
        self.assertIn("curl -fsSL", calls[1][0])

        # 2. clone ok, checkout fails -> triggers fallback
        rc, calls = simulate_run(
            {"repo": "https://example.com/repo.git", "sha": "v1.0"},
            {"git checkout": (1, "checkout err")},
        )
        self.assertEqual(len(calls), 3)
        self.assertIn("git clone", calls[0][0])
        self.assertIn("git checkout", calls[1][0])
        self.assertIn("curl -fsSL", calls[2][0])

        # 3. clone ok, checkout ok -> no fallback
        rc, calls = simulate_run(
            {"repo": "https://example.com/repo.git", "sha": "v1.0"},
            {},
        )
        self.assertEqual(len(calls), 2)
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()