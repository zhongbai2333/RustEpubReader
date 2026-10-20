#!/usr/bin/env python3
"""Reproduce the opt-in JNI/C ABI image transport/profile assessment on simulators."""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_image_checks", ROOT / "scripts/check-native-performance.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
artifacts = native.load_module("android_native_artifacts", "android-native-artifacts.py")
PROFILES = ("release", "release-fast")
COUNTS = {"generated": 12, "alice": 6}
METRICS = ("encode_parse_ms", "transfer_ms", "bounds_ms", "decode_ms", "total_ms")


def validate(value, run_id, profile, hashes):
    if not isinstance(value, dict):
        raise ValueError("Invalid native bridge envelope")
    if (value.get("passed") is not True or value.get("schema") != 2
            or value.get("run_id") != run_id or value.get("profile") != profile
            or value.get("physical_device") is not False or type(value.get("process_id")) is not int
            or value["process_id"] <= 0 or value.get("lifetime_checks") is not True
            or value.get("byte_and_metadata_parity") is not True
            or value.get("isolated_blank_host") is not True or value.get("release_network_checks") is not False):
        raise ValueError("Invalid or failed native bridge report")
    expected = {(label, count, selected, iteration, transport)
                for label, count in COUNTS.items() for selected in (1, count)
                for iteration in range(7) for transport in ("base64", "resource")}
    cases = value.get("cases")
    if not isinstance(cases, list) or len(cases) != len(expected):
        raise ValueError("Missing native bridge cases")
    seen = set()
    for label, count in COUNTS.items():
        book = value.get(label, {})
        signatures = book.get("image_sha256") if isinstance(book, dict) else None
        if (not isinstance(book, dict) or book.get("sha256") != hashes[label] or not isinstance(signatures, list)
                or len(signatures) != count or any(not isinstance(s, str) or len(s) != 64
                or any(c not in "0123456789abcdef" for c in s) for s in signatures)):
            raise ValueError("Image/book fingerprint mismatch")
        for key in ("prepare_and_first_legacy_chapter_ms", "decoder_warmup_ms"):
            number = book.get(key)
            if type(number) not in (int, float) or not math.isfinite(number) or number < 0:
                raise ValueError("Missing cold preparation/decoder warmup timing")
    for row in cases:
        if not isinstance(row, dict):
            raise ValueError("Invalid native bridge case")
        if (any(type(row.get(k)) is not int for k in ("chapter", "image_count", "selected", "iteration"))
                or row["chapter"] != {"generated": 0, "alice": 7}.get(row.get("book"))):
            raise ValueError("Invalid chapter/count coordinates")
        key = tuple(row.get(k) for k in ("book", "image_count", "selected", "iteration", "transport"))
        if key not in expected or key in seen:
            raise ValueError("Unexpected or duplicated bridge case")
        seen.add(key)
        for metric in METRICS:
            value_ms = row.get(metric)
            if type(value_ms) not in (int, float) or not math.isfinite(value_ms) or not 0 <= value_ms < 60000:
                raise ValueError("Invalid bridge timing")
        if any(type(row.get(k)) is not int or row[k] <= 0 for k in ("json_bytes", "image_bytes")):
            raise ValueError("Missing byte counts")
        dimensions = row.get("dimensions")
        if (not isinstance(dimensions, list) or len(dimensions) != row["selected"]
                or any(not isinstance(d, list) or len(d) != 2 or any(type(x) is not int or x <= 0 for x in d) for d in dimensions)):
            raise ValueError("Invalid native image bounds")
    return value


def validate_memory(value, run_id, profile, book_hash, transport, platform):
    metric = "pss_bytes" if platform == "android" else "physical_footprint_bytes"
    if (not isinstance(value, dict) or value.get("passed") is not True or value.get("schema") != 2
            or value.get("measurement") != "retained-native-chapter" or value.get("run_id") != run_id
            or value.get("profile") != profile or value.get("physical_device") is not False
            or value.get("isolated_blank_host") is not True or value.get("release_network_checks") is not False
            or type(value.get("process_id")) is not int or value["process_id"] <= 0
            or value.get("transport") != transport or value.get("book_sha256") != book_hash
            or type(value.get("image_count")) is not int or value["image_count"] != 12
            or type(value.get("chapter")) is not int or value["chapter"] != 0
            or value.get("bitmap_decode_measured") is not False
            or value.get("probe_gc_requested") is not (platform == "android")
            or type(value.get("json_bytes")) is not int or value["json_bytes"] <= 0):
        raise ValueError("Invalid isolated native memory report")
    points = value.get("steady_memory_samples")
    baseline = value.get("baseline_memory")
    if (not isinstance(points, list) or len(points) != 5 or not isinstance(baseline, dict)
            or type(baseline.get(metric)) is not int or baseline[metric] <= 0
            or any(not isinstance(point, dict) or type(point.get(metric)) is not int or point[metric] <= 0 for point in points)):
        raise ValueError("Missing native memory observations")
    return value


