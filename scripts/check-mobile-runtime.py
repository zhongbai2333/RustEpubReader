#!/usr/bin/env python3
"""Run real native smoke checks on disposable mobile installations, without accounts."""
import argparse
import base64
from contextlib import contextmanager, nullcontext
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
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


def validate_ios_input_runs(output):
    modes = {mode: 0 for mode in ("none", "curl", "slide", "cover", "scroll")}
    paths = list(output.glob("selection-run-*.json"))
    if len(paths) != 15 or (output / "selection-input-copy-errors.json").exists():
        raise RuntimeError("Missing complete per-iteration native input evidence")
    identities, taps, repeated = set(), 0, 0
    for path in paths:
        match = re.fullmatch(r"selection-run-([0-9A-Fa-f-]{36})-(none|curl|slide|cover|scroll)\.json", path.name)
        if not match or path.is_symlink() or path.stat().st_size > 262144:
            raise RuntimeError("Invalid native input report path or size")
        try:
            report = json.loads(path.read_text())
            identity = str(uuid.UUID(match[1]))
        except ValueError as error:
            raise RuntimeError("Unreadable native input report") from error
        if not isinstance(report, dict):
            raise RuntimeError("Native input report is not an object")
        if identity in identities or report.get("run_id") != match[1] or report.get("mode") != match[2]:
            raise RuntimeError("Native input report identity differs or repeats")
        identities.add(identity); modes[match[2]] += 1
        focus, events, touches = report.get("focus", {}), report.get("events", []), report.get("touches", [])
        if (not isinstance(focus, dict) or any(focus.get(key) is not True for key in ("confirmed", "key_window", "foreground_scene")) or
            type(focus.get("received_taps")) is not int or not 1 <= focus["received_taps"] <= 3 or
            type(focus.get("ignored_probes")) is not int or focus["ignored_probes"] != 0):
            raise RuntimeError("Native input was not acknowledged or used fault injection")
        if (not isinstance(events, list) or not 3 <= len(events) <= 512 or any(not isinstance(e, dict) for e in events) or
            events[0].get("state") != 1 or events[-1].get("state") != 3 or
            any(type(e.get("state")) is not int or e["state"] not in (1, 2, 3) for e in events) or not any(e["state"] == 2 for e in events)):
            raise RuntimeError("Native selection recognizer did not complete")
        if (not isinstance(touches, list) or not 3 <= len(touches) <= 512 or any(not isinstance(e, dict) for e in touches) or
            touches[0].get("phase") != "began" or touches[-1].get("phase") != "ended" or
            any(e.get("phase") not in ("began", "moved", "ended") for e in touches) or not any(e["phase"] == "moved" for e in touches)):
            raise RuntimeError("Native touch stream did not complete")
        taps += focus["received_taps"]; repeated += focus["received_taps"] > 1
    if set(modes.values()) != {3}:
        raise RuntimeError("Native input evidence does not cover three iterations per mode")
    return dict(passed=True, runs=15, modes=modes, received_focus_taps=taps, repeated_readiness_handshakes=repeated)


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
    if case.get("UseUITargetAppProvidedByTests") is not True or case.get("UITargetAppPath") or case.get("UITargetAppBundleIdentifier"):
        raise ValueError("XCTest must control a provided app without an IDE launch target")
    target = app.resolve()
    dependency = "__TESTROOT__/" + str(target.relative_to(root)) if target.is_relative_to(root) else None
    if dependency is None or dependency not in case.get("DependentProductPaths", []) or not target.is_dir():
        raise ValueError("XCTest target differs from the admitted app")
    with (target / "Info.plist").open("rb") as stream:
        target_info = plistlib.load(stream)
    if target_info.get("CFBundleIdentifier") not in ("local.reader.columns", "local.reader.ui-files") or "iPhoneSimulator" not in target_info.get("CFBundleSupportedPlatforms", []):
        raise ValueError("Expected an isolated simulator target app")
    runner = product("TestHostPath")
    with (runner / "Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info.get("CFBundleIdentifier") != "local.reader.ui-files.tests.xctrunner" or "iPhoneSimulator" not in info.get("CFBundleSupportedPlatforms", []):
        raise ValueError("Expected an isolated simulator UI-test runner")
    if case.get("TestBundlePath") != "__TESTHOST__/PlugIns/ReaderUITests.xctest" or not (runner / "PlugIns/ReaderUITests.xctest").is_dir():
        raise ValueError("Missing native selection XCTest bundle")
    return runner


def ios_selection_manifest(args):
    """Pass admitted fixtures to the runner, which launches the installed app."""
    runner = ios_selection_inputs(args.app, args.xctestrun)
    with args.xctestrun.open("rb") as stream:
        values = plistlib.load(stream)
    case = values["ReaderUITests"]
    root = args.xctestrun.resolve().parent
    app_path = "__TESTROOT__/" + str(args.app.resolve().relative_to(root))
    runner_path = "__TESTROOT__/" + str(runner.relative_to(root))
    case["DependentProductPaths"] = [path for path in case.get("DependentProductPaths", [])
                                     if path != app_path and not path.startswith(app_path + "/")]
    if any(path != runner_path and not path.startswith(runner_path + "/") for path in case["DependentProductPaths"]):
        raise ValueError("Unsupported external XCTest runner dependency")
    # Xcode also searches the build-products directory when resolving a bundle
    # identifier. Give it only the runner, so it cannot select/install the reader
    # as a local build instead of using the already admitted installation.
    products = args.output.parent / (args.output.name + "-xctest-products")
    products.mkdir(exist_ok=False)
    staged_runner = products / runner.relative_to(root)
    staged_runner.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(runner, staged_runner, symlinks=True)
    if artifact_hash(staged_runner) != artifact_hash(runner):
        raise ValueError("Staged XCTest runner differs from the admitted runner")
    environment = case.setdefault("EnvironmentVariables", {})
    environment["READER_SELECTION_IGNORE_FOCUS_PROBES"] = "0"
    with (args.app / "Info.plist").open("rb") as stream:
        environment["READER_UI_TARGET_BUNDLE"] = plistlib.load(stream)["CFBundleIdentifier"]
    for key, fixture in (("READER_SELECTION_EPUB_BASE64", ROOT / "core/tests/fixtures/native-typography.epub"),
                         ("READER_NAVIGATION_EPUB_BASE64", ROOT / "ios/Tests/fixtures/native-reading-modes.epub")):
        environment[key] = base64.b64encode(fixture.read_bytes()).decode("ascii")
    def absolute_products(value):
        if isinstance(value, str):
            return value.replace("__TESTROOT__", str(products.resolve()))
        if isinstance(value, list):
            return [absolute_products(item) for item in value]
        if isinstance(value, dict):
            return {key: absolute_products(item) for key, item in value.items()}
        return value
    path = args.output / "selection-inputs.xctestrun"
    path.write_bytes(plistlib.dumps(absolute_products(values)))
    return path.resolve()


def ios_installed_target(args, identifier, bundle):
    paths = {kind: ios_simctl(args.output, "get_app_container", identifier, bundle, kind).decode().strip()
             for kind in ("app", "data")}
    installed = Path(paths["app"])
    with (installed / "Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    with (args.app / "Info.plist").open("rb") as stream:
        admitted = plistlib.load(stream)
    if info.get("CFBundleIdentifier") != bundle or admitted.get("CFBundleIdentifier") != bundle or info.get("CFBundleExecutable") != admitted.get("CFBundleExecutable"):
        raise ValueError("Installed app identity differs from the admitted target")
    binary = (installed / info["CFBundleExecutable"]).resolve()
    source = (args.app / info["CFBundleExecutable"]).resolve()
    if not binary.is_relative_to(installed.resolve()) or not source.is_relative_to(args.app.resolve()):
        raise ValueError("Installed executable escapes its app bundle")
    checksum = hashlib.sha256(binary.read_bytes()).hexdigest()
    if checksum != hashlib.sha256(source.read_bytes()).hexdigest():
        raise ValueError("Installed executable differs from the admitted app")
    return dict(paths, binary_sha256=checksum, binary_inode=binary.stat().st_ino)


def collect_ios_selection(args, identifier, bundle):
    # The collector installs once. A targetless runner controls that exact app
    # through XCUIApplication(bundleIdentifier:), without an IDE debug launch.
    manifest = ios_selection_manifest(args)
    before = ios_installed_target(args, identifier, bundle)
    command = ["xcodebuild", "test-without-building", "-xctestrun", str(manifest), "-destination", "id=" + identifier,
               "-parallel-testing-enabled", "NO", "-test-iterations", "3", "-only-testing:ReaderUITests/ReaderSelectionUITests", "-resultBundlePath", str(args.output / "selection.xcresult")]
    execution_failed = False
    try:
        with (args.output / "selection-ui.log").open("wb") as log:
            # Repeat all six cases three times, without retry-on-failure. Any
            # failed iteration fails the suite. Keep per-case assertions bounded
            # and allow cold runner setup and Settings restoration to complete.
            result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=max(1200, args.timeout))
        if result.returncode != 0:
            raise RuntimeError("Native selection XCTest failed; inspect selection-ui.log and XCResult")
    except BaseException:
        execution_failed = True
        raise
    finally:
        after, inspection_error = None, None
        try:
            after = ios_installed_target(args, identifier, bundle)
        except (OSError, subprocess.SubprocessError, RuntimeError, ValueError) as error:
            inspection_error = error
        (args.output / "xctest-installation.json").write_text(json.dumps(
            {"before": before, "after": after, "unchanged": after is not None and before == after,
             **({"inspection_error": str(inspection_error)} if inspection_error else {})}, indent=2) + "\n")
        # A crashed simulator can no longer answer simctl. Its admitted data
        # directory still contains the input traces needed to diagnose XCTest.
        container = Path((after or before)["data"])
        for pattern in ("selection-ui-*.json", "selection-gesture-*.json", "reduced-motion-verification.json"):
            for path in (container / "Library/Caches").glob(pattern):
                shutil.copyfile(path, args.output / path.name)
        # Keep each UUID, including failed iterations, rather than overwriting
        # their traces when the next repetition of the same mode completes.
        copy_errors = []
        for index, path in enumerate((container / "Library/Caches").glob("selection-run-*.json")):
            if index >= 32 or path.is_symlink() or not path.is_file() or path.stat().st_size > 262144:
                copy_errors.append(path.name)
                continue
            shutil.copyfile(path, args.output / path.name)
        if copy_errors:
            (args.output / "selection-input-copy-errors.json").write_text(json.dumps(copy_errors) + "\n")
        if inspection_error is not None and not execution_failed:
            raise inspection_error
    if before != after:
        raise RuntimeError("XCTest replaced the admitted target app or data container")
    rows = [validate_report((args.output / ("selection-ui-" + mode + ".json")).read_bytes()) for mode in ("none", "curl", "slide", "cover", "scroll")]
    selection = validate_selection({"passed": True, "cases": rows}, "ios")
    selection["input_readiness"] = validate_ios_input_runs(args.output)
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
        record(status="failed", error=str(error),
               stdout=(getattr(error, "stdout", None) or b"").decode(errors="replace")[-4000:],
               stderr=(getattr(error, "stderr", None) or b"").decode(errors="replace")[-4000:])
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


def prepare_ios_ui(output, identifier):
    """Exercise system UI launch services before admitting the test app."""
    bundle = "com.apple.Preferences"
    reply = ios_simctl(output, "launch", identifier, bundle).decode().strip()
    # A booted SpringBoard or an installed icon does not establish that an
    # application can start. Require a real system-app PID. The test app will
    # take the foreground normally; a separate terminate can itself stall
    # during cold initialization. Device cleanup owns the probe process too.
    if not re.fullmatch(re.escape(bundle) + r":\s*[1-9][0-9]*", reply):
        raise RuntimeError("iOS UI readiness probe did not return a process: " + repr(reply))


@contextmanager
def attach_ios_simulator(output, identifier):
    developer = os.environ.get("DEVELOPER_DIR") or run("xcode-select", "-p").decode().strip()
    simulator = Path(developer) / "Applications/Simulator.app"
    if not simulator.is_dir():
        raise ValueError("Selected Xcode Simulator application is missing: " + str(simulator))
    # Launch Services may wait for the selected iPad's first boot before it
    # acknowledges opening the window. Start it concurrently with bootstatus;
    # the existing boot budget, not a 30-second GUI request, owns first boot.
    command = ["open", "-a", str(simulator), "--args", "-CurrentDeviceUDID", identifier]
    with (output / "simulator-window.log").open("wb") as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
        try:
            yield
            code = process.wait(timeout=30)
            if code:
                raise RuntimeError("Simulator window request failed with exit code " + str(code))
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


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


def select_ios_runtime(runtimes, requested=None):
    available = [r for r in runtimes if r.get("isAvailable") and ".iOS-" in r["identifier"]]
    if requested is not None:
        if not re.fullmatch(r"[0-9]+(?:\.[0-9]+){1,2}", requested):
            raise ValueError("Expected an iOS runtime version such as 26.4")
        prefix = tuple(map(int, requested.split(".")))
        available = [r for r in available if tuple(map(int, r["version"].split(".")))[:len(prefix)] == prefix]
    if not available:
        raise ValueError("No available iOS Simulator runtime matches: " + str(requested))
    return max(available, key=lambda r: tuple(map(int, r["version"].split("."))))


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
        if original is not None and case in ("accessibility", "navigation"):
            # The instrumentation captures this before finishing its Activity;
            # the later screen can already be the launcher or TalkBack setup.
            try:
                png = device("exec-out", "run-as", ANDROID_PACKAGE, "cat", "cache/" + case + "-failure.png")
                if not png.startswith(b"\x89PNG\r\n\x1a\n"):
                    raise RuntimeError("Invalid " + case + " failure screenshot")
                (output / (case + "-failure.png")).write_bytes(png)
            except Exception as error:
                (output / (case + "-failure-screenshot-error.txt")).write_text(str(error) + "\n")
        try:
            png = device("exec-out", "screencap", "-p")
            if not png.startswith(b"\x89PNG\r\n\x1a\n"):
                raise RuntimeError("Invalid native runtime screenshot")
            (output / (case + "-screen.png")).write_bytes(png)
        except Exception as error:
            (output / (case + "-screenshot-error.txt")).write_text(str(error) + "\n")
            if original is None:
                raise


def validate_accessibility(value):
    expected = {"Realistic", "Slide", "Cover", "None"}
    cases = value.get("cases")
    if (value.get("touch_exploration") is not True or
        value.get("dont_suppress_accessibility_services") is not True or
        not isinstance(cases, list) or len(cases) != len(expected) or
        any(not isinstance(c, dict) for c in cases) or
        {c.get("mode") for c in cases} != expected or
        any(c.get(key) is not True for c in cases for key in ("native_text_focus", "custom_actions", "exact_return"))):
        raise RuntimeError("Incomplete TalkBack/native page-turn coverage")
    if next(c for c in cases if c["mode"] == "Cover").get("outgoing_focus_scroll_rejected") is not True:
        raise RuntimeError("Missing TalkBack outgoing-page focus regression")
    return value


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
                validate_accessibility(results["accessibility"])
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
    runtime = select_ios_runtime(runtimes, getattr(args,"ios_runtime_version",None))
    types = json.loads(run("xcrun", "simctl", "list", "devicetypes", "-j"))["devicetypes"]
    device_type = compatible_tablet(types, runtime["version"])
    identifier = ios_simctl(args.output, "create", "ReaderCI-" + str(uuid.uuid4()),
                            device_type, runtime["identifier"]).decode().strip()
    completed = False
    try:
        ios_simctl(args.output, "boot", identifier)
        # A newly created hosted iPad may still be migrating system data at
        # three minutes (with the Apple progress screen visible). Give only
        # first boot a separate bounded budget; app launches keep 180 seconds.
        window = attach_ios_simulator(args.output, identifier) if getattr(args, "show_simulator", False) else nullcontext()
        with window:
            ios_simctl(args.output, "bootstatus", identifier, "-b", timeout=600)
        prepare_ios_ui(args.output, identifier)
        results = {}
        ios_simctl(args.output, "install", identifier, str(args.app))
        if getattr(args, "with_selection", False):
            results["selection"] = collect_ios_selection(args, identifier, bundle)
        container = Path(ios_simctl(args.output, "get_app_container", identifier, bundle, "data").decode().strip())
        documents, caches = container / "Documents", container / "Library/Caches"
        documents.mkdir(parents=True, exist_ok=True)
        caches.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / "ios/Tests/fixtures/native-reading-modes.epub", documents / "reading-modes.epub")
        for index, (case, report) in enumerate(CASES.items()):
            path = caches / report
            path.unlink(missing_ok=True)
            flag = "--verify-single-column" if case == "single-column" else "--verify-navigation"
            try:
                # A freshly installed app has no instance to terminate. Only
                # subsequent cases replace a process that this collector started.
                options = () if index == 0 and "selection" not in results else ("--terminate-running-process",)
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
    parser.add_argument("--show-simulator", action="store_true", help="Attach the selected Xcode Simulator UI to this run's owned iPad")
    parser.add_argument("--ios-runtime-version", help="Require an installed iOS runtime version (major.minor or major.minor.patch)")
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.with_talkback and args.platform != "android":
        parser.error("--with-talkback is only supported for Android emulators")
    if args.with_book_opening and args.platform != "android":
        parser.error("--with-book-opening is only supported for Android emulators")
    if args.show_simulator and args.platform != "ios":
        parser.error("--show-simulator is only supported for iOS simulators")
    if args.ios_runtime_version and args.platform != "ios":
        parser.error("--ios-runtime-version is only supported for iOS simulators")
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
