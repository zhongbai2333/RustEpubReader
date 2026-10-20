#!/usr/bin/env python3
"""Regress report acceptance and stale-evidence rejection without launching devices."""
import importlib.util
import json
import plistlib
from types import SimpleNamespace
from pathlib import Path
import tempfile
import shlex
import subprocess
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("runtime", Path(__file__).with_name("check-mobile-runtime.py"))
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)


class ReportTests(unittest.TestCase):
    def test_device_type_must_support_the_installed_runtime(self):
        devices = [
            {"identifier": "too-new", "productFamily": "iPad", "minRuntimeVersion": 28 << 16, "maxRuntimeVersion": 0xffffffff},
            {"identifier": "phone", "productFamily": "iPhone", "minRuntimeVersion": 0, "maxRuntimeVersion": 0xffffffff},
            {"identifier": "supported", "productFamily": "iPad", "minRuntimeVersion": 26 << 16, "maxRuntimeVersion": 0xffffffff},
            {"identifier": "obsolete", "productFamily": "iPad", "minRuntimeVersion": 0, "maxRuntimeVersion": 15 << 16},
        ]
        self.assertEqual(runtime.compatible_tablet(devices, "27.2"), "supported")
        with self.assertRaises(ValueError):
            runtime.compatible_tablet(devices[:2], "27.2")

    def test_refuses_non_diagnostic_ios_bundle_before_creating_simulator(self):
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory)
            (app / "Info.plist").write_bytes(plistlib.dumps({"CFBundleSupportedPlatforms": ["iPhoneSimulator"], "CFBundleIdentifier": "com.example.real"}))
            with self.assertRaisesRegex(ValueError, "isolated iOS"):
                runtime.ios(SimpleNamespace(app=app))

    def test_artifact_identity_detects_changed_bytes_paths_and_absent_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); binary = root / "binary"; binary.write_bytes(b"owned fixture")
            first_file, first_directory = runtime.artifact_hash(binary), runtime.artifact_hash(root)
            binary.write_bytes(b"changed fixture")
            self.assertNotEqual(first_file, runtime.artifact_hash(binary))
            binary.write_bytes(b"owned fixture"); binary.rename(root / "renamed")
            self.assertNotEqual(first_directory, runtime.artifact_hash(root))
            with self.assertRaises(ValueError): runtime.artifact_hash(binary)

    def test_only_explicit_pass_is_accepted(self):
        for report in [{}, {"passed": False}, {"passed": "true"}, {"passed": 1}, []]:
            with self.subTest(report=report), self.assertRaises(RuntimeError):
                runtime.validate_report(json.dumps(report))
        self.assertEqual(runtime.validate_report('{"passed":true}'), {"passed": True})

    def test_absent_report_times_out_and_failed_report_fails_immediately(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            with self.assertRaises(TimeoutError):
                runtime.wait_report(path, 0)
            path.write_text('{"passed":false,"error":"native check failed"}')
            with self.assertRaisesRegex(RuntimeError, "native check failed"):
                runtime.wait_report(path, 1)


class IOSLifecycleTests(unittest.TestCase):
    def fixture(self, root):
        app = root / "Reader.app"; app.mkdir()
        (app / "Info.plist").write_bytes(plistlib.dumps({"CFBundleSupportedPlatforms": ["iPhoneSimulator"], "CFBundleIdentifier": "local.reader.columns"}))
        output = root / "output"; output.mkdir()
        args = SimpleNamespace(app=app, output=output, timeout=1)
        def listing(*command):
            if command[3] == "runtimes":
                return json.dumps({"runtimes": [{"isAvailable": True, "identifier": "com.apple.CoreSimulator.SimRuntime.iOS-26-3", "version": "26.3"}]}).encode()
            return json.dumps({"devicetypes": [{"identifier": "test-ipad", "productFamily": "iPad", "minRuntimeVersion": 0, "maxRuntimeVersion": 0xffffffff}]}).encode()
        return args, listing

    def test_fresh_app_and_later_cases_launch_without_separate_terminate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); args, listing = self.fixture(root); calls = []
            def device(output, *command, **kwargs):
                calls.append(command)
                if command[0] == "create": return b"owned-device\n"
                if command[0] == "get_app_container": return str(root / "container").encode()
                self.assertNotEqual(command[0], "terminate", "A never-launched app must not receive a terminate request")
                return b""
            with patch.object(runtime, "run", side_effect=listing), patch.object(runtime, "ios_simctl", side_effect=device), patch.object(runtime, "wait_report", return_value={"passed": True}):
                result = runtime.ios(args)
            self.assertEqual(set(result), set(runtime.CASES))
            launches = [c for c in calls if c[0] == "launch"]
            self.assertEqual(len(launches), len(runtime.CASES))
            self.assertTrue(all(c[1:4] == ("--terminate-running-process", "owned-device", "local.reader.columns") for c in launches))
            self.assertEqual(calls[-2:], [("shutdown", "owned-device"), ("delete", "owned-device")])

    def test_cleanup_attempts_delete_after_shutdown_timeout_and_preserves_original_error(self):
        with tempfile.TemporaryDirectory() as directory:
            args, listing = self.fixture(Path(directory)); calls = []
            def device(output, *command, **kwargs):
                calls.append(command)
                if command[0] == "create": return b"owned-device\n"
                if command[0] == "boot": raise RuntimeError("original boot failure")
                raise subprocess.TimeoutExpired(command, 45)
            with patch.object(runtime, "run", side_effect=listing), patch.object(runtime, "ios_simctl", side_effect=device):
                with self.assertRaisesRegex(RuntimeError, "original boot failure"):
                    runtime.ios(args)
            self.assertEqual(calls[-2:], [("shutdown", "owned-device"), ("delete", "owned-device")])

    def test_timed_out_command_remains_a_failure_with_diagnostics(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            error = subprocess.TimeoutExpired(["xcrun", "simctl", "launch"], 180, stderr=b"simulator unavailable")
            with patch.object(runtime.subprocess, "run", side_effect=error):
                with self.assertRaises(subprocess.TimeoutExpired):
                    runtime.ios_simctl(output, "launch", "owned-device", "local.reader.columns")
            rows = [json.loads(line) for line in (output / "simulator-commands.jsonl").read_text().splitlines()]
            self.assertEqual([r["status"] for r in rows], ["started", "failed"])
            self.assertEqual(rows[-1]["stderr"], "simulator unavailable")


class CorpusCoverageTests(unittest.TestCase):
    @staticmethod
    def report(platform):
        key = "layouts" if platform == "android" else "corpusLayouts"
        coverage = "canonical_text_coverage" if platform == "android" else "contiguousCoverage"
        return {"passed": True, "minimum_scalar_fragment_integrity": True, key: [dict(name=case["name"], chapter=case["chapter"], policy=policy, width=width, base_font_size=case.get("font_size",18),
            pages=2, drawn_pages=[0, 1], screenshot_names=[f"corpus-{case['name']}-{case['chapter']}-{width}-0.png"] if policy=="mixed" else [], images=case.get("min_images", 0), tables=case.get("min_tables", 0),
            notes_resolved=case.get("min_notes", 0), **{coverage: True})
            for case in runtime.CORPUS_CASES for policy in ("none", "mixed", "publisher") for width in (320, 640)]}

    def test_accepts_complete_native_semantic_and_drawing_coverage(self):
        for platform in ("android", "ios"):
            report = self.report(platform)
            self.assertEqual(runtime.validate_corpus(report, platform), report)

    def test_requires_the_actual_native_scalar_fragment_probe(self):
        report = self.report("android"); report.pop("minimum_scalar_fragment_integrity")
        with self.assertRaisesRegex(RuntimeError, "scalar-fragment"):
            runtime.validate_corpus(report, "android")

    def test_rejects_missing_duplicate_and_unexpected_layouts(self):
        for platform in ("android", "ios"):
            key = "layouts" if platform == "android" else "corpusLayouts"
            for mutation in (lambda rows: rows.pop(), lambda rows: rows.__setitem__(-1, rows[0]), lambda rows: rows[0].update(width=0)):
                report = self.report(platform); mutation(report[key])
                with self.assertRaises(RuntimeError): runtime.validate_corpus(report, platform)

    def test_rejects_vacuous_drawing_and_missing_semantics(self):
        for platform in ("android", "ios"):
            key = "layouts" if platform == "android" else "corpusLayouts"
            coverage = "canonical_text_coverage" if platform == "android" else "contiguousCoverage"
            for field, value in (("pages", 0), ("pages", True), ("drawn_pages", []), ("drawn_pages", [2]),
                                 ("screenshot_names", ["../stale.png"]), ("base_font_size", 18), ("images", 0), ("tables", 0), ("notes_resolved", 0), (coverage, False)):
                report = self.report(platform); report[key][0][field] = value
                with self.subTest(platform=platform, field=field), self.assertRaises(RuntimeError): runtime.validate_corpus(report, platform)


class TalkBackRestorationTests(unittest.TestCase):
    class Device:
        def __init__(self, emulator=True, fail_activation=False):
            self.emulator=emulator;self.fail_activation=fail_activation
            self.values={"enabled_accessibility_services":"owned.pkg/.Outer$Service", "accessibility_enabled":"0"}
            self.initial=dict(self.values);self.mutations=0;self.stopped=False
        def __call__(self,*args):
            if args[:3]==("shell","getprop","ro.kernel.qemu"):return b"1" if self.emulator else b"0"
            if args[:3]==("shell","pm","path"):return b"package:/product/app/talkback/talkback.apk"
            if args==("shell","am","force-stop","com.google.android.marvin.talkback"):
                self.stopped=True;return b""
            operation,key=args[2],args[4]
            if operation=="get":return self.values.get(key,"null").encode()
            self.mutations+=1
            if operation=="delete":self.values.pop(key,None);return b""
            value=shlex.split(args[5])[0]
            if self.fail_activation and key=="accessibility_enabled" and value=="1":raise RuntimeError("owned activation failure")
            self.values[key]=value;return b""

    def test_restores_existing_services_after_case_failure(self):
        device=self.Device();state={}
        with self.assertRaisesRegex(RuntimeError,"owned case failure"):
            with runtime.talkback_session(device,state):raise RuntimeError("owned case failure")
        self.assertEqual(device.values,device.initial);self.assertTrue(state["restored"])
        self.assertTrue(device.stopped)

    def test_does_not_stop_an_initially_configured_talkback_service(self):
        for component in ("com.google.android.marvin.talkback/.TalkBackService", "com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService"):
            with self.subTest(component=component):
                device=self.Device();state={}
                device.values["enabled_accessibility_services"]=component
                device.values["accessibility_enabled"]="1";before=dict(device.values)
                with runtime.talkback_session(device,state):pass
                self.assertEqual(device.values,before);self.assertFalse(device.stopped)
                self.assertFalse(state["owned_service_stopped"])

    def test_restores_null_setting_after_partial_activation_failure(self):
        device=self.Device(fail_activation=True);device.values.pop("enabled_accessibility_services");initial=dict(device.values);state={}
        with self.assertRaisesRegex(RuntimeError,"owned activation failure"):
            with runtime.talkback_session(device,state):self.fail("Activation unexpectedly succeeded")
        self.assertEqual(device.values,initial);self.assertTrue(state["restored"])

    def test_refuses_physical_device_before_settings_mutation(self):
        device=self.Device(emulator=False)
        with self.assertRaisesRegex(ValueError,"emulator"):
            with runtime.talkback_session(device,{}):self.fail("Physical device accepted")
        self.assertEqual(device.mutations,0)


class SelectionTests(unittest.TestCase):
    @staticmethod
    def report(platform):
        modes=("None","Realistic","Slide","Cover","Scroll") if platform=="android" else ("none","curl","slide","cover","scroll")
        return {"passed":True,"cases":[dict(mode=mode,persisted=True,text="文字 😀 中文",start_scalar=0,end_scalar=7,start_block=1,end_block=1,native_touch=True,native_drag=True,visible_highlight_action=True,quick_curl_swipe_exact_return=True) for mode in modes]}

    def test_accepts_all_native_gesture_and_persisted_range_cases(self):
        for platform in ("android","ios"):
            report=self.report(platform)
            self.assertEqual(runtime.validate_selection(report,platform),report)

    def test_rejects_partial_duplicate_wrong_scalar_and_action_only_results(self):
        for platform in ("android","ios"):
            changes=(lambda rows:rows.pop(),lambda rows:rows.__setitem__(-1,rows[0]),lambda rows:rows[0].update(persisted=False),lambda rows:rows[0].update(end_scalar=8),lambda rows:rows[0].update(start_scalar=False),lambda rows:rows[0].update(text="wrong excerpt"),lambda rows:rows[0].update(native_touch=False,native_drag=False))
            for change in changes:
                report=self.report(platform);change(report["cases"])
                with self.assertRaises(RuntimeError):runtime.validate_selection(report,platform)
        report=self.report("android");report["cases"][1]["quick_curl_swipe_exact_return"]=False
        with self.assertRaises(RuntimeError):runtime.validate_selection(report,"android")

    @staticmethod
    def products(root):
        product=root/"Debug-iphonesimulator";app=product/"ReaderPreview.app";runner=product/"ReaderUITests-Runner.app"
        app.mkdir(parents=True);(runner/"PlugIns/ReaderUITests.xctest").mkdir(parents=True)
        (app/"Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier":"local.reader.columns"}))
        (runner/"Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier":"local.reader.ui-files.tests.xctrunner","CFBundleSupportedPlatforms":["iPhoneSimulator"]}))
        manifest=root/"ReaderUITests.xctestrun"
        manifest.write_bytes(plistlib.dumps({"ReaderUITests":{"UITargetAppPath":"__TESTROOT__/Debug-iphonesimulator/ReaderPreview.app","TestHostPath":"__TESTROOT__/Debug-iphonesimulator/ReaderUITests-Runner.app","TestBundlePath":"__TESTHOST__/PlugIns/ReaderUITests.xctest"}}))
        return app,runner,manifest

    def test_binds_ui_runner_and_manifest_to_the_actual_app(self):
        with tempfile.TemporaryDirectory() as directory:
            app,runner,manifest=self.products(Path(directory))
            self.assertEqual(runtime.ios_selection_inputs(app,manifest),runner.resolve())
            other=Path(directory)/"Other.app";other.mkdir();(other/"binary").write_bytes(b"other build")
            with self.assertRaisesRegex(ValueError,"target differs"):runtime.ios_selection_inputs(other,manifest)

    def test_refuses_escaped_runner_or_absent_ui_test_bundle(self):
        with tempfile.TemporaryDirectory() as directory:
            app,runner,manifest=self.products(Path(directory))
            (runner/"PlugIns/ReaderUITests.xctest").rmdir()
            with self.assertRaisesRegex(ValueError,"XCTest bundle"):runtime.ios_selection_inputs(app,manifest)
            value=plistlib.loads(manifest.read_bytes());value["ReaderUITests"]["TestHostPath"]="__TESTROOT__/../outside.app";manifest.write_bytes(plistlib.dumps(value))
            with self.assertRaisesRegex(ValueError,"escapes"):runtime.ios_selection_inputs(app,manifest)

    def test_selection_results_use_the_container_after_xctest_reinstallation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);output=root/"output";output.mkdir()
            current=root/"new-container";cache=current/"Library/Caches";cache.mkdir(parents=True)
            ran=False
            def execute(*args,**kwargs):
                nonlocal ran
                ran=True
                (cache/"reduced-motion-verification.json").write_text(json.dumps(ReducedMotionTests.ios_report()))
                for row in self.report("ios")["cases"]:
                    (cache/("selection-ui-"+row["mode"]+".json")).write_text(json.dumps(dict(row,passed=True)))
                return SimpleNamespace(returncode=0)
            def lookup(output, *args):
                self.assertTrue(ran,"Container path must be resolved after XCTest runs")
                self.assertEqual(args[:1],("get_app_container",))
                return str(current).encode()
            args=SimpleNamespace(output=output,xctestrun=root/"owned.xctestrun",timeout=1)
            with patch.object(runtime.subprocess,"run",side_effect=execute),patch.object(runtime,"ios_simctl",side_effect=lookup):
                value=runtime.collect_ios_selection(args,"owned-device","local.reader.columns")
            self.assertTrue(value["passed"]);self.assertEqual(len(list(output.glob("selection-ui-*.json"))),5)


