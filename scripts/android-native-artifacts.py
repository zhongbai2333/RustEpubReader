#!/usr/bin/env python3
"""Publish the selected cached Cargo libraries by content and verify assembled APKs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {"armeabi-v7a": "armv7-linux-androideabi", "arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android"}
ASSET = "assets/reader-native.json"


def digest(stream):
    value = hashlib.sha256()
    for chunk in iter(lambda: stream.read(65536), b""):
        value.update(chunk)
    return value.hexdigest()


def file_hash(path):
    with path.open("rb") as stream:
        return digest(stream)


def cargo_target():
    output = subprocess.run(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
                            cwd=ROOT / "android-bridge", check=True, capture_output=True)
    return Path(json.loads(output.stdout)["target_directory"])


def describe(profile, target):
    if profile not in ("release", "release-fast"):
        raise ValueError("Unsupported native profile")
    libraries = {}
    for abi, triple in TARGETS.items():
        path = target / triple / profile / "libandroid_bridge.so"
        size = path.stat().st_size
        if size <= 0:
            raise ValueError(f"Empty native artifact: {abi}")
        libraries[abi] = dict(sha256=file_hash(path), bytes=size)
    return dict(schema=1, profile=profile, libraries=libraries)


def atomic_copy(source, destination, expected):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() and destination.stat().st_size == expected["bytes"] and file_hash(destination) == expected["sha256"]:
        return
    scratch = None
    try:
        with tempfile.NamedTemporaryFile(dir=destination.parent, prefix=".reader-rust-", delete=False) as output:
            scratch = Path(output.name)
            with source.open("rb") as stream:
                shutil.copyfileobj(stream, output, 65536)
        if scratch.stat().st_size != expected["bytes"] or file_hash(scratch) != expected["sha256"]:
            raise ValueError("Cargo artifact changed during publication")
        os.replace(scratch, destination)
    finally:
        if scratch is not None:
            scratch.unlink(missing_ok=True)


def publish(profile, target, output, assets):
    # Preflight every ABI before changing any generated destination.
    metadata = describe(profile, target)
    for abi, triple in TARGETS.items():
        atomic_copy(target / triple / profile / "libandroid_bridge.so", output / abi / "libandroid_bridge.so", metadata["libraries"][abi])
    assets.mkdir(parents=True, exist_ok=True)
    path = assets / "reader-native.json"
    data = (json.dumps(metadata, sort_keys=True, indent=2) + "\n").encode()
    if not path.exists() or path.read_bytes() != data:
        scratch = None
        try:
            with tempfile.NamedTemporaryFile(dir=assets, prefix=".reader-native-", delete=False) as stream:
                scratch = Path(stream.name); stream.write(data)
            os.replace(scratch, path)
        finally:
            if scratch is not None:
                scratch.unlink(missing_ok=True)
    return metadata


def validate_metadata(value, profile):
    if (not isinstance(value, dict) or type(value.get("schema")) is not int or value["schema"] != 1
            or value.get("profile") != profile or profile not in ("release", "release-fast")
            or not isinstance(value.get("libraries"), dict) or set(value["libraries"]) != set(TARGETS)):
        raise ValueError("Invalid native provenance/profile")
    for info in value["libraries"].values():
        if (not isinstance(info, dict) or type(info.get("bytes")) is not int or info["bytes"] <= 0
                or not isinstance(info.get("sha256"), str) or len(info["sha256"]) != 64
                or any(c not in "0123456789abcdef" for c in info["sha256"])):
            raise ValueError("Invalid native fingerprint")


def verify_apk(path, profile, expected=None):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("APK contains duplicate ZIP entries")
        if ASSET not in names or archive.getinfo(ASSET).file_size > 16384:
            raise ValueError("APK lacks bounded native provenance; rebuild before accepting it")
        metadata = json.loads(archive.read(ASSET))
        validate_metadata(metadata, profile)
        if expected is not None and metadata != expected:
            raise ValueError("Packaged native provenance differs from selected Cargo output")
        abis = []
        for name in names:
            if name.startswith("lib/") and name.endswith("/libandroid_bridge.so"):
                parts = name.split("/")
                if len(parts) != 3 or parts[1] not in TARGETS:
                    raise ValueError("Unexpected packaged native ABI")
                abi = parts[1]; info = metadata["libraries"][abi]
                with archive.open(name) as stream:
                    actual = digest(stream)
                if archive.getinfo(name).file_size != info["bytes"] or actual != info["sha256"]:
                    raise ValueError(f"Packaged native library does not match its profile: {abi}")
                abis.append(abi)
        if not abis:
            raise ValueError("APK has no reader native library")
    return dict(profile=profile, packaged_abis=sorted(abis), libraries=metadata["libraries"])


def verify_outputs(metadata_path, profile, target):
    expected = describe(profile, target)
    value = json.loads(metadata_path.read_bytes())
    elements = value.get("elements")
    if not isinstance(elements, list) or not elements:
        raise ValueError("Missing APK output listing")
    seen = set()
    for element in elements:
        filename = element.get("outputFile") if isinstance(element, dict) else None
        if not isinstance(filename, str) or Path(filename).name != filename or not filename.endswith(".apk"):
            raise ValueError("Invalid APK output filename")
        result = verify_apk(metadata_path.parent / filename, profile, expected)
        seen.update(result["packaged_abis"])
    if seen != set(TARGETS):
        raise ValueError("Incomplete packaged ABI coverage")
    return dict(profile=profile, verified_abis=sorted(seen))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    publish_command = commands.add_parser("publish")
    publish_command.add_argument("--profile", choices=("release", "release-fast"), required=True)
    publish_command.add_argument("--output", type=Path, required=True)
    publish_command.add_argument("--assets", type=Path, required=True)
    verify = commands.add_parser("verify-outputs")
    verify.add_argument("--profile", choices=("release", "release-fast"), required=True)
    verify.add_argument("--metadata", type=Path, required=True)
    apk = commands.add_parser("verify-apk")
    apk.add_argument("--profile", choices=("release", "release-fast"), required=True)
    apk.add_argument("--apk", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "publish":
        result = publish(args.profile, cargo_target(), args.output, args.assets)
    elif args.command == "verify-outputs":
        result = verify_outputs(args.metadata, args.profile, cargo_target())
    else:
        result = verify_apk(args.apk, args.profile)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
