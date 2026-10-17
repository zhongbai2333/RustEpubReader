#!/usr/bin/env python3
"""Exercise version updates in isolated copies; never bump this checkout."""

import importlib.util
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('bump_version', ROOT / 'scripts/bump-version.py')
bump = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bump)


class VersionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='reader-version-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in (*bump.FILES, 'scripts/bump-version.py'):
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, destination)
        # Normalize the fixture so tests remain valid after future releases.
        _, texts, workspace, package, *_ = bump.load(self.root)
        pattern = r'^version[ \t]*=[ \t]*"([^"\r\n]+)"'
        texts[bump.FILES[0]] = texts[bump.FILES[0]].replace(workspace, bump.replace_value(workspace, pattern, '1.2.3'), 1)
        texts[bump.FILES[1]] = texts[bump.FILES[1]].replace(package, bump.replace_value(package, pattern, '1.2.3'), 1)
        texts[bump.FILES[2]] = bump.replace_value(texts[bump.FILES[2]], r'^APP_VERSION_NAME=([^\r\n]+)', '1.2.3')
        texts[bump.FILES[2]] = bump.replace_value(texts[bump.FILES[2]], r'^APP_VERSION_CODE=([^\r\n]+)', '9')
        texts[bump.FILES[3]] = bump.replace_value(texts[bump.FILES[3]], r'\bMARKETING_VERSION = ([^;]+);', '1.2.3')
        texts[bump.FILES[3]] = bump.replace_value(texts[bump.FILES[3]], r'\bCURRENT_PROJECT_VERSION = ([^;]+);', '1017.3')
        for name, text in texts.items():
            (self.root / name).write_bytes(text.encode('utf-8'))

    def snapshot(self):
        return {name: (self.root / name).read_bytes() for name in bump.FILES}

    def run_cli(self, *args, success=True):
        result = subprocess.run([sys.executable, str(self.root / 'scripts/bump-version.py'), *args],
                                cwd=self.temp.name, capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def test_all_release_increments_and_explicit_version(self):
        for command, expected in [('patch', '1.2.4'), ('minor', '1.3.0'), ('major', '2.0.0'), ('v3.0.1', '3.0.1')]:
            with self.subTest(command=command):
                original = self.snapshot()
                self.run_cli(command)
                self.assertEqual(bump.load(self.root)[4:], (expected, 10, '1017.4'))
                self.run_cli('--check', '--tag', 'v' + expected)
                # No dependencies/other packages are modified in the lockfile.
                old_packages = [p for p in re.findall(bump.LOCK_PACKAGE, original['Cargo.lock'].decode())
                                if 'name = "rust_epub_reader"\n' not in p]
                new_packages = [p for p in re.findall(bump.LOCK_PACKAGE, (self.root / 'Cargo.lock').read_text())
                                if 'name = "rust_epub_reader"\n' not in p]
                self.assertEqual(old_packages, new_packages)
                for name, content in original.items():
                    (self.root / name).write_bytes(content)

    def test_dry_run_and_check_never_write(self):
        original = self.snapshot()
        self.assertIn('1.2.4', self.run_cli('patch', '--dry-run').stdout)
        self.run_cli('--check', '--tag', 'v1.2.3')
        self.run_cli('--check', '--tag', 'v9.9.9', success=False)
        self.assertEqual(original, self.snapshot())

    def test_build_only_and_overrides(self):
        original = self.snapshot()
        self.run_cli('--build-only')
        self.assertEqual(bump.load(self.root)[4:], ('1.2.3', 10, '1017.4'))
        self.run_cli('--build-only', '--android-code', '20', '--ios-build', '1018.1')
        self.assertEqual(bump.load(self.root)[4:], ('1.2.3', 20, '1018.1'))
        self.assertEqual(original['Cargo.toml'], (self.root / 'Cargo.toml').read_bytes())
        self.assertEqual(original['Cargo.lock'], (self.root / 'Cargo.lock').read_bytes())

    def test_invalid_requests_leave_every_file_untouched(self):
        original = self.snapshot()
        for args in [[], ['1.2.3'], ['1.2.2'], ['01.3.0'], ['2.0.0-beta.1'], ['2.0'],
                     ['--build-only', 'patch'], ['patch', '--android-code', '9'],
                     ['patch', '--android-code', '2100000001'], ['patch', '--ios-build', '1017.2'],
                     ['patch', '--ios-build', '0'], ['--check', 'patch'], ['--tag', 'v1.2.3']]:
            with self.subTest(args=args):
                self.run_cli(*args, success=False)
                self.assertEqual(original, self.snapshot())

    def test_conflicting_or_missing_fields_prevent_partial_update(self):
        for name, old, new in [
            ('android/gradle.properties', 'APP_VERSION_NAME=1.2.3', 'APP_VERSION_NAME=8.0.0'),
            ('Cargo.lock', 'name = "rust_epub_reader"', 'name = "renamed_reader"'),
            ('ios/RustEpubReader.xcodeproj/project.pbxproj', 'MARKETING_VERSION = 1.2.3;', 'MARKETING_VERSION = 7.0.0;'),
            ('ios/RustEpubReader.xcodeproj/project.pbxproj', 'CURRENT_PROJECT_VERSION = 1017.3;', 'CURRENT_PROJECT_VERSION = 4;'),
        ]:
            with self.subTest(name=name, old=old):
                path = self.root / name
                original = path.read_bytes()
                path.write_bytes(original.replace(old.encode(), new.encode(), 1))
                snapshot = self.snapshot()
                self.run_cli('patch', success=False)
                self.assertEqual(snapshot, self.snapshot())
                path.write_bytes(original)

    def test_crlf_and_file_modes_preserved(self):
        for name in bump.FILES:
            path = self.root / name
            path.write_bytes(path.read_bytes().replace(b'\n', b'\r\n'))
            path.chmod(0o640)
        self.run_cli('patch')
        for name in bump.FILES:
            path = self.root / name
            self.assertNotIn(b'\n', path.read_bytes().replace(b'\r\n', b''))
            if sys.platform != 'win32':
                self.assertEqual(path.stat().st_mode & 0o777, 0o640)

    def test_mid_write_failure_rolls_back(self):
        original, changed, _ = bump.prepare(self.root, 'patch')
        replace = bump.os.replace
        calls = 0

        def fail_second(source, destination):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError('Simulated write failure')
            return replace(source, destination)

        with mock.patch.object(bump.os, 'replace', side_effect=fail_second):
            with self.assertRaises(OSError):
                bump.write_changes(self.root, original, changed)
        self.assertEqual(original, self.snapshot())
        self.assertFalse(list(self.root.rglob('.reader-version-*')))

    def test_concurrent_change_is_preserved(self):
        original, changed, _ = bump.prepare(self.root, 'patch')
        path = self.root / 'android/gradle.properties'
        path.write_bytes(path.read_bytes() + b'\n# concurrent edit\n')
        snapshot = self.snapshot()
        with self.assertRaises(ValueError):
            bump.write_changes(self.root, original, changed)
        self.assertEqual(snapshot, self.snapshot())
        self.assertFalse(list(self.root.rglob('.reader-version-*')))


if __name__ == '__main__':
    unittest.main()
