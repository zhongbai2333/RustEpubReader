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
import sys
import unittest
import uuid
from unittest.mock import MagicMock, patch

SPEC = importlib.util.spec_from_file_location("runtime", Path(__file__).with_name("check-mobile-runtime.py"))
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)


class ReportTests(unittest.TestCase):
    def test_requested_ios_runtime_does_not_drift_to_another_installed_sdk(self):
        def item(version,available=True):
            return dict(identifier="com.apple.CoreSimulator.SimRuntime.iOS-"+version.replace(".","-"),version=version,isAvailable=available)
        runtimes=[item("26.2"),item("26.4.1"),item("26.4.2"),item("26.5"),item("26.40"),item("27.0",False)]
        self.assertEqual(runtime.select_ios_runtime(runtimes,"26.4")["version"],"26.4.2")
        self.assertEqual(runtime.select_ios_runtime(runtimes,"26.4.1")["version"],"26.4.1")
        for request in ("26.3","27.0","26","26.4x"):
            with self.subTest(request=request),self.assertRaises(ValueError):runtime.select_ios_runtime(runtimes,request)
        self.assertEqual(runtime.select_ios_runtime(runtimes)["version"],"26.40")

    def test_command_timeout_defaults_and_explicit_bounds_are_not_duplicated(self):
        with patch.object(runtime.subprocess,"run",return_value=SimpleNamespace(stdout=b"ok")) as process:
            self.assertEqual(runtime.run("owned-tool"),b"ok")
            self.assertEqual(process.call_args.kwargs["timeout"],180)
            self.assertEqual(runtime.run("owned-tool",timeout=10),b"ok")
            self.assertEqual(process.call_args.kwargs["timeout"],10)

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

    def test_ipad_baseline_is_stable_across_xcode_device_order(self):
        prefix="com.apple.CoreSimulator.SimDeviceType."
        def tablet(name,minimum=0):return dict(identifier=prefix+name,productFamily="iPad",minRuntimeVersion=minimum,maxRuntimeVersion=0xffffffff)
        devices=[tablet("iPad-Pro-13-inch-M5-16GB"),tablet("iPad-10th-generation"),tablet("iPad-A16")]
        for order in (devices,list(reversed(devices))):
            self.assertEqual(runtime.compatible_tablet(order,"26.2"),prefix+"iPad-A16")
        devices[-1]["minRuntimeVersion"]=28 << 16
        self.assertEqual(runtime.compatible_tablet(devices,"26.2"),prefix+"iPad-10th-generation")

    def test_explicit_performance_tablet_cannot_fall_back_to_another_viewport(self):
        def device(name, family="iPad", minimum=0):
            return dict(identifier=name,productFamily=family,minRuntimeVersion=minimum,maxRuntimeVersion=0xffffffff)
        devices=[device("baseline"),device("large-tablet"),device("phone","iPhone"),device("future-tablet",minimum=28<<16)]
        self.assertEqual(runtime.compatible_tablet(devices,"27.2","large-tablet"),"large-tablet")
        for requested in ("phone","future-tablet","absent"):
            with self.subTest(requested=requested),self.assertRaisesRegex(ValueError,"Requested iPad"):
                runtime.compatible_tablet(devices,"27.2",requested)

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
                if command == ("launch","owned-device","com.apple.Preferences"): return b"com.apple.Preferences: 123\n"
                if command[0] == "terminate": self.assertEqual(command,("terminate","owned-device","com.apple.Preferences"))
                return b""
            with patch.object(runtime, "run", side_effect=listing), patch.object(runtime, "ios_simctl", side_effect=device), patch.object(runtime, "wait_report", return_value={"passed": True}), patch.object(runtime,"attach_ios_simulator",side_effect=AssertionError("GUI was not requested")):
                result = runtime.ios(args)
            self.assertEqual(set(result), set(runtime.CASES))
            launches = [c for c in calls if c[0] == "launch" and "local.reader.columns" in c]
            self.assertEqual(len(launches), len(runtime.CASES))
            self.assertEqual(launches[0][1:3],("owned-device","local.reader.columns"))
            self.assertNotIn("--terminate-running-process",launches[0])
            self.assertTrue(all(c[1:4] == ("--terminate-running-process", "owned-device", "local.reader.columns") for c in launches[1:]))
            self.assertLess(calls.index(("launch","owned-device","com.apple.Preferences")), next(i for i,c in enumerate(calls) if c[0]=="install"))
            self.assertFalse(any(c[0]=="terminate" for c in calls))
            self.assertEqual(calls[-2:], [("shutdown", "owned-device"), ("delete", "owned-device")])

    def test_missing_requested_runtime_fails_before_creating_a_device(self):
        with tempfile.TemporaryDirectory() as directory:
            args,listing=self.fixture(Path(directory));args.ios_runtime_version="26.4"
            with patch.object(runtime,"run",side_effect=listing),patch.object(runtime,"ios_simctl") as device:
                with self.assertRaisesRegex(ValueError,"runtime matches"):runtime.ios(args)
            device.assert_not_called()

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
            error = subprocess.TimeoutExpired(["xcrun", "simctl", "launch"], 180,
                                              output=b"Waiting on system startup", stderr=b"simulator unavailable")
            with patch.object(runtime.subprocess, "run", side_effect=error):
                with self.assertRaises(subprocess.TimeoutExpired):
                    runtime.ios_simctl(output, "launch", "owned-device", "local.reader.columns")
            rows = [json.loads(line) for line in (output / "simulator-commands.jsonl").read_text().splitlines()]
            self.assertEqual([r["status"] for r in rows], ["started", "failed"])
            self.assertEqual(rows[-1]["stderr"], "simulator unavailable")
            self.assertEqual(rows[-1]["stdout"], "Waiting on system startup")

    def test_first_boot_budget_does_not_extend_reader_launches(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);args,listing=self.fixture(root);calls=[]
            def device(output,*command,**kwargs):
                calls.append((command,kwargs))
                if command[0]=="create":return b"owned-device\n"
                if command[0]=="get_app_container":return str(root/"container").encode()
                if command==("launch","owned-device","com.apple.Preferences"):return b"com.apple.Preferences: 123\n"
                return b""
            with patch.object(runtime,"run",side_effect=listing),patch.object(runtime,"ios_simctl",side_effect=device),patch.object(runtime,"wait_report",return_value={"passed":True}):
                runtime.ios(args)
            self.assertEqual([kw for cmd,kw in calls if cmd[0]=="bootstatus"],[{"timeout":600}])
            reader_launches=[kw for cmd,kw in calls if cmd[0]=="launch" and "local.reader.columns" in cmd]
            self.assertEqual(len(reader_launches),len(runtime.CASES))
            self.assertTrue(all("timeout" not in kw for kw in reader_launches))

    def test_first_boot_timeout_is_not_retried_or_admitted(self):
        with tempfile.TemporaryDirectory() as directory:
            args,listing=self.fixture(Path(directory));calls=[]
            def device(output,*command,**kwargs):
                calls.append(command)
                if command[0]=="create":return b"owned-device\n"
                if command[0]=="bootstatus":raise subprocess.TimeoutExpired(command,600,output=b"Data migration")
                self.assertNotIn(command[0],("install","launch","get_app_container"))
                return b"boot diagnostics"
            with patch.object(runtime,"run",side_effect=listing),patch.object(runtime,"ios_simctl",side_effect=device):
                with self.assertRaises(subprocess.TimeoutExpired):runtime.ios(args)
            self.assertEqual(len([cmd for cmd in calls if cmd[0]=="bootstatus"]),1)
            self.assertEqual(calls[-2:],[("shutdown","owned-device"),("delete","owned-device")])
            self.assertTrue((args.output/"simulator-processes.txt").is_file())

    def test_launch_timeout_retains_native_report_and_diagnostics_without_passing_or_retrying(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);args,listing=self.fixture(root);calls=[]
            cache=root/"container/Library/Caches";cache.mkdir(parents=True)
            def device(output,*command,**kwargs):
                calls.append(command)
                if command[0]=="create":return b"owned-device\n"
                if command[0]=="get_app_container":return str(root/"container").encode()
                if command==("launch","owned-device","com.apple.Preferences"):return b"com.apple.Preferences: 123\n"
                if command[0]=="launch":
                    (cache/"single-column-verification.json").write_text('{"passed":true}')
                    raise subprocess.TimeoutExpired(command,180)
                if command[0] in ("spawn","io"):
                    self.assertEqual(kwargs["timeout"],15)
                    return b"owned simulator diagnostics"
                return b""
            with patch.object(runtime,"run",side_effect=listing),patch.object(runtime,"ios_simctl",side_effect=device):
                with self.assertRaises(subprocess.TimeoutExpired):runtime.ios(args)
            self.assertEqual(len([c for c in calls if c[0]=="launch" and "local.reader.columns" in c]),1)
            self.assertTrue((args.output/"single-column-verification.json").is_file())
            self.assertTrue((args.output/"simulator-processes.txt").is_file())
            self.assertTrue((args.output/"simulator-recent.log").is_file())
            self.assertEqual(calls[-2:],[("shutdown","owned-device"),("delete","owned-device")])

    def test_ui_readiness_requires_a_system_app_process_without_separate_termination(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)
            with patch.object(runtime,"ios_simctl",return_value=b"com.apple.Preferences: 123\n") as device:
                runtime.prepare_ios_ui(output,"owned-device")
            self.assertEqual(device.call_args_list[0].args,(output,"launch","owned-device","com.apple.Preferences"))
            self.assertEqual(device.call_count,1)
            for reply in (b"",b"com.apple.Preferences: 0",b"other.app: 123",b"com.apple.Preferences: -1"):
                with self.subTest(reply=reply),patch.object(runtime,"ios_simctl",return_value=reply) as device:
                    with self.assertRaisesRegex(RuntimeError,"readiness probe"):runtime.prepare_ios_ui(output,"owned-device")
                    self.assertEqual(device.call_count,1)

    def test_ui_readiness_timeout_stops_before_install_and_retains_failure_diagnostics(self):
        with tempfile.TemporaryDirectory() as directory:
            args,listing=self.fixture(Path(directory));calls=[]
            def device(output,*command,**kwargs):
                calls.append(command)
                if command[0]=="create":return b"owned-device\n"
                if command==("launch","owned-device","com.apple.Preferences"):
                    raise subprocess.TimeoutExpired(command,180)
                self.assertNotIn(command[0],("install","get_app_container"))
                return b"owned environment diagnostics"
            with patch.object(runtime,"run",side_effect=listing),patch.object(runtime,"ios_simctl",side_effect=device):
                with self.assertRaises(subprocess.TimeoutExpired):runtime.ios(args)
            self.assertEqual(len([c for c in calls if c[0]=="launch"]),1)
            self.assertTrue((args.output/"simulator-processes.txt").is_file())
            self.assertEqual(calls[-2:],[("shutdown","owned-device"),("delete","owned-device")])

    def test_requested_gui_targets_the_owned_device_in_the_selected_xcode(self):
        with tempfile.TemporaryDirectory() as directory:
            developer=Path(directory);app=developer/"Applications/Simulator.app";app.mkdir(parents=True)
            process=MagicMock();process.wait.return_value=0;process.poll.return_value=0
            with patch.dict(runtime.os.environ,{"DEVELOPER_DIR":str(developer)}),patch.object(runtime.subprocess,"Popen",return_value=process) as command:
                with runtime.attach_ios_simulator(developer,"owned-device"):
                    process.wait.assert_not_called()  # Cold boot has not finished.
            self.assertEqual(command.call_args.args[0],["open","-a",str(app),"--args","-CurrentDeviceUDID","owned-device"])
            process.wait.assert_called_once_with(timeout=30)
            self.assertTrue((developer/"simulator-window.log").is_file())
            process.terminate.assert_not_called()

    def test_real_window_request_can_wait_for_boot_without_blocking_boot(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/"Applications/Simulator.app").mkdir(parents=True)
            ready=root/"boot-ready";actual_popen=subprocess.Popen;children=[]
            script="import pathlib,sys,time; p=pathlib.Path(sys.argv[1]); deadline=time.monotonic()+3\nwhile not p.exists() and time.monotonic()<deadline: time.sleep(.01)\nsys.exit(0 if p.exists() else 2)"
            def launch(command,**kwargs):
                child=actual_popen([sys.executable,"-c",script,str(ready)],**kwargs);children.append(child);return child
            with patch.dict(runtime.os.environ,{"DEVELOPER_DIR":str(root)}),patch.object(runtime.subprocess,"Popen",side_effect=launch):
                with runtime.attach_ios_simulator(root,"owned-device"):
                    self.assertIsNone(children[0].poll())
                    ready.write_text("system boot completed")
            self.assertEqual(children[0].returncode,0)

    def test_gui_request_failure_cannot_be_admitted_after_boot(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/"Applications/Simulator.app").mkdir(parents=True)
            process=MagicMock();process.wait.return_value=2;process.poll.return_value=2
            with patch.dict(runtime.os.environ,{"DEVELOPER_DIR":str(root)}),patch.object(runtime.subprocess,"Popen",return_value=process):
                with self.assertRaisesRegex(RuntimeError,"window request failed"):
                    with runtime.attach_ios_simulator(root,"owned-device"):
                        pass

    def test_gui_acknowledgement_timeout_still_fails_and_reaps_only_its_request(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/"Applications/Simulator.app").mkdir(parents=True)
            process=MagicMock();process.poll.return_value=None
            process.wait.side_effect=[subprocess.TimeoutExpired("open",30),0]
            with patch.dict(runtime.os.environ,{"DEVELOPER_DIR":str(root)}),patch.object(runtime.subprocess,"Popen",return_value=process):
                with self.assertRaises(subprocess.TimeoutExpired):
                    with runtime.attach_ios_simulator(root,"owned-device"):
                        pass
            process.terminate.assert_called_once_with()
            self.assertEqual([c.kwargs for c in process.wait.call_args_list],[{"timeout":30},{"timeout":5}])

    def test_boot_failure_reaps_pending_gui_request_without_waiting_for_acknowledgement(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/"Applications/Simulator.app").mkdir(parents=True)
            process=MagicMock();process.poll.return_value=None
            process.wait.side_effect=[subprocess.TimeoutExpired("open",5),0]
            with patch.dict(runtime.os.environ,{"DEVELOPER_DIR":str(root)}),patch.object(runtime.subprocess,"Popen",return_value=process):
                with self.assertRaisesRegex(RuntimeError,"boot failed"):
                    with runtime.attach_ios_simulator(root,"owned-device"):
                        raise RuntimeError("boot failed")
            process.terminate.assert_called_once_with();process.kill.assert_called_once_with()
            self.assertEqual([c.kwargs for c in process.wait.call_args_list],[{"timeout":5},{"timeout":5}])

    def test_gui_attachment_is_opt_in_and_precedes_the_ui_readiness_probe(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);args,listing=self.fixture(root);args.show_simulator=True;calls=[]
            def device(output,*command,**kwargs):
                calls.append(command)
                if command[0]=="create":return b"owned-device\n"
                if command[0]=="get_app_container":return str(root/"container").encode()
                if command==("launch","owned-device","com.apple.Preferences"):return b"com.apple.Preferences: 123\n"
                return b""
            developer=root/"xcode";(developer/"Applications/Simulator.app").mkdir(parents=True)
            process=MagicMock();process.poll.return_value=0
            process.wait.side_effect=lambda **kwargs:calls.append(("gui-ready","owned-device")) or 0
            def start(*args,**kwargs):
                calls.append(("gui","owned-device"));return process
            with patch.dict(runtime.os.environ,{"DEVELOPER_DIR":str(developer)}),patch.object(runtime,"run",side_effect=listing),patch.object(runtime,"ios_simctl",side_effect=device),patch.object(runtime,"wait_report",return_value={"passed":True}),patch.object(runtime.subprocess,"Popen",side_effect=start):
                runtime.ios(args)
            self.assertLess(calls.index(("boot","owned-device")),calls.index(("gui","owned-device")))
            self.assertLess(calls.index(("gui","owned-device")),calls.index(("bootstatus","owned-device","-b")))
            self.assertLess(calls.index(("bootstatus","owned-device","-b")),calls.index(("gui-ready","owned-device")))
            self.assertLess(calls.index(("gui-ready","owned-device")),calls.index(("launch","owned-device","com.apple.Preferences")))


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