class ReducedMotionTests(unittest.TestCase):
    @staticmethod
    def ios_report():
        return dict(passed=True,system_reduce_motion=True,reduced_motion_geometry_checks=15,automatic_remote_offer=True,no_local_echo=True,cases=[dict(mode=mode,cross_chapter=True,same_chapter_offset=True,precise_return=True,reflow=True) for mode in ("none","curl","slide","cover","scroll")])
    def test_real_system_policy_and_full_coverage_are_required(self):
        good=self.ios_report();self.assertEqual(runtime.validate_reduced_motion(good,"ios"),good)
        for key,value in (("system_reduce_motion",False),("reduced_motion_geometry_checks",0),("reduced_motion_geometry_checks",True),("cases",[]),("cases",[None]*5),("no_local_echo",False)):
            bad=self.ios_report();bad[key]=value
            with self.assertRaises(RuntimeError):runtime.validate_reduced_motion(bad,"ios")
        android=SelectionTests.report("android");android["system_animators_enabled"]=False
        self.assertEqual(runtime.validate_reduced_motion(android,"android"),android)
        for value in (None,True,0):
            android["system_animators_enabled"]=value
            with self.assertRaises(RuntimeError):runtime.validate_reduced_motion(android,"android")
    class Device:
        def __init__(self,emulator=True,fail=False):
            self.emulator=emulator;self.fail=fail;self.changed=0
            self.values=dict(window_animation_scale="null",transition_animation_scale="0.5",animator_duration_scale="1.0")
            self.before=dict(self.values)
        def __call__(self,*args):
            if args==("shell","getprop","ro.kernel.qemu"):return b"1" if self.emulator else b"0"
            assert args[:2]==("shell","settings") and args[3]=="global",args
            action,key=args[2],args[4]
            if action=="get":return self.values[key].encode()
            self.changed+=1
            if self.fail and self.changed==2:raise RuntimeError("injected partial settings failure")
            self.values[key]="null" if action=="delete" else shlex.split(args[5])[0]
            return b""
    def test_restores_original_or_absent_keys_after_success_and_failures(self):
        for failure in ("none","body","activation"):
            device=self.Device(fail=failure=="activation");state={}
            try:
                with runtime.reduced_motion_session(device,state):
                    self.assertEqual(set(device.values.values()),{"0"})
                    if failure=="body":raise RuntimeError("injected native failure")
            except RuntimeError:
                self.assertNotEqual(failure,"none")
            self.assertEqual(device.values,device.before);self.assertTrue(state["restored"])
    def test_refuses_physical_device_before_any_settings_change(self):
        device=self.Device(emulator=False)
        with self.assertRaises(ValueError):
            with runtime.reduced_motion_session(device,{}):pass
        self.assertEqual(device.changed,0)


