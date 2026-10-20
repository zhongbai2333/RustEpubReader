#!/usr/bin/env python3
"""Collect opt-in native Release reader reports on isolated desktop/mobile data."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import statistics
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "local.reader.performance"
SCHEMA = 3
HOST_IDLE_MINIMUM = 80
HOST_IDLE_CONSECUTIVE = 10
HOST_IDLE_TIMEOUT = 300


def load_module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


core = load_module("core_performance", "check-performance.py")
mobile = load_module("mobile_runtime", "check-mobile-runtime.py")


def run(*args, **kwargs):
    return mobile.run(*map(str, args), **kwargs)


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def artifact_hash(path):
    return mobile.artifact_hash(path)


def parse_host_idle(output):
    # top's first CPU line is cumulative; its second line covers the measured
    # one-second interval. Historical load averages cannot establish quietness.
    values = re.findall(r"CPU usage:[^\n]*,\s*([+-]?[0-9.]+)% idle", output)
    if len(values) < 2 or not math.isfinite(value := float(values[-1])) or not 0 <= value <= 100:
        raise ValueError("Missing or invalid measured host CPU idle percentage")
    return value


def wait_host_idle(observations, timeout=HOST_IDLE_TIMEOUT, minimum_idle=HOST_IDLE_MINIMUM, consecutive=HOST_IDLE_CONSECUTIVE):
    deadline = time.monotonic() + timeout
    ready = 0
    while time.monotonic() < deadline:
        output = run("top", "-l", "2", "-s", "1", "-n", "0", env=dict(os.environ, LC_ALL="C"), timeout=10).decode()
        idle = parse_host_idle(output)
        observations.append({"time": time.time(), "idle_percent": idle})
        ready = ready + 1 if idle >= minimum_idle else 0
        if ready >= consecutive:
            return
    raise RuntimeError("Host did not become quiet before performance sampling; keep this attempt separate from timing baselines")


def collector_sources(entrypoint):
    paths = {Path(__file__), Path(core.__file__), Path(mobile.__file__), entrypoint}
    return {str(path.resolve().relative_to(ROOT)): digest(path) for path in sorted(paths)}

def validate_build(build, current, app, pinned=False):
    source = build.get("source", {})
    entries = source.get("files")
    if (not isinstance(entries, dict) or not entries or
            any(not isinstance(name, str) or not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value) for name, value in entries.items()) or
            source.get("sha256") != hashlib.sha256(json.dumps(entries, sort_keys=True).encode()).hexdigest()):
        raise ValueError("Invalid build source manifest")
    if (build.get("schema") != SCHEMA or build.get("passed") is not True
            or (not pinned and source != current) or build.get("artifact_sha256") != artifact_hash(app)):
        raise ValueError("Build record does not match source and artifact; rebuild or use an immutable pinned build")


def harness_sources(build, client, scenario):
    if scenario == "dense-shelf":
        paths = {"desktop": ["desktop/src/shelf_performance.rs"],
                 "android": ["android/app/src/performance/java/com/epub/reader/ui/reader/ShelfPerformanceActivity.kt", "android/app/src/performance/java/com/epub/reader/ui/reader/ShelfPerformanceHooks.kt"],
                 "ios": ["ios/Tests/ReadingReleasePerformance.swift"]}[client]
    else:
        paths = {"desktop": ["desktop/src/performance.rs"],
                 "android": ["android/app/src/performance/java/com/epub/reader/ui/reader/ReaderPerformanceActivity.kt"],
                 "ios": ["ios/Tests/ReadingReleasePerformance.swift"]}[client]
    try:
        return {path: build["source"]["files"][path] for path in paths}
    except KeyError as error:
        raise ValueError("Build is missing its native harness source fingerprint") from error


def sources():
    # Include dirty and new source, not ignored build products or runtime evidence.
    paths = run("git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", cwd=ROOT).split(b"\0")
    entries = {os.fsdecode(p): digest(ROOT / os.fsdecode(p)) for p in paths if p and (ROOT / os.fsdecode(p)).is_file()}
    value = hashlib.sha256(json.dumps(entries, sort_keys=True).encode()).hexdigest()
    return {"sha256": value, "files": entries}


def record_build(args):
    if not args.command:
        raise ValueError("A build command is required after --command")
    args.output.mkdir(parents=True, exist_ok=False)
    before = sources()
    report = {"schema": SCHEMA, "passed": False, "source": before, "command": args.command}
    try:
        with (args.output / "build.log").open("wb") as log:
            subprocess.run(args.command, cwd=ROOT, check=True, stdout=log, stderr=subprocess.STDOUT)
        if before != sources():
            raise ValueError("Source changed during build; rebuild from a stable source tree")
        report.update(artifact_sha256=artifact_hash(args.app), passed=True)
    finally:
        (args.output / "build.json").write_text(json.dumps(report, indent=2) + "\n")


def checked_number(value, name, positive=False):
    if (not isinstance(value, (int, float)) or isinstance(value, bool)
            or not math.isfinite(value) or value < 0 or (positive and value == 0)):
        raise ValueError(f"Invalid or unavailable native metric: {name}")
    return value


def targets(count):
    if not isinstance(count, int) or isinstance(count, bool) or count < 1:
        raise ValueError("Invalid chapter count")
    return sorted({min(3, count - 1), count // 2, count - 1})


def walk_count(count):
    return min(200, count - min(3, count - 1))


def normalize(value, client, book, run_id, scenario="reader"):
    if value.get("passed") is not True or value.get("profile") != "release" or value.get("harness_schema") != SCHEMA:
        raise ValueError(f"Failed, old or non-Release native harness: {value.get('error', '')}")
    if value.get("run_id") != run_id or value.get("book_sha256") != book["sha256"] or value.get("chapter_count") != book["chapters"]:
        raise ValueError("Wrong run, book fingerprint or chapter count")
    if not isinstance(value.get("viewport"), dict) or not value["viewport"]:
        raise ValueError("Missing viewport configuration")
    if scenario not in ["reader", "visible-endurance"] or value.get("scenario", "reader") != scenario:
        raise ValueError("Wrong native measurement scenario")
    endurance = scenario == "visible-endurance"
    chapters = [0] if endurance else targets(book["chapters"])
    metrics = {}
    def add(key, number):
        metrics[key] = checked_number(number, key, positive=True)
    def memory(prefix, numbers):
        for name, number in numbers.items():
            if number is not None:
                add(prefix + "_" + name, number)
    if client == "desktop":
        stats = value["cache_stats"]
        resident, loads = stats[:2]
        if value["desktop_resident_chapters"] > min(32, book["chapters"]):
            raise ValueError("Desktop chapter cache grew unbounded")
        add("first_content_visible_ms", value["first_content_visible_ms"])
        for name in ["baseline_rss_bytes", "first_content_rss_bytes", "after_walk_rss_bytes", "peak_rss_bytes"]:
            if value.get(name) is not None:
                add(name, value[name])
        cases = value["cases"]
        first, frame_prefix, count_name = "chapter_first_visible_ms", "frame_callback", "frame_samples"
    else:
        resident, loads = value["cache_stats"]["resident"], value["cache_stats"]["body_loads"]
        for stage in ["baseline", "first_content", "after_walk"]:
            memory(stage, value[stage + "_memory"])
        if client == "ios":
            if value["initial_core_cache"]["body_loads"] != 0 or value["swift_resident_bodies"] > min(32, book["chapters"]):
                raise ValueError("iOS eagerly loaded or retained too many chapter bodies")
            add("first_content_layout_draw_ms", value["first_content_layout_draw_ms"])
            add("import_or_existing_entry_ms", value["import_or_existing_entry_ms"])
            add("metadata_open_ms", value["metadata_open_ms"])
            cases = value["cases"]
            first, frame_prefix, count_name = "chapter_first_layout_draw_ms", "display_interval", "display_samples"
        else:
            add("first_content_draw_ms", value["first_content_draw_ms"])
            add("core_and_first_body_ms", value["core_and_first_body_ms"])
            cases = value["layouts"]
            first, frame_prefix, count_name = "first_layout_draw_ms", "frame_duration", "frame_count"
            scroll = value["scroll_cases"]
            if len(scroll) != (0 if endurance else len(chapters)) or {s["chapter"] for s in scroll} != (set() if endurance else set(chapters)):
                raise ValueError("Incomplete scroll scenarios")
            for item in scroll:
                if item["extra_body_loads"] > 8 or item["resident"] > 8:
                    raise ValueError("Scroll eagerly loaded chapter bodies")
                add(f'scroll/chapter-{item["chapter"]}/ready_ms', item["ready_ms"])
    checked_number(resident, "resident", positive=True)
    checked_number(loads, "body_loads", positive=True)
    if not 1 <= resident <= min(8, book["chapters"]) or loads < walk_count(book["chapters"]) or value.get("walk_chapters") != walk_count(book["chapters"]):
        raise ValueError("Chapter walk did not run or native core cache grew unbounded")
    modes = ["curl", "slide"] if client == "ios" else ["Realistic", "Slide"]
    expected = {(mode, index) for mode in modes for index in chapters}
    seen = set()
    for case in cases:
        key = (case["mode"], case["chapter"])
        if key in seen:
            raise ValueError("Duplicate native scenario")
        seen.add(key)
        prefix = f'{case["mode"]}/chapter-{case["chapter"]}/'
        if checked_number(case[count_name], count_name) < 8:
            raise ValueError("Too few native frame samples")
        add(prefix + first, case[first])
        for percentile in ["p50", "p95", "max"]:
            name = f"{frame_prefix}_{percentile}_ms"
            add(prefix + name, case[name])
        if not case[f"{frame_prefix}_p50_ms"] <= case[f"{frame_prefix}_p95_ms"] <= case[f"{frame_prefix}_max_ms"]:
            raise ValueError("Invalid frame quantiles")
        turns = case["turn_total_ms"]
        if len(turns) != (128 if endurance else 8):
            raise ValueError("Incomplete native page turns")
        for number in turns:
            checked_number(number, "turn_total_ms", positive=True)
        add(prefix + "turn_median_ms", statistics.median(turns))
        add(prefix + "turn_max_ms", max(turns))
        if endurance:
            if client == "android" and not 1 <= checked_number(case.get("styled_resident"), "styled_resident", positive=True) <= 8:
                raise ValueError("Styled Android chapter cache grew unbounded")
            curve = case.get("turn_memory_samples", [])
            if case.get("returned_to_origin") is not True or [p.get("turns") for p in curve] != list(range(16, 129, 16)):
                raise ValueError("Incomplete visible-turn memory curve or failed return")
            counters = {}
            for point in curve:
                values = {"rss_bytes": point.get("rss_bytes")} if client == "desktop" else point["memory"]
                if not values or all(value is None for value in values.values()):
                    raise ValueError("No visible-turn memory measurement")
                for name, number in values.items():
                    if number is not None:
                        counters.setdefault(name, []).append(checked_number(number, name, positive=True))
            for name, numbers in counters.items():
                if len(numbers) != 8:
                    raise ValueError("Visible-turn memory counter coverage changed")
                add(prefix + "visible_turn_sampled_max_" + name, max(numbers))
                add(prefix + "visible_turn_last_" + name, numbers[-1])
        if client == "desktop":
            for name in ["update_cpu_p95_ms", "update_cpu_max_ms"]:
                add(prefix + name, case[name])
    if seen != expected or len(cases) != len(expected):
        raise ValueError("Incomplete native mode/chapter coverage")
    walk = walk_count(book["chapters"])
    expected_points = list(range(25, walk, 25)) + [walk]
    curve = value.get("walk_memory_samples", [])
    if [point.get("chapters") for point in curve] != expected_points:
        raise ValueError("Incomplete chapter-walk memory curve")
    curve_values = {}
    for point in curve:
        counters = ({"rss_bytes": point.get("rss_bytes")} if client == "desktop" else point["memory"])
        for name, number in counters.items():
            if number is not None:
                curve_values.setdefault(name, []).append(checked_number(number, name, positive=True))
    for name, numbers in curve_values.items():
        add("walk_sampled_max_" + name, max(numbers))
    samples = value.get("steady_memory_samples", [])
    if len(samples) != 5:
        raise ValueError("Missing post-walk memory samples")
    names = set(samples[0])
    if any(set(sample) != names for sample in samples):
        raise ValueError("Inconsistent memory sampling coverage")
    for name in names:
        numbers = [checked_number(sample[name], name, positive=True) for sample in samples]
        add("steady_" + name, statistics.median(numbers))
    return metrics


def wait_json(path, timeout, process=None):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if path.is_file():
            try:
                return json.loads(path.read_bytes())
            except (json.JSONDecodeError, UnicodeDecodeError):
                pass  # A writer may still be publishing the report.
        if process is not None and process.poll() is not None:
            raise RuntimeError("Desktop process exited before producing its report")
        time.sleep(0.2)
    raise TimeoutError("Native reader performance report timed out")


class Desktop:
    def __init__(self, args):
        if platform.system() != "Darwin":
            raise ValueError("This runner currently validates native macOS desktop only")
        self.args = args
        # A local bundle gives the eframe process a regular macOS application window.
        app = args.output / "ReaderPerformance.app"
        self.exe = app / "Contents/MacOS/ReaderPerformance"
        self.exe.parent.mkdir(parents=True)
        shutil.copy2(args.app, self.exe)
        with (app / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump({"CFBundleIdentifier": PACKAGE + ".desktop", "CFBundleName": "ReaderPerformance",
                          "CFBundleExecutable": "ReaderPerformance", "CFBundlePackageType": "APPL",
                          "NSHighResolutionCapable": True}, stream)
        run("codesign", "--force", "--sign", "-", app)
        self.environment = {"host_system": platform.system(), "host_os": platform.release(),
                            "host_architecture": platform.machine(), "cpu": run("sysctl", "-n", "machdep.cpu.brand_string").decode().strip(),
                            "toolchain": run("rustc", "--version").decode().strip(),
                            "display": run("system_profiler", "SPDisplaysDataType", "-json").decode()}

    def sample(self, pair, mode, run_id):
        data = self.args.output / f"data-{pair}"
        data.mkdir(exist_ok=True)
        # Each pair has its own book/index. Reopen preserves only that pair's data.
        source = data / "book.epub"
        if mode == "cold":
            shutil.copyfile(self.args.book, source)
        report = self.args.output / f"{run_id}.json"
        env = dict(os.environ, READER_PERFORMANCE_REPORT=str(report), READER_PERFORMANCE_RUN=run_id,
                   READER_PERFORMANCE_ENDURANCE="1" if self.args.scenario == "visible-endurance" else "0")
        with (self.args.output / f"{run_id}.log").open("wb") as log:
            process = subprocess.Popen([str(self.exe), "--measure-reader", str(source)], cwd=data, env=env, stdout=log, stderr=log)
            try:
                return wait_json(report, self.args.timeout, process)
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=10)

    def close(self):
        pass


class Android:
    def __init__(self, args):
        self.args = args
        sdk = Path(os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT", "")))
        self.adb = sdk / "platform-tools/adb"
        if not args.serial.startswith("emulator-") or self.device("shell", "getprop", "ro.kernel.qemu").strip() != b"1":
            raise ValueError("Use a simulator; physical-device performance requires separate explicit testing")
        aapts = sorted((sdk / "build-tools").glob("*/aapt"))
        badging = run(aapts[-1], "dump", "badging", args.app).decode()
        if f"name='{PACKAGE}'" not in badging.splitlines()[0] or "application-debuggable" in badging:
            raise ValueError("Expected isolated non-debuggable local.reader.performance APK")
        self.device("install", "-r", args.app)
        props = ["ro.build.fingerprint", "ro.product.cpu.abi", "ro.product.model", "ro.sf.lcd_density"]
        self.environment = {p: self.device("shell", "getprop", p).decode().strip() for p in props}
        driver = self.device("shell", "dumpsys", "SurfaceFlinger").decode()
        self.environment.update(physical_device=False, graphics_backend=next((line.strip() for line in driver.splitlines() if "GLES:" in line), "unknown"),
                                display_size=self.device("shell", "wm", "size").decode().strip(), display_density=self.device("shell", "wm", "density").decode().strip(),
                                host_os=platform.release(), host_architecture=platform.machine(),
                                emulator=run(sdk / "emulator/emulator", "-version").decode().splitlines()[0], host_cpu=platform.processor(), logical_cpus=os.cpu_count())
        self.external = f"/sdcard/Android/data/{PACKAGE}/files"

    def device(self, *command):
        return run(self.adb, "-s", self.args.serial, *command)

    def sample(self, pair, mode, run_id):
        self.device("shell", "am", "force-stop", PACKAGE)
        if mode == "cold":
            # Never clear a user package; the fixed isolated package was checked above.
            self.device("shell", "pm", "clear", PACKAGE)
            self.device("shell", "mkdir", "-p", self.external)
            self.device("push", self.args.book, self.external + "/large-publication.epub")
        report = self.external + "/release-performance.json"
        self.device("shell", "rm", "-f", report)
        self.device("shell", "am", "start", "-W", "-n", PACKAGE + "/com.zhongbai233.epub.reader.ui.reader.ReaderPerformanceActivity", "--es", "performance_run", run_id,
                    "--ez", "performance_endurance", "true" if self.args.scenario == "visible-endurance" else "false")
        end = time.monotonic() + self.args.timeout
        while time.monotonic() < end:
            response = subprocess.run([str(self.adb), "-s", self.args.serial, "exec-out", "cat", report], capture_output=True, timeout=15)
            if response.returncode == 0:
                try:
                    return json.loads(response.stdout)
                except json.JSONDecodeError:
                    pass
            time.sleep(0.4)
        raise TimeoutError("Android performance report timed out")

    def close(self):
        self.device("shell", "am", "force-stop", PACKAGE)


class IOS:
    def __init__(self, args, reader_fixture=True):
        self.args = args
        self.identifier = None
        self.launched = False
        self.host_idle_samples = []
        with (args.app / "Info.plist").open("rb") as stream:
            info = plistlib.load(stream)
        if info["CFBundleIdentifier"] != PACKAGE or "iPhoneSimulator" not in info.get("CFBundleSupportedPlatforms", []):
            raise ValueError("Expected isolated local.reader.performance Simulator app")
        available = [r for r in json.loads(run("xcrun", "simctl", "list", "runtimes", "-j"))["runtimes"] if r.get("isAvailable") and ".iOS-" in r["identifier"]]
        runtime = max(available, key=lambda r: tuple(map(int, r["version"].split("."))))
        types = json.loads(run("xcrun", "simctl", "list", "devicetypes", "-j"))["devicetypes"]
        device = mobile.compatible_tablet(types, runtime["version"], getattr(args, "ios_device_type", None))
        self.environment = {"host_cpu": run("sysctl", "-n", "machdep.cpu.brand_string").decode().strip(), "logical_cpus":os.cpu_count(), "physical_device": False, "runtime": runtime["identifier"], "runtime_build": runtime["buildversion"],
                            "device_type": device, "host_os": platform.release(), "host_architecture": platform.machine(),
                            "xcode": run("xcodebuild", "-version").decode().strip(), "graphics_backend": "CoreSimulator UIKit/Metal"}
        self.environment["host_idle_policy"] = {"minimum_idle_percent": HOST_IDLE_MINIMUM, "consecutive_one_second_samples": HOST_IDLE_CONSECUTIVE, "timeout_seconds": HOST_IDLE_TIMEOUT}
        self.identifier = run("xcrun", "simctl", "create", "ReaderPerf-" + str(uuid.uuid4()), device, runtime["identifier"]).decode().strip()
        try:
            run("xcrun", "simctl", "boot", self.identifier)
            run("xcrun", "simctl", "bootstatus", self.identifier, "-b")
            run("xcrun", "simctl", "install", self.identifier, args.app)
            self.container = Path(run("xcrun", "simctl", "get_app_container", self.identifier, PACKAGE, "data").decode().strip())
            (self.container / "Documents").mkdir(exist_ok=True)
            if reader_fixture:
                shutil.copyfile(args.book, self.container / "Documents/release-performance.epub")
        except BaseException:
            self.close()
            raise

    def sample(self, pair, mode, run_id):
        # Same run root for the cold/indexed pair; report ID remains unique per process.
        filename = "release-performance-reopen.json" if mode == "reopen" else "release-performance.json"
        path = self.container / "Library/Caches" / filename
        path.unlink(missing_ok=True)
        self.launch("--measure-release-book", "--performance-run", str(pair), "--performance-report-id", run_id,
            *(["--performance-endurance"] if self.args.scenario == "visible-endurance" else []),
            *(["--performance-reopen"] if mode == "reopen" else []))
        return wait_json(path, self.args.timeout)

    def launch(self, *arguments):
        if self.launched:
            run("xcrun", "simctl", "terminate", self.identifier, PACKAGE, timeout=30)
            self.launched = False
        observations = {"launch": len(self.host_idle_samples), "samples": []}
        self.host_idle_samples.append(observations)
        try:
            wait_host_idle(observations["samples"])
        finally:
            (self.args.output / "host-idle-samples.json").write_text(json.dumps(self.host_idle_samples, indent=2) + "\n")
        run("xcrun", "simctl", "launch", self.identifier, PACKAGE, *arguments)
        self.launched = True

    def close(self):
        if self.identifier:
            for action in ["shutdown", "delete"]:
                subprocess.run(["xcrun", "simctl", action, self.identifier], capture_output=True, timeout=45)
            self.identifier = None


def collect(args):
    args.app = args.app.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    summary = {"schema": SCHEMA, "kind": "native-client", "passed": False, "books": {}, "unmeasured": ["loading_cancel_latency", "large_shelf_cover_pacing", "physical_device", "direct_gpu_present_latency"]}
    driver = None
    try:
        build = json.loads(args.build_record.read_text())
        initial = sources()
        validate_build(build, initial, args.app, args.pinned_build)
        manifest_path = args.fixtures / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        book = next(b for b in manifest["books"] if b["file"] == args.book_name)
        args.book = (args.fixtures / book["file"]).resolve()
        targets(book["chapters"])
        if digest(args.book) != book["sha256"]:
            raise ValueError("Invalid native reader stress fixture")
        summary.update(source_sha256=build["source"]["sha256"], collection_source_initial_sha256=initial["sha256"], pinned_build=args.pinned_build,
                       artifact_sha256=build["artifact_sha256"], fixture_manifest_sha256=digest(manifest_path), build_command=build["command"], collector_sha256=digest(Path(__file__)), collector_sources=collector_sources(Path(__file__)))
        driver = {"desktop": Desktop, "android": Android, "ios": IOS}[args.platform](args)
        summary["environment"] = dict(driver.environment, client=args.platform, device=args.device, profile="release", harness_schema=SCHEMA, build_command=build["command"], scenario=args.scenario,
                                      harness_sources=harness_sources(build, args.platform, args.scenario))
        values = {"cold": []} if args.scenario == "visible-endurance" else {"cold": [], "reopen": []}
        viewport = None
        for pair in range(args.runs):
            for mode in values:
                run_id = f"{mode}-{pair}-{uuid.uuid4()}"
                value = driver.sample(pair, mode, run_id)
                (args.output / f"{run_id}.json").write_text(json.dumps(value, indent=2) + "\n")
                metrics = normalize(value, args.platform, book, run_id, args.scenario)
                if viewport is not None and value["viewport"] != viewport:
                    raise ValueError("Viewport or accessibility settings changed during measurement")
                viewport = value["viewport"]
                values[mode].append(metrics)
        summary["environment"]["viewport"] = viewport
        for mode, samples in values.items():
            if any(set(s) != set(samples[0]) for s in samples):
                raise ValueError("Native metric coverage changed between repetitions")
            summary["books"][book["file"] + "/" + mode] = {"samples": len(samples), "metrics": {key: statistics.median(sample[key] for sample in samples) for key in samples[0]}}
        summary["baseline_compared"] = bool(args.baseline)
        summary["regressions"] = core.compare(summary, json.loads(args.baseline.read_text())) if args.baseline else []
        if summary["regressions"]:
            raise ValueError(f"Native performance regression: {summary['regressions']}")
        final = sources()
        summary["collection_source_final_sha256"] = final["sha256"]
        summary["source_changed_during_collection"] = initial != final
        if (not args.pinned_build and initial != final) or artifact_hash(args.app) != summary["artifact_sha256"]:
            raise ValueError("Source or application changed during collection")
        summary["passed"] = True
    except (OSError, ValueError, KeyError, RuntimeError, TimeoutError, subprocess.SubprocessError) as error:
        summary["error"] = str(error)
        raise
    finally:
        try:
            if driver:
                driver.close()
        finally:
            (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command_name", required=True)
    build = commands.add_parser("build", help="Build and bind the result to the current dirty source")
    build.add_argument("--app", type=Path, required=True)
    build.add_argument("--output", type=Path, required=True)
    build.add_argument("--command", nargs=argparse.REMAINDER, required=True)
    measure = commands.add_parser("collect")
    measure.add_argument("platform", choices=["desktop", "android", "ios"])
    measure.add_argument("--book-name", choices=["many-chapters.epub", "long-chapter.epub", "illustrated.epub", "tables.epub"], default="many-chapters.epub")
    measure.add_argument("--app", type=Path, required=True)
    measure.add_argument("--build-record", type=Path, required=True)
    measure.add_argument("--fixtures", type=Path, required=True)
    measure.add_argument("--output", type=Path, required=True)
    measure.add_argument("--device", required=True)
    measure.add_argument("--ios-device-type", help="Exact compatible iPad Simulator type identifier; never silently substitutes a smaller viewport")
    measure.add_argument("--serial", default="emulator-5556")
    measure.add_argument("--runs", type=int, default=3, choices=range(3, 11))
    measure.add_argument("--timeout", type=int, default=240)
    measure.add_argument("--baseline", type=Path)
    measure.add_argument("--scenario", choices=["reader", "visible-endurance"], default="reader")
    measure.add_argument("--pinned-build", action="store_true", help="Attribute results to the recorded immutable build; workspace edits are recorded separately, artifact changes still fail")
    args = parser.parse_args()
    if args.command_name == "collect" and args.ios_device_type and args.platform != "ios":
        parser.error("--ios-device-type is only supported for iOS")
    record_build(args) if args.command_name == "build" else collect(args)


if __name__ == "__main__":
    main()
