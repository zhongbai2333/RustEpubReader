#!/usr/bin/env python3
"""Transport one admitted simulator build and require every parallel runtime shard."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import tarfile
import uuid

SPEC = importlib.util.spec_from_file_location("mobile_runtime", Path(__file__).with_name("check-mobile-runtime.py"))
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)
SHARDS = {
    "ipad-layout": ("layout", "ipad"),
    "ipad-features": ("features", "ipad"),
    "iphone-features": ("features", "iphone"),
    "ipad-selection": ("selection", "ipad"),
    "ipad-motion": ("motion", "ipad"),
}


def source_sha():
    return runtime.run("git", "rev-parse", "HEAD").decode().strip()


def check_record(record):
    if record.get("schema") != 1 or record.get("source_sha") != source_sha():
        raise RuntimeError("Simulator build source differs from this checkout")
    if record.get("collector_sha256") != runtime.artifact_hash(Path(__file__).with_name("check-mobile-runtime.py")):
        raise RuntimeError("Simulator build uses another runtime collector")
    if os.environ.get("GITHUB_RUN_ID") and str(record.get("producer_run_id")) != os.environ["GITHUB_RUN_ID"]:
        raise RuntimeError("Simulator build belongs to another workflow run")
    attempt = os.environ.get("GITHUB_RUN_ATTEMPT")
    if attempt and not 1 <= int(record.get("producer_run_attempt", 0)) <= int(attempt):
        raise RuntimeError("Simulator build belongs to a future or invalid workflow attempt")


def executables(app, runner):
    for product in (app, runner):
        info = plistlib.loads((product / "Info.plist").read_bytes())
        binary = product / info["CFBundleExecutable"]
        if not binary.resolve().is_relative_to(product.resolve()) or not binary.is_file() or not os.access(binary, os.X_OK):
            raise RuntimeError("Missing executable permissions in transported simulator product")


def pack(products, manifest, output):
    products, manifest = products.resolve(), manifest.resolve()
    if manifest.parent != products:
        raise RuntimeError("XCTest manifest must be inside the transported product root")
    app = products / "Debug-iphonesimulator/ReaderPreview.app"
    runner = runtime.ios_selection_inputs(app, manifest)
    executables(app, runner)
    output.mkdir(parents=True, exist_ok=False)
    archive = output / "simulator-tests.tar.gz"
    members = [app, runner, manifest]
    frameworks = products / "Debug-iphonesimulator/PackageFrameworks"
    if frameworks.exists():
        members.append(frameworks)
    with tarfile.open(archive, "w:gz") as tar:
        for member in members:
            if any(p.is_symlink() and not p.resolve().is_relative_to(products) for p in member.rglob("*")):
                raise RuntimeError("Simulator product symlink escapes its product root")
            tar.add(member, arcname=str(member.relative_to(products)))
    record = dict(schema=1, source_sha=source_sha(),
                  producer_run_id=os.environ.get("GITHUB_RUN_ID", "local-" + str(uuid.uuid4())),
                  producer_run_attempt=os.environ.get("GITHUB_RUN_ATTEMPT", "1"),
                  collector_sha256=runtime.artifact_hash(Path(__file__).with_name("check-mobile-runtime.py")),
                  app_sha256=runtime.artifact_hash(app), ui_runner_sha256=runtime.artifact_hash(runner),
                  xctestrun_sha256=runtime.artifact_hash(manifest), manifest=manifest.name,
                  transport_sha256=runtime.artifact_hash(archive),
                  xcode_version=runtime.run("xcodebuild", "-version").decode().strip())
    (output / "build.json").write_text(json.dumps(record, indent=2) + "\n")
    return record


def unpack(transport, products):
    record = json.loads((transport / "build.json").read_text())
    check_record(record)
    archive = transport / "simulator-tests.tar.gz"
    if runtime.artifact_hash(archive) != record["transport_sha256"]:
        raise RuntimeError("Simulator transport checksum differs")
    if runtime.run("xcodebuild", "-version").decode().strip() != record["xcode_version"]:
        raise RuntimeError("Runtime host Xcode differs from the build host")
    products.mkdir(parents=True, exist_ok=False)
    with tarfile.open(archive) as tar:
        tar.extractall(products, filter="data")
    name = record["manifest"]
    if Path(name).name != name:
        raise RuntimeError("Invalid transported manifest name")
    manifest = products / name
    app = products / "Debug-iphonesimulator/ReaderPreview.app"
    runner = runtime.ios_selection_inputs(app, manifest)
    executables(app, runner)
    for key, path in (("app_sha256", app), ("ui_runner_sha256", runner), ("xctestrun_sha256", manifest)):
        if runtime.artifact_hash(path) != record[key]:
            raise RuntimeError("Restored simulator product differs: " + key)
    return record


def aggregate(reports, build, prefix, runtime_version="26.4"):
    record = json.loads(build.read_text())
    check_record(record)
    identities, devices, results = set(), set(), {}
    for name, (shard, family) in SHARDS.items():
        folder = reports / (prefix + "-receipt-" + name)
        report = runtime.validate_report((folder / "summary.json").read_bytes())
        expected = {"platform": "ios", "ios_shard": shard, "ios_device_family": family,
                    "source_sha": record["source_sha"], "build_record_sha256": runtime.artifact_hash(build),
                    "collector_sha256": record["collector_sha256"], "artifact_sha256": record["app_sha256"],
                    "ui_runner_sha256": record["ui_runner_sha256"], "xctestrun_sha256": record["xctestrun_sha256"],
                    "artifacts_unchanged": True}
        if any(type(report.get(key)) != type(value) or report.get(key) != value for key, value in expected.items()):
            raise RuntimeError("Parallel runtime identity differs: " + name)
        identity = str(uuid.UUID(report["run_id"]))
        system = report["ios_runtime"]
        device = str(uuid.UUID(system["simulator_id"]))
        product = "iPad" if family == "ipad" else "iPhone"
        if (identity in identities or device in devices or
            product not in system["device_type"] or
            system["version"].split(".")[:len(runtime_version.split("."))] != runtime_version.split(".")):
            raise RuntimeError("Parallel runtime device/version differs or repeats: " + name)
        identities.add(identity); devices.add(device)
        checks = report["checks"]
        if shard == "layout":
            if set(checks) != {"single-column", "navigation", "corpus"}:
                raise RuntimeError("Incomplete iPad layout shard")
            for value in checks.values():
                runtime.validate_report(json.dumps(value))
            runtime.validate_corpus(checks["corpus"], "ios")
        else:
            if set(checks) != {shard}:
                raise RuntimeError("Incomplete XCTest shard: " + name)
            installation = json.loads((folder / "xctest-installation.json").read_text())
            if installation.get("unchanged") is not True or installation["before"] != installation["after"]:
                raise RuntimeError("XCTest replaced its admitted installation: " + name)
            if shard == "features":
                tree = json.loads((folder / "platform-parity-tests.json").read_text())
                runtime.validate_ios_xctest(tree, runtime.IOS_FEATURE_TESTS, 1, system["simulator_id"])
                runtime.validate_report(json.dumps(checks[shard]["platform_parity"]))
            else:
                tree = json.loads((folder / "selection-tests.json").read_text())
                cases = runtime.IOS_SELECTION_TESTS if shard == "selection" else runtime.IOS_MOTION_TESTS
                runtime.validate_ios_xctest(tree, cases, 3, system["simulator_id"])
                if shard == "selection":
                    runtime.validate_selection(checks[shard], "ios")
                    runtime.validate_ios_input_runs(folder)
                else:
                    runtime.validate_reduced_motion(checks[shard]["reduced_motion"], "ios")
            model = tree["devices"][0].get("modelName", "")
            if product not in model:
                raise RuntimeError("XCTest ran on another device family: " + name)
        results[name] = {"passed": True, "run_id": identity, "runtime": system}
    return {"passed": True, "source_sha": record["source_sha"], "shards": results,
            "selection_iterations_per_mode": 3, "motion_iterations": 3}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser("pack")
    create.add_argument("--products", type=Path, required=True)
    create.add_argument("--manifest", type=Path, required=True)
    create.add_argument("--output", type=Path, required=True)
    restore = commands.add_parser("unpack")
    restore.add_argument("--transport", type=Path, required=True)
    restore.add_argument("--products", type=Path, required=True)
    verify = commands.add_parser("aggregate")
    verify.add_argument("--reports", type=Path, required=True)
    verify.add_argument("--build", type=Path, required=True)
    verify.add_argument("--prefix", required=True)
    verify.add_argument("--output", type=Path, required=True)
    verify.add_argument("--ios-runtime-version", default="26.4")
    args = parser.parse_args()
    if args.command == "pack":
        pack(args.products, args.manifest, args.output)
    elif args.command == "unpack":
        unpack(args.transport, args.products)
    else:
        value = aggregate(args.reports, args.build, args.prefix, args.ios_runtime_version)
        args.output.write_text(json.dumps(value, indent=2) + "\n")
        print(json.dumps(value))


if __name__ == "__main__":
    main()