class ImmersiveConfirmationTests(unittest.TestCase):
    class Device:
        def __init__(self, before="null", emulator=True, reject_setup=False):
            self.value=before;self.emulator=emulator;self.mutations=0;self.reject_setup=reject_setup
        def __call__(self,*args):
            if args[:3]==("shell","getprop","ro.kernel.qemu"):return b"1" if self.emulator else b"0"
            if args[:4]==("shell","settings","get","secure"):return self.value.encode()
            self.mutations+=1
            if args[2]=="delete":self.value="null"
            elif not (self.reject_setup and args[-1]=="confirmed"):self.value=shlex.split(args[-1])[0]
            return b""

    def test_first_run_and_preexisting_settings_are_restored_exactly(self):
        for before in ("null","","confirmed","prior;setting"):
            with self.subTest(before=before):
                device=self.Device(before);state={}
                with runtime.immersive_confirmation_session(device,state):self.assertEqual(device.value,"confirmed")
                self.assertEqual(device.value,before);self.assertTrue(state["restored"])
                if before=="confirmed":self.assertEqual(device.mutations,0)

    def test_failed_native_case_still_restores_the_first_run_state(self):
        device=self.Device();state={}
        with self.assertRaisesRegex(RuntimeError,"actual native failure"):
            with runtime.immersive_confirmation_session(device,state):raise RuntimeError("actual native failure")
        self.assertEqual(device.value,"null");self.assertTrue(state["restored"])

    def test_refuses_physical_device_and_failed_setup_cannot_run_the_case(self):
        device=self.Device(emulator=False)
        with self.assertRaisesRegex(ValueError,"emulator"):
            with runtime.immersive_confirmation_session(device,{}):self.fail("Physical device admitted")
        self.assertEqual(device.mutations,0)
        device=self.Device(reject_setup=True)
        with self.assertRaisesRegex(RuntimeError,"Could not prepare"):
            with runtime.immersive_confirmation_session(device,{}):self.fail("Unprepared device admitted")
        self.assertEqual(device.value,"null")


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


