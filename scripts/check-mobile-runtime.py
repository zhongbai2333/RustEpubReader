#!/usr/bin/env python3
"""Run real native smoke checks on disposable mobile installations, without accounts."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import shlex
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
ANDROID_PACKAGE = "local.reader.columns"
CASES = {
    "single-column": "single-column-verification.json",
    "navigation": "navigation-verification.json",
}

CORPUS_CASES = [
    {"name": "native-typography", "chapter": 0, "min_images": 2, "min_tables": 3, "min_notes": 1, "font_size": 32},
    {"name": "original-layout", "chapter": 0},
]


def validate_selection(report, platform):
    rows = report.get("cases", [])
    expected = {"None", "Realistic", "Slide", "Cover", "Scroll"} if platform == "android" else {"none", "curl", "slide", "cover", "scroll"}
    if report.get("passed") is not True or not isinstance(rows, list) or len(rows) != 5 or any(not isinstance(row, dict) for row in rows) or {row.get("mode") for row in rows} != expected:
        raise RuntimeError("Incomplete native selection modes")
    for row in rows:
        if row.get("persisted") is not True or row.get("text") != "文字 😀 中文" or type(row.get("start_scalar")) is not int or row["start_scalar"] != 0 or type(row.get("end_scalar")) is not int or row["end_scalar"] != 7:
            raise RuntimeError("Native selection excerpt/scalar persistence differs")
        if platform == "android":
            if row.get("native_drag") is not True or row.get("visible_highlight_action") is not True or row["mode"] == "Realistic" and row.get("quick_curl_swipe_exact_return") is not True:
                raise RuntimeError("Missing native drag/action or quick curl return")
        elif row.get("native_touch") is not True or row.get("start_block") != 1 or row.get("end_block") != 1:
            raise RuntimeError("Missing native touch or canonical selection block")
    return report


def validate_reduced_motion(report, platform):
    if platform == "android":
        validate_selection(report, platform)
        if report.get("system_animators_enabled") is not False:
            raise RuntimeError("Android system Reduce Motion policy was not exercised")
    else:
        rows = report.get("cases", [])
        if report.get("passed") is not True or report.get("system_reduce_motion") is not True or type(report.get("reduced_motion_geometry_checks")) is not int or report["reduced_motion_geometry_checks"] != 15:
            raise RuntimeError("iOS system Reduce Motion geometry checks missing")
        if not isinstance(rows,list) or len(rows) != 5 or any(not isinstance(row,dict) for row in rows) or {row.get("mode") for row in rows} != {"none", "curl", "slide", "cover", "scroll"} or any(any(row.get(key) is not True for key in ("cross_chapter", "same_chapter_offset", "precise_return", "reflow")) for row in rows):
            raise RuntimeError("iOS Reduce Motion navigation coverage incomplete")
        if report.get("automatic_remote_offer") is not True or report.get("no_local_echo") is not True:
            raise RuntimeError("iOS Reduce Motion remote progress checks missing")
        if report.get("quick_menu_teardown") is not True:
            raise RuntimeError("iOS reading menu teardown check missing")
    return report


def ios_selection_inputs(app, manifest):
    """Bind XCTest to the admitted app and an owned, complete simulator runner."""
    if manifest is None:
        raise ValueError("iOS selection requires --xctestrun")
    manifest = manifest.resolve()
    root = manifest.parent
    with manifest.open("rb") as stream:
        values = plistlib.load(stream)
    case = values.get("ReaderUITests", {})
    def product(key):
        path = case.get(key, "")
        if not isinstance(path, str) or not path.startswith("__TESTROOT__/"):
            raise ValueError("Expected an owned relative XCTest product")
        path = (root / path.removeprefix("__TESTROOT__/")).resolve()
        if not path.is_relative_to(root) or not path.is_dir():
            raise ValueError("XCTest product is missing or escapes its product root")
        return path
    target, runner = product("UITargetAppPath"), product("TestHostPath")
    if artifact_hash(target) != artifact_hash(app):
        raise ValueError("XCTest target differs from the admitted app")
    with (runner / "Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info.get("CFBundleIdentifier") != "local.reader.ui-files.tests.xctrunner" or "iPhoneSimulator" not in info.get("CFBundleSupportedPlatforms", []):
        raise ValueError("Expected an isolated simulator UI-test runner")
    if case.get("TestBundlePath") != "__TESTHOST__/PlugIns/ReaderUITests.xctest" or not (runner / "PlugIns/ReaderUITests.xctest").is_dir():
        raise ValueError("Missing native selection XCTest bundle")
    return runner


def collect_ios_selection(args, identifier, bundle):
    command = ["xcodebuild", "test-without-building", "-xctestrun", str(args.xctestrun.resolve()), "-destination", "id=" + identifier,
               "-parallel-testing-enabled", "NO", "-only-testing:ReaderUITests/ReaderSelectionUITests", "-resultBundlePath", str(args.output / "selection.xcresult")]
    try:
        with (args.output / "selection-ui.log").open("wb") as log:
            # Six XCTest cases include cold runner setup and restoring Settings.
            # Keep per-case assertions bounded; allow the suite to finish them
            # and write a complete XCResult instead of killing its cleanup.
            result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=max(600, args.timeout))
        if result.returncode != 0:
            raise RuntimeError("Native selection XCTest failed; inspect selection-ui.log and XCResult")
    finally:
        # XCTest can reinstall the same bundle and move its data container. A
        # pre-test path is not authoritative after that operation.
        container = Path(ios_simctl(args.output, "get_app_container", identifier, bundle, "data").decode().strip())
        for pattern in ("selection-ui-*.json", "reduced-motion-verification.json"):
            for path in (container / "Library/Caches").glob(pattern):
                shutil.copyfile(path, args.output / path.name)
    rows = [validate_report((args.output / ("selection-ui-" + mode + ".json")).read_bytes()) for mode in ("none", "curl", "slide", "cover", "scroll")]
    selection = validate_selection({"passed": True, "cases": rows}, "ios")
    selection["reduced_motion"] = validate_reduced_motion(validate_report((args.output / "reduced-motion-verification.json").read_bytes()), "ios")
    return selection


def validate_corpus(report, platform, cases=CORPUS_CASES):
    """Require every declared policy/width, not merely a nonempty passed report."""
    if platform == "android" and report.get("minimum_scalar_fragment_integrity") is not True:
        raise RuntimeError("Missing native scalar-fragment integrity check")
    layouts = report.get("layouts" if platform == "android" else "corpusLayouts", [])
    expected = {(c["name"], c["chapter"], policy, width): c for c in cases
                for policy in ("none", "mixed", "publisher") for width in (320, 640)}
    if not expected or not isinstance(layouts, list) or len(layouts) != len(expected):
        raise RuntimeError("Incomplete native corpus layouts")
    seen = set()
    for layout in layouts:
        if not isinstance(layout, dict):
            raise RuntimeError("Invalid native corpus layout")
        key = tuple(layout.get(k) for k in ("name", "chapter", "policy", "width"))
        if key not in expected or key in seen:
            raise RuntimeError("Unexpected/duplicate native corpus layout")
        seen.add(key)
        if layout.get("base_font_size") != expected[key].get("font_size", 18):
            raise RuntimeError("Wrong native corpus font size")
        pages, drawn = layout.get("pages"), layout.get("drawn_pages")
        if type(pages) is not int or pages <= 0 or not isinstance(drawn, list) or not drawn or any(type(i) is not int or i < 0 or i >= pages for i in drawn):
            raise RuntimeError("Invalid native drawing coverage")
        coverage = "canonical_text_coverage" if platform == "android" else "contiguousCoverage"
        if layout.get(coverage) is not True:
            raise RuntimeError("Missing native text coverage")
        names = layout.get("screenshot_names")
        allowed = {f"corpus-{key[0]}-{key[1]}-{key[3]}-{i}.png" for i in drawn}
        if not isinstance(names, list) or len(names) != len(set(names)) or any(name not in allowed for name in names) or (key[2] == "mixed" and not names):
            raise RuntimeError("Invalid/missing native corpus screenshots")
        for metric, minimum in (("images", "min_images"), ("tables", "min_tables"), ("notes_resolved", "min_notes")):
            if type(layout.get(metric)) is not int or layout[metric] < expected[key].get(minimum, 0):
                raise RuntimeError("Missing native corpus semantics: " + metric)
    return report


def artifact_hash(path):
    """Stable content identity shared by build records and runtime collection."""
    def digest(file):
        value = hashlib.sha256()
        with file.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                value.update(block)
        return value.hexdigest()
    if path.is_file():
        return digest(path)
    if not path.is_dir():
        raise ValueError(f"Missing application: {path}")
    value = hashlib.sha256()
    for entry in sorted(path.rglob("*")):
        if entry.is_file():
            value.update(entry.relative_to(path).as_posix().encode() + b"\0")
            value.update(digest(entry).encode() + b"\0")
    return value.hexdigest()


def run(*args, **kwargs):
    try:
        kwargs.setdefault("timeout", 180)
        return subprocess.run(args, check=True, capture_output=True, **kwargs).stdout
    except subprocess.CalledProcessError as error:
        detail = (error.stderr or b"").decode(errors="replace")[-4000:]
        raise RuntimeError(f"Command failed ({error.returncode}): {args}; {detail}") from error


def ios_simctl(output, *args, timeout=180):
    """Retain lifecycle diagnostics even when a freshly booted simulator hangs."""
    command = ["xcrun", "simctl", *args]
    started = time.monotonic()
    def record(**fields):
        with (output / "simulator-commands.jsonl").open("a") as log:
            log.write(json.dumps(dict(command=command, elapsed_seconds=round(time.monotonic() - started, 3), **fields)) + "\n")
    record(status="started", timeout_seconds=timeout)
    try:
        result = subprocess.run(command, check=True, capture_output=True, timeout=timeout)
    except (subprocess.SubprocessError, OSError) as error:
        record(status="failed", error=str(error), stderr=(getattr(error, "stderr", None) or b"").decode(errors="replace")[-4000:])
        raise
    record(status="completed", stdout=result.stdout.decode(errors="replace")[-4000:])
    return result.stdout


def cleanup_ios_simulator(output, identifier):
    errors = []
    for operation in ("shutdown", "delete"):
        try:
            ios_simctl(output, operation, identifier, timeout=45)
        except (OSError, subprocess.SubprocessError) as error:
            errors.append(str(error))
    return errors


def capture_ios_failure(output, identifier, bundle):
    """Collect bounded diagnostics before the owned simulator is deleted."""
    predicate = 'process == "SpringBoard" OR process == "runningboardd" OR eventMessage CONTAINS[c] "' + bundle + '"'
    commands = [
        ("simulator-processes.txt", ("spawn", identifier, "launchctl", "list")),
        ("simulator-recent.log", ("spawn", identifier, "log", "show", "--style", "compact", "--last", "3m", "--predicate", predicate)),
        ("simulator-failure-screen.log", ("io", identifier, "screenshot", "--type=png", str((output / "simulator-failure.png").resolve()))),
    ]
    for filename, command in commands:
        try:
            data = ios_simctl(output, *command, timeout=15)
            (output / filename).write_bytes(data)
        except (OSError, subprocess.SubprocessError) as error:
            (output / (filename + ".error.txt")).write_text(str(error) + "\n")


def compatible_tablet(types, version, requested=None):
    parts = [int(x) for x in version.split(".")]
    parts = (parts + [0, 0])[:3]
    encoded = (parts[0] << 16) | (parts[1] << 8) | parts[2]
    compatible = [device for device in types if device.get("productFamily") == "iPad"
                  and device.get("minRuntimeVersion", 0) <= encoded <= device.get("maxRuntimeVersion", 0)]
    if requested is not None:
        if any(device["identifier"] == requested for device in compatible):
            return requested
        raise ValueError("Requested iPad device type does not support the selected Simulator runtime: " + requested)
    # Keep the CI baseline independent of Xcode's newest-device-first order.
    for model in ("iPad-A16", "iPad-10th-generation", "iPad-Air-11-inch-M2"):
        for device in compatible:
            if device["identifier"] == "com.apple.CoreSimulator.SimDeviceType." + model:
                return device["identifier"]
    if compatible:
        return compatible[0]["identifier"]
    raise ValueError("No iPad device type supports the selected Simulator runtime")


def validate_report(data):
    value = json.loads(data)
    if not isinstance(value, dict) or value.get("passed") is not True:
        raise RuntimeError(f"Native check failed: {value}")
    return value


def wait_report(path, timeout):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if path.is_file():
            return validate_report(path.read_bytes())
        time.sleep(0.3)
    raise TimeoutError(f"Native check did not produce {path.name}")


@contextmanager
def reduced_motion_session(device, state):
    """Only an owned emulator may be changed; restore even partial activation."""
    if device("shell", "getprop", "ro.kernel.qemu").strip() != b"1":
        raise ValueError("Reduce Motion checks require an emulator")
    keys = ("window_animation_scale", "transition_animation_scale", "animator_duration_scale")
    before = {key: device("shell", "settings", "get", "global", key).decode().strip() for key in keys}
    state.update(before=before, restored=False)
    try:
        for key in keys:
            device("shell", "settings", "put", "global", key, "0")
        state["during"] = {key: device("shell", "settings", "get", "global", key).decode().strip() for key in keys}
        if any(value not in ("0", "0.0") for value in state["during"].values()):
            raise RuntimeError("Emulator did not disable animations")
        yield state
    finally:
        errors = []
        for key, value in before.items():
            try:
                if value == "null": device("shell", "settings", "delete", "global", key)
                else: device("shell", "settings", "put", "global", key, shlex.quote(value))
            except Exception as error:
                errors.append(str(error))
        state["after"] = {key: device("shell", "settings", "get", "global", key).decode().strip() for key in keys}
        state["restored"] = state["after"] == before and not errors
        if not state["restored"]:
            raise RuntimeError("Could not restore emulator motion settings: " + repr(errors))


@contextmanager
def talkback_session(device, state):
    """Restore secure settings even when activation or a native check fails."""
    if device("shell", "getprop", "ro.kernel.qemu").strip() != b"1":
        raise ValueError("TalkBack checks require an emulator")
    if not device("shell", "pm", "path", "com.google.android.marvin.talkback").strip().startswith(b"package:"):
        raise ValueError("The emulator image must provide TalkBack")
    keys = ["enabled_accessibility_services", "accessibility_enabled"]
    before = {key: device("shell", "settings", "get", "secure", key).decode().strip() for key in keys}
    state.update(before=before, restored=False)
    component = "com.google.android.marvin.talkback/.TalkBackService"
    services = [] if before[keys[0]] in ("null", "") else before[keys[0]].split(":")
    full_component = "com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService"
    was_present = component in services or full_component in services
    if not was_present:
        services.append(component)
    try:
        device("shell", "settings", "put", "secure", keys[0], shlex.quote(":".join(services)))
        device("shell", "settings", "put", "secure", keys[1], "1")
        yield state
    finally:
        errors = []
        for key, value in before.items():
            try:
                if value == "null": device("shell", "settings", "delete", "secure", key)
                else: device("shell", "settings", "put", "secure", key, shlex.quote(value))
            except Exception as error:
                errors.append(str(error))
        state["owned_service_stopped"] = False
        if not was_present:
            try:
                # Disabling the service can leave its newly started onboarding
                # permission activity above the next test. Close only the
                # TalkBack process this session activated; never stop an
                # initially configured service or decide a notification grant.
                device("shell", "am", "force-stop", "com.google.android.marvin.talkback")
                state["owned_service_stopped"] = True
            except Exception as error:
                errors.append(str(error))
        state["after"] = {key: device("shell", "settings", "get", "secure", key).decode().strip() for key in keys}
        state["restored"] = state["after"] == before and not errors
        if not state["restored"]:
            raise RuntimeError("Could not restore emulator accessibility settings: " + repr(errors))


def collect_android_result(device, case, report, output):
    """A native failure still retains its screen; capture errors cannot hide it."""
    original = None
    try:
        data = device("exec-out", "run-as", ANDROID_PACKAGE, "cat", "cache/" + report)
        (output / report).write_bytes(data)
        return validate_report(data)
    except Exception as error:
        original = error
        raise
    finally:
        if original is not None and case == "accessibility":
            # The instrumentation captures this before finishing its Activity;
            # the later screen can already be the launcher or TalkBack setup.
            try:
                png = device("exec-out", "run-as", ANDROID_PACKAGE, "cat", "cache/accessibility-failure.png")
                if not png.startswith(b"\x89PNG\r\n\x1a\n"):
                    raise RuntimeError("Invalid accessibility failure screenshot")
                (output / "accessibility-failure.png").write_bytes(png)
            except Exception as error:
                (output / "accessibility-failure-screenshot-error.txt").write_text(str(error) + "\n")
        try:
            png = device("exec-out", "screencap", "-p")
            if not png.startswith(b"\x89PNG\r\n\x1a\n"):
                raise RuntimeError("Invalid native runtime screenshot")
            (output / (case + "-screen.png")).write_bytes(png)
        except Exception as error:
            (output / (case + "-screenshot-error.txt")).write_text(str(error) + "\n")
            if original is None:
                raise


def collect_android_opening_screens(device, output, required):
    folder = output / "book-opening-runtime"
    folder.mkdir(exist_ok=True)
    for name in ("shelf", "before-back", "returned-shelf", "reopened", "failed-before-cleanup"):
        try:
            png = device("exec-out", "run-as", ANDROID_PACKAGE, "cat", "cache/book-opening-runtime/" + name + ".png")
            if not png.startswith(b"\x89PNG\r\n\x1a\n"):
                raise RuntimeError("Missing opening-cancellation screenshot: " + name)
            (folder / (name + ".png")).write_bytes(png)
        except Exception as error:
            (folder / (name + "-screenshot-error.txt")).write_text(str(error) + "\n")
            if required and name in ("shelf", "returned-shelf", "reopened"):
                raise


@contextmanager
def immersive_confirmation_session(device, state):
    """Use an onboarded emulator for input tests, then restore its exact setting."""
    if device("shell", "getprop", "ro.kernel.qemu").strip() != b"1":
        raise ValueError("Immersive tutorial setup requires an emulator")
    key = "immersive_mode_confirmations"
    def current():
        return device("shell", "settings", "get", "secure", key).decode().strip()
    before = state["before"] = current()
    try:
        if before != "confirmed":
            device("shell", "settings", "put", "secure", key, "confirmed")
        state["during"] = current()
        if state["during"] != "confirmed":
            raise RuntimeError("Could not prepare the emulator immersive tutorial state")
        yield
    finally:
        if before != "confirmed":
            if before == "null": device("shell", "settings", "delete", "secure", key)
            else: device("shell", "settings", "put", "secure", key, shlex.quote(before))
        state["after"] = current()
        state["restored"] = state["after"] == before
        if not state["restored"]:
            raise RuntimeError("Could not restore the emulator immersive tutorial state")


def android(args):
    sdk = Path(os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT", "")))
    adb = sdk / "platform-tools/adb"
    def device(*command):
        return run(str(adb), "-s", args.serial, *command)
    if not args.serial.startswith("emulator-"):
        raise ValueError("Runtime checks require an emulator, never a physical device")
    if device("shell", "getprop", "ro.kernel.qemu").strip() != b"1":
        raise ValueError("Target is not an Android emulator")
    aapts = sorted((sdk / "build-tools").glob("*/aapt"))
    if not aapts:
        raise ValueError("Android build-tools/aapt is required")
    for apk, expected in [(args.app, ANDROID_PACKAGE), (args.tests, ANDROID_PACKAGE + ".test")]:
        first = run(str(aapts[-1]), "dump", "badging", str(apk)).decode().splitlines()[0]
        if f"name='{expected}'" not in first:
            raise ValueError(f"Refusing APK with unexpected package: {first}")
        device("install", "-r", str(apk))
    state = {}
    try:
        with immersive_confirmation_session(device, state):
            results = android_checks(args, device)
        results["immersive_confirmation_settings"] = state
        return results
    finally:
        (args.output / "immersive-confirmation-settings.json").write_text(json.dumps(state, indent=2) + "\n")


def android_checks(args, device):
    fixture = ROOT / "ios/Tests/fixtures/native-reading-modes.epub"
    remote = "/data/local/tmp/reader-runtime.epub"
    device("push", str(fixture), remote)
    results = {}
    def sample(case, report, filename):
        device("shell", "am", "force-stop", ANDROID_PACKAGE)
        device("shell", "run-as", ANDROID_PACKAGE, "mkdir", "-p", "cache")
        if filename is not None:
            device("shell", "run-as", ANDROID_PACKAGE, "cp", remote, "cache/" + filename)
        device("shell", "run-as", ANDROID_PACKAGE, "rm", "-f", "cache/" + report)
        output = device("shell", "am", "instrument", "-w", "-e", "check", case,
                        ANDROID_PACKAGE + ".test/com.zhongbai233.epub.reader.ui.reader.IllustrationInstrumentation")
        (args.output / (case + ".log")).write_bytes(output)
        results[case] = collect_android_result(device, case, report, args.output)
    for case, report in CASES.items():
        sample(case, report, "columns.epub" if case == "single-column" else "navigation.epub")
    if getattr(args, "with_book_opening", False):
        # This directory belongs solely to the admitted diagnostic test. Results
        # from each execution are copied out before another execution can run.
        device("shell", "run-as", ANDROID_PACKAGE, "rm", "-rf", "cache/book-opening-runtime")
        (args.output / "book-opening-runtime").mkdir()
        opening_passed = False
        try:
            sample("book-opening", "book-opening-runtime/summary.json", "book-opening.epub")
            opening_passed = True
        finally:
            collect_android_opening_screens(device, args.output, required=opening_passed)
    if getattr(args, "with_corpus", False):
        device("shell", "run-as", ANDROID_PACKAGE, "mkdir", "-p", "cache/corpus")
        remote_corpus = "/data/local/tmp/reader-ci-corpus"
        cases_path = args.output / "corpus-cases.json"
        cases_path.write_text(json.dumps(CORPUS_CASES, indent=2) + "\n")
        try:
            for case in CORPUS_CASES:
                name = case["name"] + ".epub"
                device("push", str(ROOT / "core/tests/fixtures" / name), remote_corpus)
                device("shell", "run-as", ANDROID_PACKAGE, "cp", remote_corpus, "cache/corpus/" + name)
            device("push", str(cases_path), remote_corpus)
            device("shell", "run-as", ANDROID_PACKAGE, "cp", remote_corpus, "cache/corpus/cases.json")
            sample("corpus", "corpus-verification.json", None)
            validate_corpus(results["corpus"], "android")
            for layout in results["corpus"]["layouts"]:
                for name in layout["screenshot_names"]:
                    (args.output / name).write_bytes(device("exec-out", "run-as", ANDROID_PACKAGE, "cat", "cache/" + name))
        finally:
            device("shell", "rm", "-f", remote_corpus)
    if getattr(args, "with_selection", False):
        temporary = "/data/local/tmp/reader-ci-selection.epub"
        try:
            device("push", str(ROOT / "core/tests/fixtures/native-typography.epub"), temporary)
            device("shell", "run-as", ANDROID_PACKAGE, "cp", temporary, "cache/selection.epub")
            def selection_sample(reduced=False):
                key = "selection-reduced" if reduced else "selection"
                folder = key + "-runtime"
                device("shell", "run-as", ANDROID_PACKAGE, "rm", "-rf", "cache/" + folder)
                (args.output / folder).mkdir()
                sample(key, folder + "/summary.json", None)
                (validate_reduced_motion if reduced else validate_selection)(results[key], "android")
                for mode in ("None", "Realistic", "Slide", "Cover", "Scroll"):
                    name = "selection-" + mode + ".png"
                    png = device("exec-out", "run-as", ANDROID_PACKAGE, "cat", "cache/" + folder + "/" + name)
                    if not png.startswith(b"\x89PNG\r\n\x1a\n"):
                        raise RuntimeError("Missing native selection screenshot")
                    (args.output / folder / name).write_bytes(png)
            selection_sample()
            state = {}
            try:
                with reduced_motion_session(device, state):
                    selection_sample(reduced=True)
            finally:
                results["reduced_motion_settings"] = state
                (args.output / "reduced-motion-settings.json").write_text(json.dumps(state, indent=2) + "\n")
        finally:
            device("shell", "rm", "-f", temporary)
    if getattr(args, "with_talkback", False):
        state = {}
        try:
            with talkback_session(device, state):
                sample("accessibility", "accessibility-verification.json", "accessibility.epub")
                value = results["accessibility"]
                expected = {"Realistic", "Slide", "Cover", "None"}
                if value.get("touch_exploration") is not True or value.get("dont_suppress_accessibility_services") is not True or {c.get("mode") for c in value.get("cases", [])} != expected:
                    raise RuntimeError("Incomplete TalkBack/native page-turn coverage")
        finally:
            (args.output / "accessibility-settings.json").write_text(json.dumps(state, indent=2) + "\n")
    return results


def ios(args):
    with (args.app / "Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if "iPhoneSimulator" not in info.get("CFBundleSupportedPlatforms", []):
        raise ValueError("Expected an iOS Simulator application")
    bundle = info["CFBundleIdentifier"]
    if bundle not in ("local.reader.columns", "local.reader.ui-files"):
        raise ValueError("Runtime checks require an isolated iOS diagnostic bundle")
    if getattr(args, "with_selection", False):
        ios_selection_inputs(args.app, args.xctestrun)
    runtimes = json.loads(run("xcrun", "simctl", "list", "runtimes", "-j"))["runtimes"]
    available = [r for r in runtimes if r.get("isAvailable") and ".iOS-" in r["identifier"]]
    if not available:
        raise ValueError("No available iOS Simulator runtime")
    runtime = max(available, key=lambda r: tuple(map(int, r["version"].split("."))))
    types = json.loads(run("xcrun", "simctl", "list", "devicetypes", "-j"))["devicetypes"]
    device_type = compatible_tablet(types, runtime["version"])
    identifier = ios_simctl(args.output, "create", "ReaderCI-" + str(uuid.uuid4()),
                            device_type, runtime["identifier"]).decode().strip()
    completed = False
    try:
        ios_simctl(args.output, "boot", identifier)
        ios_simctl(args.output, "bootstatus", identifier, "-b")
        ios_simctl(args.output, "install", identifier, str(args.app))
        container = Path(ios_simctl(args.output, "get_app_container", identifier, bundle, "data").decode().strip())
        documents, caches = container / "Documents", container / "Library/Caches"
        documents.mkdir(parents=True, exist_ok=True)
        caches.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / "ios/Tests/fixtures/native-reading-modes.epub", documents / "reading-modes.epub")
        results = {}
        for index, (case, report) in enumerate(CASES.items()):
            path = caches / report
            path.unlink(missing_ok=True)
            flag = "--verify-single-column" if case == "single-column" else "--verify-navigation"
            try:
                # A freshly installed app has no instance to terminate. Only
                # subsequent cases replace a process that this collector started.
                options = () if index == 0 else ("--terminate-running-process",)
                ios_simctl(args.output, "launch", *options, identifier, bundle, "--verify-reading-modes", flag)
                results[case] = wait_report(path, args.timeout)
            finally:
                if path.is_file():
                    shutil.copyfile(path, args.output / report)
                for screenshot in caches.glob("*.png"):
                    shutil.copyfile(screenshot, args.output / screenshot.name)
        if getattr(args, "with_corpus", False):
            for case in CORPUS_CASES:
                name = case["name"] + ".epub"
                shutil.copyfile(ROOT / "core/tests/fixtures" / name, documents / name)
            (documents / "corpus-cases.json").write_text(json.dumps(CORPUS_CASES, indent=2))
            path = caches / "native-verification.json"
            path.unlink(missing_ok=True)
            try:
                ios_simctl(args.output, "launch", "--terminate-running-process", identifier, bundle, "--verify-native", "--book-share-preview")
                results["corpus"] = validate_corpus(wait_report(path, args.timeout), "ios")
            finally:
                if path.is_file(): shutil.copyfile(path, args.output / path.name)
                for screenshot in caches.glob("corpus-*.png"): shutil.copyfile(screenshot, args.output / screenshot.name)
        if getattr(args, "with_selection", False):
            shutil.copyfile(ROOT / "core/tests/fixtures/native-typography.epub", documents / "selection.epub")
            for mode in ("none", "curl", "slide", "cover", "scroll"):
                (caches / ("selection-ui-" + mode + ".json")).unlink(missing_ok=True)
            # Each XCTest calls XCUIApplication.launch() with its own arguments.
            results["selection"] = collect_ios_selection(args, identifier, bundle)
        completed = True
        return results
    except (OSError, subprocess.SubprocessError, RuntimeError, ValueError):
        capture_ios_failure(args.output, identifier, bundle)
        raise
    finally:
        cleanup_errors = cleanup_ios_simulator(args.output, identifier)
        if completed and cleanup_errors:
            raise RuntimeError("Could not clean up owned iOS simulator: " + repr(cleanup_errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=["android", "ios"])
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--tests", type=Path)
    parser.add_argument("--serial", default="emulator-5554")
    parser.add_argument("--with-corpus", action="store_true", help="Draw the committed typography/script corpus with native renderers")
    parser.add_argument("--with-talkback", action="store_true", help="Android emulator only; enable the installed TalkBack service temporarily")
    parser.add_argument("--with-book-opening", action="store_true", help="Android emulator only; native shelf input, cancellation and exact reopening")
    parser.add_argument("--with-selection", action="store_true", help="Native selection gestures, persisted scalar ranges, and real system Reduce Motion checks")
    parser.add_argument("--xctestrun", type=Path, help="Pinned ReaderUITests manifest, required for iOS selection")
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.with_talkback and args.platform != "android":
        parser.error("--with-talkback is only supported for Android emulators")
    if args.with_book_opening and args.platform != "android":
        parser.error("--with-book-opening is only supported for Android emulators")
    if args.platform == "android" and args.tests is None:
        parser.error("--tests is required for Android")
    ui_runner = ios_selection_inputs(args.app, args.xctestrun) if args.with_selection and args.platform == "ios" else None
    args.output.mkdir(parents=True, exist_ok=False)
    summary = {"platform": args.platform, "passed": False, "run_id": str(uuid.uuid4()),
               "collector_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
               "artifact_sha256": artifact_hash(args.app),
               "tests_sha256": artifact_hash(args.tests) if args.tests else None,
               "fixture_sha256": hashlib.sha256((ROOT / "ios/Tests/fixtures/native-reading-modes.epub").read_bytes()).hexdigest()}
    if args.with_selection:
        summary["selection_fixture_sha256"] = hashlib.sha256((ROOT / "core/tests/fixtures/native-typography.epub").read_bytes()).hexdigest()
    if ui_runner is not None:
        summary["ui_runner_sha256"] = artifact_hash(ui_runner)
        summary["xctestrun_sha256"] = hashlib.sha256(args.xctestrun.read_bytes()).hexdigest()
    if args.with_corpus:
        summary["corpus_fixtures"] = {case["name"]: hashlib.sha256((ROOT / "core/tests/fixtures" / (case["name"] + ".epub")).read_bytes()).hexdigest() for case in CORPUS_CASES}
    try:
        summary["checks"] = android(args) if args.platform == "android" else ios(args)
        summary["passed"] = True
    except (OSError, ValueError, RuntimeError, TimeoutError, subprocess.SubprocessError) as error:
        summary["error"] = str(error)
        raise
    finally:
        try:
            summary["artifacts_unchanged"] = artifact_hash(args.app) == summary["artifact_sha256"] and (args.tests is None or artifact_hash(args.tests) == summary["tests_sha256"])
            if ui_runner is not None:
                summary["artifacts_unchanged"] &= artifact_hash(ui_runner) == summary["ui_runner_sha256"] and hashlib.sha256(args.xctestrun.read_bytes()).hexdigest() == summary["xctestrun_sha256"]
        except (OSError, ValueError):
            summary["artifacts_unchanged"] = False
        if not summary["artifacts_unchanged"]:
            summary["passed"] = False
            summary["error"] = "Runtime input artifacts changed during collection"
        (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        if not summary["artifacts_unchanged"]:
            raise RuntimeError(summary["error"])


if __name__ == "__main__":
    main()