def summarize_memory(runs):
    groups = []
    for platform in sorted({run["platform"] for run in runs}):
        metric = "pss_bytes" if platform == "android" else "physical_footprint_bytes"
        for transport in ("base64", "resource"):
            selected = [json.loads(Path(run["path"]).read_text()) for run in runs
                        if run["platform"] == platform and run["transport"] == transport]
            group = dict(platform=platform, transport=transport, samples=len(selected), metric=metric)
            columns = {"baseline": [row["baseline_memory"][metric] for row in selected],
                       "steady": [statistics.median(point[metric] for point in row["steady_memory_samples"]) for row in selected]}
            columns["steady_delta"] = [after-before for before, after in zip(columns["baseline"], columns["steady"])]
            for extra in ("java_heap_used_bytes", "native_heap_allocated_bytes"):
                if all(extra in row for row in selected):
                    columns[extra] = [row[extra] for row in selected]
            for name, values in columns.items():
                group[name] = dict(median=statistics.median(values), min=min(values), max=max(values))
            groups.append(group)
    return groups


def summarize(runs, skip_first=False):
    keys = ("platform", "profile", "book", "selected", "transport")
    rows = [dict(row, platform=run["platform"], profile=run["profile"])
            for run in runs for row in json.loads(Path(run["path"]).read_text())["cases"]
            if not skip_first or row["iteration"] > 0]
    groups = []
    for key in sorted({tuple(row[k] for k in keys) for row in rows}):
        selected = [row for row in rows if tuple(row[k] for k in keys) == key]
        group = dict(zip(keys, key), samples=len(selected))
        for metric in (*METRICS, "json_bytes", "image_bytes"):
            values = [row[metric] for row in selected]
            group[metric] = dict(median=statistics.median(values), min=min(values), max=max(values))
        groups.append(group)
    return groups