class AccessibilityCoverageTests(unittest.TestCase):
    @staticmethod
    def report():
        return dict(touch_exploration=True,dont_suppress_accessibility_services=True,cases=[dict(mode=mode,native_text_focus=True,custom_actions=True,exact_return=True,outgoing_focus_scroll_rejected=True) for mode in ("Realistic","Slide","Cover","None")])

    def test_requires_native_turns_and_outgoing_focus_regression(self):
        good=self.report();self.assertIs(runtime.validate_accessibility(good),good)
        for value in (None,False):
            bad=self.report();bad['cases'][2]['outgoing_focus_scroll_rejected']=value
            with self.assertRaisesRegex(RuntimeError,"outgoing-page"):runtime.validate_accessibility(bad)

    def test_rejects_duplicate_missing_and_failed_native_cases(self):
        for change in (lambda v:v['cases'].pop(),lambda v:v['cases'].__setitem__(0,v['cases'][1]),lambda v:v['cases'][0].update(exact_return=False),lambda v:v['cases'][1].update(native_text_focus=False),lambda v:v.update(touch_exploration=False),lambda v:v.update(cases=[None]*4)):
            bad=self.report();change(bad)
            with self.assertRaises(RuntimeError):runtime.validate_accessibility(bad)


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
        (app/"Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier":"local.reader.columns","CFBundleExecutable":"ReaderPreview","CFBundleSupportedPlatforms":["iPhoneSimulator"]}))
        (app/'ReaderPreview').write_bytes(b'admitted native executable')
        (runner/"Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier":"local.reader.ui-files.tests.xctrunner","CFBundleSupportedPlatforms":["iPhoneSimulator"]}))
        manifest=root/"ReaderUITests.xctestrun"
        manifest.write_bytes(plistlib.dumps({"ReaderUITests":{"UseUITargetAppProvidedByTests":True,"DependentProductPaths":["__TESTROOT__/Debug-iphonesimulator/ReaderPreview.app","__TESTROOT__/Debug-iphonesimulator/ReaderUITests-Runner.app","__TESTROOT__/Debug-iphonesimulator/ReaderUITests-Runner.app/PlugIns/ReaderUITests.xctest"],"TestHostPath":"__TESTROOT__/Debug-iphonesimulator/ReaderUITests-Runner.app","TestBundlePath":"__TESTHOST__/PlugIns/ReaderUITests.xctest"}}))
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

    def test_rejects_ide_target_binding_and_unowned_app(self):
        for key,value in (('UseUITargetAppProvidedByTests',False),('UITargetAppPath','__TESTROOT__/Debug-iphonesimulator/ReaderPreview.app')):
            with tempfile.TemporaryDirectory() as directory:
                app,runner,manifest=self.products(Path(directory));v=plistlib.loads(manifest.read_bytes());v['ReaderUITests'][key]=value;manifest.write_bytes(plistlib.dumps(v))
                with self.assertRaisesRegex(ValueError,'IDE launch target'):runtime.ios_selection_inputs(app,manifest)
        with tempfile.TemporaryDirectory() as directory:
            app,runner,manifest=self.products(Path(directory));v=plistlib.loads((app/'Info.plist').read_bytes());v['CFBundleIdentifier']='com.user.real.app';(app/'Info.plist').write_bytes(plistlib.dumps(v))
            with self.assertRaisesRegex(ValueError,'isolated simulator target'):runtime.ios_selection_inputs(app,manifest)

    def test_selection_manifest_seeds_fixture_without_mutating_or_installing_build_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);app,runner,manifest=self.products(root)
            output=root/'reports';output.mkdir();original=manifest.read_bytes()
            before=(runtime.artifact_hash(app),runtime.artifact_hash(runner))
            with patch.object(runtime,'ios_simctl') as simctl:
                path=runtime.ios_selection_manifest(SimpleNamespace(app=app,xctestrun=manifest,output=output))
            simctl.assert_not_called()
            case=plistlib.loads(path.read_bytes())['ReaderUITests']
            import base64
            self.assertEqual(base64.b64decode(case['EnvironmentVariables']['READER_SELECTION_EPUB_BASE64']),(runtime.ROOT/'core/tests/fixtures/native-typography.epub').read_bytes())
            self.assertEqual(base64.b64decode(case['EnvironmentVariables']['READER_NAVIGATION_EPUB_BASE64']),(runtime.ROOT/'ios/Tests/fixtures/native-reading-modes.epub').read_bytes())
            self.assertEqual(case['EnvironmentVariables']['READER_UI_TARGET_BUNDLE'],'local.reader.columns')
            self.assertEqual(case['EnvironmentVariables']['READER_SELECTION_IGNORE_FOCUS_PROBES'],'0')
            self.assertNotIn('UITargetAppPath',case)
            staged=root/'reports-xctest-products/Debug-iphonesimulator/ReaderUITests-Runner.app'
            self.assertEqual(case['TestHostPath'],str(staged.resolve()))
            self.assertEqual(runtime.artifact_hash(staged),runtime.artifact_hash(runner))
            self.assertFalse((staged.parent/'ReaderPreview.app').exists())
            self.assertEqual(case['DependentProductPaths'],[str(staged.resolve()),str(staged.resolve()/'PlugIns/ReaderUITests.xctest')])
            self.assertEqual(case['TestBundlePath'],'__TESTHOST__/PlugIns/ReaderUITests.xctest')
            self.assertEqual(manifest.read_bytes(),original)
            self.assertEqual(before,(runtime.artifact_hash(app),runtime.artifact_hash(runner)))

    def test_selection_results_require_the_same_admitted_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);output=root/"output";output.mkdir()
            current=root/"new-container";cache=current/"Library/Caches";cache.mkdir(parents=True)
            ran=False
            def execute(*args,**kwargs):
                nonlocal ran
                ran=True
                self.assertGreaterEqual(kwargs["timeout"],1200,"Allow three complete suites including cold XCTest setup and cleanup")
                command=args[0]
                self.assertEqual(command[command.index('-xctestrun')+1],str(root/'selection-inputs.xctestrun'))
                self.assertEqual(command[command.index("-test-iterations")+1],"3")
                self.assertNotIn("-retry-tests-on-failure",command)
                self.assertNotIn("-run-tests-until-failure",command)
                (cache/"reduced-motion-verification.json").write_text(json.dumps(ReducedMotionTests.ios_report()))
                (cache/"selection-gesture-slide.json").write_text('{"events":[{"state":1},{"state":3}]}')
                for row in self.report("ios")["cases"]:
                    (cache/("selection-ui-"+row["mode"]+".json")).write_text(json.dumps(dict(row,passed=True)))
                IOSInputEvidenceTests.populate(cache)
                return SimpleNamespace(returncode=0)
            state={'app':'/owned.app','data':str(current),'binary_sha256':'admitted','binary_inode':1}
            args=SimpleNamespace(output=output,xctestrun=root/"owned.xctestrun",timeout=1)
            with patch.object(runtime.subprocess,"run",side_effect=execute),patch.object(runtime,'ios_installed_target',return_value=state),patch.object(runtime,'ios_selection_manifest',return_value=root/'selection-inputs.xctestrun'):
                value=runtime.collect_ios_selection(args,"owned-device","local.reader.columns")
            self.assertTrue(value["passed"]);self.assertEqual(len(list(output.glob("selection-ui-*.json"))),5)
            self.assertEqual(json.loads((output/"selection-gesture-slide.json").read_text())["events"],[{"state":1},{"state":3}])
            self.assertTrue(json.loads((output/'xctest-installation.json').read_text())['unchanged'])
            self.assertEqual(value['input_readiness']['runs'],15)

    def test_installed_binary_must_match_admitted_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);app,runner,manifest=self.products(root);installed=root/'installed.app'
            import shutil
            shutil.copytree(app,installed)
            args=SimpleNamespace(app=app,output=root)
            def lookup(output,*args):return str(installed if args[-1]=='app' else root/'data').encode()
            with patch.object(runtime,'ios_simctl',side_effect=lookup):
                before=runtime.ios_installed_target(args,'owned-device','local.reader.columns')
                self.assertEqual(before['binary_sha256'],runtime.hashlib.sha256((app/'ReaderPreview').read_bytes()).hexdigest())
                (installed/'ReaderPreview').write_bytes(b'wrong executable')
                with self.assertRaisesRegex(ValueError,'executable differs'):runtime.ios_installed_target(args,'owned-device','local.reader.columns')

    def test_reinstalled_target_cannot_pass_even_with_successful_xctest(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);args=SimpleNamespace(output=root,xctestrun=root/'source',timeout=1)
            before={'app':'/owned.app','data':str(root),'binary_sha256':'same','binary_inode':1}
            after=dict(before,binary_inode=2)
            with patch.object(runtime,'ios_installed_target',side_effect=[before,after]),patch.object(runtime,'ios_selection_manifest',return_value=root/'selection-inputs.xctestrun'),patch.object(runtime.subprocess,'run',return_value=SimpleNamespace(returncode=0)):
                with self.assertRaisesRegex(RuntimeError,'replaced the admitted target'):runtime.collect_ios_selection(args,'owned-device','local.reader.columns')
            self.assertFalse(json.loads((root/'xctest-installation.json').read_text())['unchanged'])

    def test_invalid_selection_inputs_do_not_start_xctest(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);args=SimpleNamespace(output=root,xctestrun=root/"owned.xctestrun",timeout=1)
            with patch.object(runtime,'ios_selection_manifest',side_effect=ValueError('wrong admitted app')),patch.object(runtime.subprocess,"run") as execute:
                with self.assertRaisesRegex(ValueError,'wrong admitted app'):runtime.collect_ios_selection(args,"owned-device","local.reader.columns")
            execute.assert_not_called()

    def test_target_is_installed_once_before_external_ui_and_failure_stops_cli(self):
        for failed in (False,True):
            with tempfile.TemporaryDirectory() as directory:
                root=Path(directory);app,runner,manifest=self.products(root)
                (app/'Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'local.reader.columns','CFBundleSupportedPlatforms':['iPhoneSimulator']}))
                commands=[];selected=False
                def simctl(output,*args,**kwargs):
                    commands.append(args)
                    if args[0]=='create':return b'owned-device'
                    if args[0]=='get_app_container':
                        self.assertTrue(selected);return str(root/'container').encode()
                    if args[0]=='launch':
                        self.assertTrue(selected);self.assertEqual(args[1],'--terminate-running-process')
                    if args[0]=='install':self.assertFalse(selected)
                    return b''
                def selection(*args):
                    nonlocal selected
                    self.assertEqual(len([c for c in commands if c[0]=='install']),1)
                    self.assertFalse(any(c[0] in ('launch','get_app_container') for c in commands))
                    selected=True
                    if failed:raise RuntimeError('native XCTest failed')
                    return {'passed':True}
                args=SimpleNamespace(app=app,xctestrun=manifest,output=root/'reports',with_selection=True,with_corpus=False,timeout=1)
                with patch.object(runtime,'run',side_effect=[b'{"runtimes":[]}',b'{"devicetypes":[]}']),patch.object(runtime,'select_ios_runtime',return_value={'identifier':'owned.runtime','version':'26.4.1'}),patch.object(runtime,'compatible_tablet',return_value='owned.ipad'),patch.object(runtime,'ios_simctl',side_effect=simctl),patch.object(runtime,'prepare_ios_ui'),patch.object(runtime,'collect_ios_selection',side_effect=selection),patch.object(runtime,'wait_report',return_value={'passed':True}),patch.object(runtime,'capture_ios_failure'),patch.object(runtime,'cleanup_ios_simulator',return_value=[]):
                    if failed:
                        with self.assertRaisesRegex(RuntimeError,'native XCTest failed'):runtime.ios(args)
                        self.assertFalse(any(c[0] in ('launch','get_app_container') for c in commands))
                    else:
                        self.assertTrue(runtime.ios(args)['selection']['passed'])


class ReducedMotionTests(unittest.TestCase):
    @staticmethod
    def ios_report():
        return dict(passed=True,system_reduce_motion=True,reduced_motion_geometry_checks=15,automatic_remote_offer=True,no_local_echo=True,quick_menu_teardown=True,cases=[dict(mode=mode,cross_chapter=True,same_chapter_offset=True,precise_return=True,reflow=True) for mode in ("none","curl","slide","cover","scroll")])
    def test_real_system_policy_and_full_coverage_are_required(self):
        good=self.ios_report();self.assertEqual(runtime.validate_reduced_motion(good,"ios"),good)
        for key,value in (("system_reduce_motion",False),("reduced_motion_geometry_checks",0),("reduced_motion_geometry_checks",True),("cases",[]),("cases",[None]*5),("no_local_echo",False),("quick_menu_teardown",False),("quick_menu_teardown",None)):
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
    def test_opening_failure_retains_the_screen_before_activity_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            def device(*args):
                if args[-1].endswith("failed-before-cleanup.png"):return b"\x89PNG\r\n\x1a\nsystem tutorial"
                raise RuntimeError("case stopped before this screenshot")
            runtime.collect_android_opening_screens(device,root,required=False)
            self.assertTrue((root/"book-opening-runtime/failed-before-cleanup.png").is_file())
            self.assertTrue((root/"book-opening-runtime/shelf-screenshot-error.txt").is_file())

    def test_passed_opening_case_still_requires_all_three_proof_screens(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError,"Missing opening-cancellation screenshot: shelf"):
                runtime.collect_android_opening_screens(lambda *args:b"not a PNG",Path(directory),required=True)

    def test_failed_xctest_cannot_be_replaced_by_a_passed_native_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);output=root/"output";output.mkdir()
            cache=root/"container/Library/Caches";cache.mkdir(parents=True)
            (cache/"reduced-motion-verification.json").write_text('{"passed":true}')
            (cache/"selection-gesture-slide.json").write_text('{"events":[]}')
            failed_run=cache/f"selection-run-{str(uuid.uuid4()).upper()}-cover.json"
            failed_run.write_text('{"events":[],"focus":{"confirmed":false}}')
            args=SimpleNamespace(output=output,xctestrun=root/"owned.xctestrun",timeout=1)
            with patch.object(runtime.subprocess,"run",return_value=SimpleNamespace(returncode=1)),patch.object(runtime,'ios_installed_target',return_value={'data':str(root/'container'),'app':'/owned.app'}),patch.object(runtime,'ios_selection_manifest',return_value=root/'selection-inputs.xctestrun'):
                with self.assertRaisesRegex(RuntimeError,"Native selection XCTest failed"):
                    runtime.collect_ios_selection(args,"owned-device","local.reader.columns")
            self.assertTrue((output/"reduced-motion-verification.json").is_file())
            self.assertTrue((output/"selection-gesture-slide.json").is_file())
            self.assertTrue((output/failed_run.name).is_file())

    def test_shutdown_after_xctest_preserves_the_primary_failure_and_admitted_traces(self):
        for returncode in (0,1):
            with self.subTest(returncode=returncode),tempfile.TemporaryDirectory() as directory:
                root=Path(directory);output=root/"output";output.mkdir()
                cache=root/"container/Library/Caches";cache.mkdir(parents=True)
                (cache/"selection-gesture-cover.json").write_text('{"events":[],"touches":[]}')
                args=SimpleNamespace(output=output,xctestrun=root/"owned.xctestrun",timeout=1)
                shutdown=RuntimeError("Simulator is Shutdown")
                installed={'data':str(root/'container'),'app':'/owned.app'}
                with patch.object(runtime.subprocess,"run",return_value=SimpleNamespace(returncode=returncode)),patch.object(runtime,'ios_installed_target',side_effect=[installed,shutdown]),patch.object(runtime,'ios_selection_manifest',return_value=root/'selection-inputs.xctestrun'):
                    expected="Native selection XCTest failed" if returncode else "Simulator is Shutdown"
                    with self.assertRaisesRegex(RuntimeError,expected):runtime.collect_ios_selection(args,"owned-device","local.reader.columns")
                admission=json.loads((output/"xctest-installation.json").read_text())
                self.assertFalse(admission["unchanged"]);self.assertIsNone(admission["after"])
                self.assertEqual(admission["inspection_error"],"Simulator is Shutdown")
                self.assertTrue((output/"selection-gesture-cover.json").is_file())

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

    def test_accessibility_failure_preserves_the_screen_before_activity_cleanup(self):
        for valid in (True, False):
            with self.subTest(valid=valid), tempfile.TemporaryDirectory() as directory:
                root=Path(directory)
                def device(*args):
                    if args[-1]=="cache/accessibility-failure.png":
                        return b"\x89PNG\r\n\x1a\nbefore cleanup" if valid else b"missing image"
                    if args==("exec-out","screencap","-p"):
                        return b"\x89PNG\r\n\x1a\nafter cleanup"
                    return b'{"passed":false,"error":"original accessibility failure"}'
                with self.assertRaisesRegex(RuntimeError,"original accessibility failure"):
                    runtime.collect_android_result(device,"accessibility","accessibility.json",root)
                if valid:
                    self.assertIn(b"before cleanup",(root/"accessibility-failure.png").read_bytes())
                else:
                    self.assertTrue((root/"accessibility-failure-screenshot-error.txt").is_file())
                self.assertIn(b"after cleanup",(root/"accessibility-screen.png").read_bytes())

    def test_accessibility_success_never_collects_a_stale_failure_image(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            def device(*args):
                if args==("exec-out","screencap","-p"):return b"\x89PNG\r\n\x1a\npassed screen"
                self.assertNotEqual(args[-1],"cache/accessibility-failure.png")
                return b'{"passed":true}'
            self.assertTrue(runtime.collect_android_result(device,"accessibility","accessibility.json",root)["passed"])
            self.assertFalse((root/"accessibility-failure.png").exists())

    def test_navigation_failure_collects_its_own_screen_without_hiding_the_error(self):
        for valid in (True,False):
            with self.subTest(valid=valid),tempfile.TemporaryDirectory() as directory:
                root=Path(directory)
                def device(*args):
                    if args[-1]=="cache/navigation-failure.png":return b"\x89PNG\r\n\x1a\nreader before cleanup" if valid else b"missing image"
                    if args==("exec-out","screencap","-p"):return b"\x89PNG\r\n\x1a\nlauncher"
                    return b'{"passed":false,"error":"restart did not restore position"}'
                with self.assertRaisesRegex(RuntimeError,"restart did not restore position"):
                    runtime.collect_android_result(device,"navigation","navigation.json",root)
                self.assertEqual((root/"navigation-failure.png").exists(),valid)
                self.assertEqual((root/"navigation-failure-screenshot-error.txt").exists(),not valid)

    def test_navigation_success_never_collects_a_stale_failure_image(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            def device(*args):
                if args==("exec-out","screencap","-p"):return b"\x89PNG\r\n\x1a\nreader"
                self.assertNotEqual(args[-1],"cache/navigation-failure.png")
                return b'{"passed":true}'
            self.assertTrue(runtime.collect_android_result(device,"navigation","navigation.json",root)["passed"])
            self.assertFalse((root/"navigation-failure.png").exists())


class IOSInputEvidenceTests(unittest.TestCase):
    @staticmethod
    def populate(output):
        paths=[]
        for mode in ("none","curl","slide","cover","scroll"):
            for _ in range(3):
                identity=str(uuid.uuid4()).upper();path=output/f"selection-run-{identity}-{mode}.json"
                report={"mode":mode,"run_id":identity,
                    "focus":{"confirmed":True,"received_taps":1,"ignored_probes":0,"key_window":True,"foreground_scene":True},
                    "events":[{"state":1},{"state":2},{"state":3}],
                    "touches":[{"phase":"began"},{"phase":"moved"},{"phase":"ended"}]}
                path.write_text(json.dumps(report));paths.append(path)
        return paths

    def test_all_three_iterations_require_actual_input_and_recognizer_completion(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);paths=self.populate(root)
            for path,count in zip(paths,(2,3)):
                report=json.loads(path.read_text());report["focus"]["received_taps"]=count;path.write_text(json.dumps(report))
            result=runtime.validate_ios_input_runs(root)
            self.assertEqual(result["runs"],15);self.assertEqual(result["received_focus_taps"],18)
            self.assertEqual(result["repeated_readiness_handshakes"],2)
            self.assertEqual(set(result["modes"].values()),{3})

    def test_missing_iteration_or_mode_cannot_be_hidden_by_the_latest_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);paths=self.populate(root);paths[0].unlink()
            with self.assertRaisesRegex(RuntimeError,"per-iteration"):runtime.validate_ios_input_runs(root)

    def test_unacknowledged_window_fault_injection_and_unbounded_probe_counts_fail(self):
        mutations=[("confirmed",False),("key_window",False),("foreground_scene",False),("ignored_probes",1),
                   ("received_taps",0),("received_taps",4),("received_taps",True)]
        for key,value in mutations:
            with self.subTest(key=key,value=value),tempfile.TemporaryDirectory() as directory:
                root=Path(directory);path=self.populate(root)[0];report=json.loads(path.read_text());report["focus"][key]=value;path.write_text(json.dumps(report))
                with self.assertRaisesRegex(RuntimeError,"not acknowledged"):runtime.validate_ios_input_runs(root)

    def test_one_bad_iteration_is_not_replaced_by_other_completed_gestures(self):
        for field,value in [("events",[{"state":1}]),("events",[{"state":1},{"state":2},{"state":4}]),
                            ("touches",[{"phase":"began"},{"phase":"moved"},{"phase":"cancelled"}]),("touches",[])]:
            with self.subTest(field=field,value=value),tempfile.TemporaryDirectory() as directory:
                root=Path(directory);path=self.populate(root)[0];report=json.loads(path.read_text());report[field]=value;path.write_text(json.dumps(report))
                with self.assertRaisesRegex(RuntimeError,"did not complete"):runtime.validate_ios_input_runs(root)

    def test_duplicate_run_identity_across_modes_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);paths=self.populate(root);first=json.loads(paths[0].read_text());other=json.loads(paths[3].read_text())
            other["run_id"]=first["run_id"];paths[3].unlink();(root/f"selection-run-{first['run_id']}-{other['mode']}.json").write_text(json.dumps(other))
            with self.assertRaisesRegex(RuntimeError,"identity"):runtime.validate_ios_input_runs(root)

    def test_fifteen_reports_must_still_have_three_of_each_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);path=self.populate(root)[0];report=json.loads(path.read_text());path.unlink();report["mode"]="curl"
            (root/f"selection-run-{report['run_id']}-curl.json").write_text(json.dumps(report))
            with self.assertRaisesRegex(RuntimeError,"three iterations"):runtime.validate_ios_input_runs(root)

    def test_report_payload_identity_and_mode_must_match_its_filename(self):
        for key,value in (("mode","curl"),("run_id",str(uuid.uuid4()))):
            with self.subTest(key=key),tempfile.TemporaryDirectory() as directory:
                root=Path(directory);path=self.populate(root)[0];report=json.loads(path.read_text());report[key]=value;path.write_text(json.dumps(report))
                with self.assertRaisesRegex(RuntimeError,"identity"):runtime.validate_ios_input_runs(root)

    def test_symlink_oversized_and_unreadable_reports_fail_closed(self):
        for kind in ("symlink","oversized","array","broken"):
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as directory:
                root=Path(directory)/"reports";root.mkdir();path=self.populate(root)[0]
                if kind=="symlink":
                    external=root.parent/"outside.json";path.rename(external);path.symlink_to(external)
                else:path.write_text("x"*262145 if kind=="oversized" else "[]" if kind=="array" else "{")
                with self.assertRaises(RuntimeError):runtime.validate_ios_input_runs(root)

    def test_copy_errors_cannot_be_ignored_even_when_fifteen_reports_exist(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);self.populate(root);(root/"selection-input-copy-errors.json").write_text('["unreadable"]')
            with self.assertRaisesRegex(RuntimeError,"per-iteration"):runtime.validate_ios_input_runs(root)


if __name__ == "__main__":
    unittest.main()
