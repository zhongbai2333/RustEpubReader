#!/usr/bin/env python3
"""Exercise cache setup shell steps without installing system packages."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]


def shell_step(path, name):
    """Read a literal run block from one named workflow/composite step."""
    lines = (ROOT / path).read_text().splitlines()
    start = next(i for i, line in enumerate(lines) if line.strip() == f"- name: {name}")
    run = next(i for i in range(start + 1, len(lines)) if lines[i].strip() == "run: |")
    indent = len(lines[run]) - len(lines[run].lstrip())
    end = run + 1
    while end < len(lines) and (not lines[end].strip() or len(lines[end]) - len(lines[end].lstrip()) > indent):
        end += 1
    return textwrap.dedent("\n".join(lines[run + 1:end]))


class CacheSetupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        RUNNER_TEMP=str(self.root), GITHUB_PATH=str(self.root / "github-path"))

    def run_shell(self, script):
        return subprocess.run(["bash", "-e", "-o", "pipefail"], input=script,
                              text=True, capture_output=True, env=self.env, timeout=20)

    def policy_script(self):
        script = shell_step(".github/actions/install-linux-dependencies/action.yml",
                            "Resolve current package versions")
        # Exercise the actual policy loop; APT update/install stay outside tests.
        start = script.index('versions=""')
        end = script.index("\ndone", start) + len("\ndone")
        self.env["package_names"] = "libclang-dev"
        return 'set -euo pipefail\n' + script[start:end] + '\nprintf "%s" "$versions"\n'

    def install_policy_producer(self):
        producer = self.bin / "apt-cache"
        producer.write_text("#!" + shutil.which("python3") + "\n" + textwrap.dedent('''
            import os
            import sys
            if os.environ.get("POLICY_ERROR"):
                sys.exit(int(os.environ["POLICY_ERROR"]))
            value = os.environ.get("CANDIDATE", "1:21.1.6-71")
            try:
                os.write(1, ("libclang-dev:\\n  Candidate: " + value + "\\n").encode())
                # Exceed the pipe buffer so an early consumer exit cannot hide.
                for _ in range(2048):
                    os.write(1, b"     version table entry " + b"x" * 1000 + b"\\n")
            except BrokenPipeError:
                os._exit(100)
        '''))
        producer.chmod(0o755)

    def test_policy_output_is_fully_consumed(self):
        self.install_policy_producer()
        result = self.run_shell(self.policy_script())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "libclang-dev=1:21.1.6-71\n")

    def test_policy_errors_are_not_hidden(self):
        self.install_policy_producer()
        self.env["POLICY_ERROR"] = "100"
        self.assertEqual(self.run_shell(self.policy_script()).returncode, 100)

    def test_unavailable_candidate_is_rejected(self):
        self.install_policy_producer()
        self.env["CANDIDATE"] = "(none)"
        result = self.run_shell(self.policy_script())
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("No install candidate", result.stderr)

    @unittest.skipUnless(shutil.which("cargo"), "Cargo is required to test plugin dispatch")
    def test_cached_ndk_uses_cargo_plugin_dispatch(self):
        tool_dir = self.root / "reader-cargo-ndk" / "bin"
        tool_dir.mkdir(parents=True)
        plugin = tool_dir / "cargo-ndk"
        plugin.write_text('#!/bin/sh\n[ "$1" = ndk ] && [ "$2" = --version ] || exit 1\n'
                          'echo "cargo-ndk 4.1.2"\n')
        plugin.chmod(0o755)
        script = shell_step(".github/workflows/android-build.yml", "Verify cargo-ndk")
        result = self.run_shell(script)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cargo-ndk 4.1.2", result.stdout)
        self.assertEqual(Path(self.env["GITHUB_PATH"]).read_text(), str(tool_dir) + "\n")

    def test_missing_ndk_executable_is_rejected(self):
        script = shell_step(".github/workflows/android-build.yml", "Verify cargo-ndk")
        self.assertNotEqual(self.run_shell(script).returncode, 0)


if __name__ == "__main__":
    unittest.main()