class FailureEvidenceTests(unittest.TestCase):
    def test_native_failure_or_missing_report_still_captures_the_screen(self):
        for missing in (False,True):
            with tempfile.TemporaryDirectory() as directory:
                root=Path(directory)
                def device(*args):
                    if args==("exec-out","screencap","-p"):return b"\x89PNG\r\n\x1a\nowned screenshot"
                    if missing:raise RuntimeError("missing owned report")
                    return b'{"passed":false,"error":"actual native failure"}'
                with self.assertRaisesRegex(RuntimeError,"missing owned report" if missing else "actual native failure"):
                    runtime.collect_android_result(device,"navigation","navigation.json",root)
                self.assertTrue((root/"navigation-screen.png").is_file())
                if not missing:self.assertIn("actual native failure",(root/"navigation.json").read_text())
    def test_capture_failure_fails_a_pass_but_preserves_the_original_native_error(self):
        for passed in (True,False):
            with tempfile.TemporaryDirectory() as directory:
                root=Path(directory)
                def device(*args):
                    return b"not a PNG" if args==("exec-out","screencap","-p") else json.dumps(dict(passed=passed,error="original native error")).encode()
                with self.assertRaisesRegex(RuntimeError,"Invalid native runtime screenshot" if passed else "original native error"):
                    runtime.collect_android_result(device,"navigation","navigation.json",root)
                self.assertTrue((root/"navigation-screenshot-error.txt").is_file())


if __name__ == "__main__":
    unittest.main()
