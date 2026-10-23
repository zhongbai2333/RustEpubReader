#!/usr/bin/env python3
"""Reject missing, stale or incomplete parallel Apple checks and transport damage."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
import uuid
from unittest.mock import patch


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


ci = module("ios_ci", "ios-ci.py")
fixtures = module("runtime_test_fixtures", "test-mobile-runtime.py")
runtime = ci.runtime


def tree(cases, iterations, device, family="iPad"):
    return {"devices": [{"deviceId": device, "modelName": family + " test device"}],
            "testNodes": [{"nodeType": "Test Suite", "children": [
                {"nodeType": "Test Case", "nodeIdentifier": name, "result": "Passed", "children": [
                    {"nodeType": "Repetition", "nodeIdentifier": str(i), "result": "Passed"}
                    for i in range(1, iterations + 1)] if iterations > 1 else []}
                for name in sorted(cases)]}]}


class ReceiptTests(unittest.TestCase):
    def test_all_cases_and_all_three_repetitions_are_required(self):
        good = tree(runtime.IOS_SELECTION_TESTS, 3, "owned")
        result = runtime.validate_ios_xctest(good, runtime.IOS_SELECTION_TESTS, 3, "owned")
        self.assertEqual(list(result["cases"].values()), [3] * 5)
        changes = [
            lambda t: t["testNodes"][0]["children"].pop(),
            lambda t: t["testNodes"][0]["children"].append(t["testNodes"][0]["children"][0]),
            lambda t: t["testNodes"][0]["children"][0]["children"].pop(),
            lambda t: t["testNodes"][0]["children"][0]["children"][0].update(result="Failed"),
            lambda t: t["testNodes"][0]["children"][0].update(result="Skipped"),
            lambda t: t["devices"][0].update(deviceId="another"),
        ]
        for change in changes:
            bad = copy.deepcopy(good); change(bad)
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                runtime.validate_ios_xctest(bad, runtime.IOS_SELECTION_TESTS, 3, "owned")

    def test_xctest_timeout_keeps_its_failure_and_prints_the_native_log(self):
        import contextlib
        import io
        import subprocess
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "native-ui.log"
            failure = subprocess.TimeoutExpired(["xcodebuild"], 300)
            def timeout(*args, **kwargs):
                kwargs["stdout"].write(b"first native launch timed out")
                raise failure
            output = io.StringIO()
            with patch.object(runtime.subprocess, "run", side_effect=timeout), contextlib.redirect_stdout(output):
                with self.assertRaises(subprocess.TimeoutExpired) as raised:
                    runtime.run_ios_xctest(["xcodebuild"], path, 300)
            self.assertIs(raised.exception, failure)
            self.assertIn("first native launch timed out", output.getvalue())

    def test_phone_selection_never_chooses_a_tablet_or_incompatible_runtime(self):
        devices = [{"identifier": "pad", "productFamily": "iPad"},
                   {"identifier": "future", "productFamily": "iPhone", "minRuntimeVersion": 0x1C0000},
                   {"identifier": "phone", "productFamily": "iPhone", "maxRuntimeVersion": 0xffffffff}]
        self.assertEqual(runtime.compatible_phone(devices, "26.4"), "phone")
        with self.assertRaisesRegex(ValueError, "No iPhone"):
            runtime.compatible_phone(devices[:2], "26.4")

    def test_shards_do_not_run_or_require_other_suites(self):
        from types import SimpleNamespace
        for shard in ("features", "selection", "motion"):
            with self.subTest(shard=shard), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); cache = root / "Library/Caches"; cache.mkdir(parents=True)
                args = SimpleNamespace(output=root, xctestrun=root / "source", timeout=1, ios_shard=shard)
                admission = {"data": str(root), "app": "/owned.app"}
                if shard == "selection":
                    for row in fixtures.SelectionTests.report("ios")["cases"]:
                        (cache / ("selection-ui-" + row["mode"] + ".json")).write_text(json.dumps(dict(row, passed=True)))
                    fixtures.IOSInputEvidenceTests.populate(cache)
                if shard == "motion":
                    (cache / "reduced-motion-verification.json").write_text(json.dumps(fixtures.ReducedMotionTests.ios_report()))
                with patch.object(runtime, "ios_selection_manifest", return_value=root / "manifest"), \
                     patch.object(runtime, "ios_installed_target", return_value=admission), \
                     patch.object(runtime, "ios_xctest_receipt", return_value={"passed": True}) as receipt, \
                     patch.object(runtime.subprocess, "run", return_value=SimpleNamespace(returncode=0)) as execute:
                    result = runtime.collect_ios_selection(args, "owned", "local.reader.columns")
                execute.assert_called_once(); self.assertTrue(result["passed"])
                command = execute.call_args.args[0]
                expected = runtime.IOS_FEATURE_TESTS if shard == "features" else runtime.IOS_SELECTION_TESTS if shard == "selection" else runtime.IOS_MOTION_TESTS
                self.assertEqual(receipt.call_args.args[2:5], (expected, 1 if shard == "features" else 3, "owned"))
                if shard == "features":
                    self.assertNotIn("-test-iterations", command)
                else:
                    self.assertEqual(command[command.index("-test-iterations") + 1], "3")
                    self.assertNotIn("-retry-tests-on-failure", command)
                if shard == "selection":
                    self.assertIn("-skip-testing:ReaderUITests/ReaderSelectionUITests/testSystemReducedMotionNavigation", command)
                if shard == "motion":
                    self.assertIn("-only-testing:ReaderUITests/ReaderSelectionUITests/testSystemReducedMotionNavigation", command)


class TransportTests(unittest.TestCase):
    def products(self, root):
        app, runner, manifest = fixtures.SelectionTests.products(root)
        info = plistlib.loads((runner / "Info.plist").read_bytes()); info["CFBundleExecutable"] = "runner"
        (runner / "Info.plist").write_bytes(plistlib.dumps(info))
        (runner / "runner").write_bytes(b"runner executable")
        for binary in (app / "ReaderPreview", runner / "runner"):
            binary.chmod(0o755)
        (app / "linked").symlink_to("ReaderPreview")
        return app, runner, manifest

    def test_roundtrip_preserves_executables_symlinks_and_native_build_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); app, runner, manifest = self.products(root / "Products")
            with patch.object(ci, "source_sha", return_value="a" * 40), patch.object(runtime, "run", return_value=b"Xcode pinned"):
                record = ci.pack(manifest.parent, manifest, root / "transport")
                ci.unpack(root / "transport", root / "restored")
            self.assertTrue((root / "restored/Debug-iphonesimulator/ReaderPreview.app/linked").is_symlink())
            self.assertTrue(os.access(root / "restored/Debug-iphonesimulator/ReaderPreview.app/ReaderPreview", os.X_OK))
            self.assertEqual(runtime.artifact_hash(app), record["app_sha256"])

    def test_corrupt_transport_and_another_commit_are_rejected(self):
        for corruption in ("transport", "source", "collector"):
            with self.subTest(corruption=corruption), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); app, runner, manifest = self.products(root / "Products")
                with patch.object(ci, "source_sha", return_value="a" * 40), patch.object(runtime, "run", return_value=b"Xcode pinned"):
                    ci.pack(manifest.parent, manifest, root / "transport")
                    if corruption == "transport":
                        (root / "transport/simulator-tests.tar.gz").write_bytes(b"corrupt")
                    else:
                        p = root / "transport/build.json"; data = json.loads(p.read_text())
                        data["source_sha" if corruption == "source" else "collector_sha256"] = "b" * 40
                        p.write_text(json.dumps(data))
                    with self.assertRaises(RuntimeError): ci.unpack(root / "transport", root / "restored")

    def test_failed_job_rerun_reuses_only_an_earlier_build_from_the_same_workflow(self):
        record = {"schema": 1, "source_sha": ci.source_sha(),
                  "collector_sha256": runtime.artifact_hash(Path(__file__).with_name("check-mobile-runtime.py")),
                  "producer_run_id": "owned-run", "producer_run_attempt": "1"}
        with patch.dict(os.environ, {"GITHUB_RUN_ID": "owned-run", "GITHUB_RUN_ATTEMPT": "2"}):
            ci.check_record(record)
            for change in ({"producer_run_id": "another-run"}, {"producer_run_attempt": "3"}):
                with self.subTest(change=change), self.assertRaises(RuntimeError):
                    ci.check_record(dict(record, **change))


class AggregateTests(unittest.TestCase):
    def fixture(self, root):
        record = {"schema": 1, "source_sha": ci.source_sha(),
                  "collector_sha256": runtime.artifact_hash(Path(__file__).with_name("check-mobile-runtime.py")),
                  "producer_run_id": os.environ.get("GITHUB_RUN_ID", "local"),
                  "producer_run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", "1"),
                  "app_sha256": "app", "ui_runner_sha256": "runner", "xctestrun_sha256": "manifest"}
        build = root / "build.json"; build.write_text(json.dumps(record))
        reports = root / "reports"; reports.mkdir()
        for name, (shard, family) in ci.SHARDS.items():
            folder = reports / ("test-receipt-" + name); folder.mkdir()
            device = str(uuid.uuid4())
            checks = {"single-column": {"passed": True}, "navigation": {"passed": True}, "corpus": fixtures.CorpusCoverageTests.report("ios")} if shard == "layout" else {shard: {"passed": True}}
            summary = {"passed": True, "platform": "ios", "ios_shard": shard, "ios_device_family": family,
                       "source_sha": record["source_sha"], "build_record_sha256": runtime.artifact_hash(build),
                       "collector_sha256": record["collector_sha256"], "artifact_sha256": "app", "ui_runner_sha256": "runner",
                       "xctestrun_sha256": "manifest", "artifacts_unchanged": True, "run_id": str(uuid.uuid4()),
                       "ios_runtime": {"simulator_id": device, "device_type": "iPad" if family == "ipad" else "iPhone", "version": "26.4.1"}, "checks": checks}
            if shard != "layout":
                (folder / "xctest-installation.json").write_text(json.dumps({"unchanged": True, "before": {"binary": "same"}, "after": {"binary": "same"}}))
                cases = runtime.IOS_FEATURE_TESTS if shard == "features" else runtime.IOS_SELECTION_TESTS if shard == "selection" else runtime.IOS_MOTION_TESTS
                result = tree(cases, 1 if shard == "features" else 3, device, "iPad" if family == "ipad" else "iPhone")
                (folder / ("platform-parity-tests.json" if shard == "features" else "selection-tests.json")).write_text(json.dumps(result))
                if shard == "features": checks[shard]["platform_parity"] = {"passed": True}
                elif shard == "selection":
                    checks[shard] = fixtures.SelectionTests.report("ios")
                    fixtures.IOSInputEvidenceTests.populate(folder)
                else: checks[shard]["reduced_motion"] = fixtures.ReducedMotionTests.ios_report()
            (folder / "summary.json").write_text(json.dumps(summary))
        return reports, build

    def test_complete_five_shards_pass_with_three_reading_and_motion_repetitions(self):
        with tempfile.TemporaryDirectory() as directory:
            reports, build = self.fixture(Path(directory))
            result = ci.aggregate(reports, build, "test")
            self.assertEqual(set(result["shards"]), set(ci.SHARDS)); self.assertEqual(result["motion_iterations"], 3)

    def test_missing_failed_stale_or_replaced_build_cannot_be_aggregated(self):
        for change in ("missing", "failed", "old-source", "old-build", "changed-inputs", "wrong-family", "wrong-runtime", "reinstalled", "motion-repetition", "selection-run", "corpus"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                reports, build = self.fixture(Path(directory)); folder = reports / "test-receipt-ipad-motion"
                if change == "missing":
                    (folder / "summary.json").unlink()
                elif change == "reinstalled":
                    (folder / "xctest-installation.json").write_text('{"unchanged":false}')
                elif change == "motion-repetition":
                    p = folder / "selection-tests.json"; v = json.loads(p.read_text()); v["testNodes"][0]["children"][0]["children"].pop(); p.write_text(json.dumps(v))
                elif change == "selection-run":
                    next((reports / "test-receipt-ipad-selection").glob("selection-run-*.json")).unlink()
                else:
                    if change == "corpus": folder = reports / "test-receipt-ipad-layout"
                    p = folder / "summary.json"; v = json.loads(p.read_text())
                    if change == "failed": v["passed"] = False
                    elif change == "old-source": v["source_sha"] = "old"
                    elif change == "old-build": v["build_record_sha256"] = "old"
                    elif change == "changed-inputs": v["artifacts_unchanged"] = False
                    elif change == "wrong-family": v["ios_device_family"] = "iphone"
                    elif change == "wrong-runtime": v["ios_runtime"]["version"] = "26.40"
                    elif change == "corpus": v["checks"]["corpus"]["corpusLayouts"].pop()
                    p.write_text(json.dumps(v))
                with self.assertRaises((RuntimeError, OSError, ValueError)):
                    ci.aggregate(reports, build, "test")


if __name__ == "__main__":
    unittest.main()