def wait_android(driver, path, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        response = subprocess.run([str(driver.adb), "-s", driver.args.serial, "exec-out", "cat", path],
                                  capture_output=True, timeout=15)
        if response.returncode == 0:
            try:
                return json.loads(response.stdout)
            except json.JSONDecodeError:
                pass
        time.sleep(.3)
    raise TimeoutError("Android image report timed out")


def wait_ios(path, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            try:
                return json.loads(path.read_text())
            except json.JSONDecodeError:
                pass
        time.sleep(.3)
    raise TimeoutError("iOS image report timed out")


def collect(args):
    args.output.mkdir(parents=True, exist_ok=False)
    builds = json.loads(args.builds.read_text())
    initial = native.sources()
    report = dict(passed=False, kind="retained-native-chapter" if args.memory else "image-bridge-and-decode", collection_source_initial=initial["sha256"], runs=[],
                  collector_sha256=native.digest(Path(__file__)),
                  unmeasured=["reader/pagination frames", "physical devices", "per-image ZIP loading",
                              "per-allocation native heap peaks", "device IPA size"])
    output = args.output / "summary.json"
    try:
        platforms = ("android", "ios") if args.platform == "all" else (args.platform,)
        presets = ("release",) if args.memory else PROFILES
        if builds.get("passed") is not True or not isinstance(builds.get("builds"), list) or not builds["builds"]:
            raise ValueError("A successful native build manifest is required")
        pairs = {(row.get("platform"), row.get("profile")) for row in builds["builds"]}
        known = {(platform, profile) for platform in ("android", "ios") for profile in PROFILES}
        required = {(platform, profile) for platform in platforms for profile in presets}
        if len(pairs) != len(builds["builds"]) or not pairs <= known or not required <= pairs:
            raise ValueError("Incomplete or duplicated build matrix")
        report["platforms"] = list(platforms)
        config = {"books": [{"label": "generated", "file": "illustrated.epub", "chapter": 0},
                            {"label": "alice", "file": "alice.epub", "chapter": 7}]}
        fixtures = {"illustrated.epub": args.generated.resolve(), "alice.epub": args.alice.resolve()}
        hashes = {"generated": native.digest(args.generated), "alice": native.digest(args.alice)}
        report.update(source=builds["source"]["sha256"], fixtures={name: dict(path=str(path), sha256=native.digest(path)) for name, path in fixtures.items()})
        signatures = {}
        try:
            report["host_swap_usage"] = native.run("sysctl", "vm.swapusage").decode().strip()
        except (OSError, RuntimeError, subprocess.SubprocessError):
            report["host_swap_usage"] = None
        for profile in presets:
            for platform in platforms:
                build = next(row for row in builds["builds"] if row["platform"] == platform and row["profile"] == profile)
                app = Path(build["artifact"]["path"])
                record = json.loads((Path(build["output"]) / "build.json").read_text())
                native.validate_build(record, initial, app, args.pinned_build)
                if record["source"] != builds["source"] or record["artifact_sha256"] != build["artifact"]["sha256"]:
                    raise ValueError("Build manifest/artifact attribution mismatch")
                provenance = artifacts.verify_apk(app, profile) if platform == "android" else None
                args.app = app
                driver = native.Android(args) if platform == "android" else native.IOS(args, reader_fixture=False)
                try:
                    if platform == "android":
                        directory = driver.external
                        driver.device("shell", "mkdir", "-p", directory)
                        local = args.output / "image-bridge-config.json"; local.write_text(json.dumps(config))
                        driver.device("push", local, directory + "/image-bridge-config.json")
                        for filename, source in fixtures.items():
                            driver.device("push", source, directory + "/" + filename)
                    else:
                        directory = driver.container / "Library/Caches"; directory.mkdir(parents=True, exist_ok=True)
                        (directory / "image-bridge-config.json").write_text(json.dumps(config))
                        for filename, source in fixtures.items():
                            shutil.copyfile(source, directory / filename)
                    pids = set()
                    schedule = ([(index, transport) for index in range(args.runs)
                                 for transport in (("base64", "resource") if index % 2 == 0 else ("resource", "base64"))]
                                if args.memory else [(index, None) for index in range(args.runs)])
                    for index, transport in schedule:
                        run_id = f"{platform}-{profile}-{transport or 'both'}-{index}-{uuid.uuid4()}"
                        run_config = dict(config)
                        if transport:
                            run_config = {"books": config["books"][:1], "memory_transport": transport}
                        if platform == "android":
                            local = args.output / "image-bridge-config.json"; local.write_text(json.dumps(run_config))
                            driver.device("push", local, directory + "/image-bridge-config.json")
                        else:
                            (directory / "image-bridge-config.json").write_text(json.dumps(run_config))
                        if platform == "android":
                            driver.device("shell", "am", "force-stop", native.PACKAGE)
                            path = directory + "/image-bridge-performance.json"; driver.device("shell", "rm", "-f", path)
                            driver.device("shell", "am", "start", "-W", "-n", native.PACKAGE + "/com.zhongbai233.epub.reader.ui.reader.ImageBridgePerformanceActivity", "--es", "performance_run", run_id)
                            value = wait_android(driver, path, args.timeout)
                        else:
                            subprocess.run(["xcrun", "simctl", "terminate", driver.identifier, native.PACKAGE], capture_output=True, timeout=20)
                            path = directory / "image-bridge-performance.json"; path.unlink(missing_ok=True)
                            env = dict(os.environ, SIMCTL_CHILD_READER_PERFORMANCE_RUN=run_id, SIMCTL_CHILD_READER_RUST_PROFILE=profile)
                            native.run("xcrun", "simctl", "launch", driver.identifier, native.PACKAGE, "--measure-image-bridge", env=env)
                            value = wait_ios(path, args.timeout)
                        saved = args.output / (run_id + ".json"); saved.write_text(json.dumps(value, indent=2))
                        if args.memory:
                            validate_memory(value, run_id, profile, hashes["generated"], transport, platform)
                        else:
                            validate(value, run_id, profile, hashes)
                        if value["process_id"] in pids:
                            raise ValueError("Expected independent native processes")
                        pids.add(value["process_id"])
                        for label in (() if args.memory else COUNTS):
                            signature = sorted(value[label]["image_sha256"])
                            if label in signatures and signatures[label] != signature:
                                raise ValueError("Native image bytes differ across platforms/profiles")
                            signatures[label] = signature
                        report["runs"].append(dict(platform=platform, profile=profile, transport=transport, run_id=run_id, process_id=value["process_id"], path=str(saved.resolve()), environment=driver.environment, artifact=build["artifact"], native_provenance=provenance))
                        output.write_text(json.dumps(report, indent=2))
                finally:
                    driver.close()
        if args.memory:
            report.update(groups=summarize_memory(report["runs"]), passed=True,
                          sampling_note="One transport per fresh process; actual native chapter DTO kept reachable for five observations. Android probe explicitly requests GC. No bitmap-cache or whole-reader memory claim.")
        else:
            report.update(groups=summarize(report["runs"]), steady_excluding_iteration_zero=summarize(report["runs"], True), passed=True,
                          sampling_note="Cached chapter bodies; first decoder/framework warmup recorded separately. Iteration zero retained in raw/all summaries and excluded in the additional steady summary. No UI-frame or allocation-peak claim.")
    except BaseException as error:
        report["error"] = str(error)
        raise
    finally:
        report["collection_source_final"] = native.sources()["sha256"]
        output.write_text(json.dumps(report, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("builds", "generated", "alice", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--serial", default="emulator-5556")
    parser.add_argument("--platform", choices=("all", "android", "ios"), default="all")
    parser.add_argument("--runs", type=int, choices=range(3, 11), default=3)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--pinned-build", action="store_true")
    parser.add_argument("--memory", action="store_true", help="Retained chapter DTO memory: release only, one transport per process, generated 12-image fixture")
    collect(parser.parse_args())


if __name__ == "__main__":
    main()
