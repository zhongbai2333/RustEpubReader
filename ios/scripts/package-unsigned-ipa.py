#!/usr/bin/env python3
"""Package an unsigned arm64 iPhoneOS archive, or verify the resulting IPA.

Only the application goes into Payload; archive metadata and dSYMs stay out.
This does not sign, provision, install, or upload the application.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import plistlib
import re
import stat
import struct
import tempfile
import zipfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def executable_name(info):
    require(info.get("CFBundlePackageType") == "APPL", "Not an application bundle")
    require(info.get("CFBundleSupportedPlatforms") == ["iPhoneOS"], "Expected an iPhoneOS device app, not a simulator app")
    require(info.get("DTPlatformName", "iphoneos") == "iphoneos", "Unexpected Apple platform")
    name = info.get("CFBundleExecutable", "")
    require(isinstance(name, str) and name not in ("", ".", "..") and not any(c in name for c in "/\\\0"), "Invalid bundle executable name")
    require(bool(info.get("CFBundleIdentifier")), "Missing bundle identifier")
    return name


def check_executable(data):
    require(len(data) >= 32, "Truncated Mach-O executable")
    magic, cpu, _, kind, count, command_bytes = struct.unpack_from("<6I", data)
    require(magic == 0xFEEDFACF and cpu == 0x100000C and kind == 2, "Expected an arm64 Mach-O application executable")
    end = 32 + command_bytes
    require(end <= len(data) and count <= command_bytes // 8, "Invalid Mach-O command table")
    position, device = 32, False
    for _ in range(count):
        require(position + 8 <= end, "Truncated Mach-O command")
        command, size = struct.unpack_from("<2I", data, position)
        require(size >= 8 and position + size <= end, "Invalid Mach-O command size")
        if command == 0x32:  # LC_BUILD_VERSION: arm64 also exists on simulators.
            require(size >= 24 and struct.unpack_from("<I", data, position + 8)[0] == 2, "Executable is not built for physical iOS devices")
            device = True
        elif command == 0x25:  # LC_VERSION_MIN_IPHONEOS
            require(size >= 16, "Invalid iOS deployment command")
            device = True
        elif command == 0x1D:  # LC_CODE_SIGNATURE
            require(size >= 16 and struct.unpack_from("<I", data, position + 12)[0] == 0, "Application executable is already signed")
        position += size
    require(position == end and device, "Missing iOS device build metadata")


def verify(ipa, checksum=None):
    ipa = Path(ipa)
    with zipfile.ZipFile(ipa) as archive:
        members = archive.infolist()
        names = [item.filename for item in members]
        require(len(names) == len(set(names)), "Duplicate ZIP entries")
        for name in names:
            path = PurePosixPath(name)
            require(not path.is_absolute() and ".." not in path.parts and "\\" not in name and "\0" not in name, "Unsafe IPA entry")
        roots = {PurePosixPath(name).parts[1] for name in names if name.startswith("Payload/") and len(PurePosixPath(name).parts) >= 2}
        require(len(roots) == 1 and next(iter(roots)).endswith(".app"), "Expected one application under Payload")
        root = "Payload/" + next(iter(roots))
        require(all(name in ("Payload/", root + "/") or name.startswith(root + "/") for name in names), "Unexpected content outside Payload application")
        for item in members:
            parts = PurePosixPath(item.filename).parts
            require("_CodeSignature" not in parts and parts[-1] != "embedded.mobileprovision", "Signing metadata found in unsigned IPA")
            if stat.S_ISLNK(item.external_attr >> 16):
                target = archive.read(item).decode("utf-8")
                link = PurePosixPath(target)
                require(target and not link.is_absolute() and ".." not in link.parts and "\\" not in target, "Unsafe bundle symlink")
        info = plistlib.loads(archive.read(root + "/Info.plist"))
        name = executable_name(info)
        member = archive.getinfo(root + "/" + name)
        require(stat.S_ISREG(member.external_attr >> 16) and member.external_attr >> 16 & 0o111, "Application executable permissions were not preserved")
        check_executable(archive.read(member))
        require(archive.testzip() is None, "IPA CRC check failed")
    digest = hashlib.sha256()
    with ipa.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    digest = digest.hexdigest()
    if checksum:
        fields = Path(checksum).read_text().split()
        require(fields == [digest, ipa.name], "IPA checksum does not match")
    return {"file": ipa.name, "bytes": ipa.stat().st_size, "sha256": digest,
            "bundle_id": info["CFBundleIdentifier"], "version": info.get("CFBundleShortVersionString"),
            "platform": "iPhoneOS", "architecture": "arm64", "application_signed": False}


def package(archive, output_dir, version):
    require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+-]{0,127}", version), "Version must be a safe filename label")
    apps = list((Path(archive) / "Products/Applications").glob("*.app"))
    require(len(apps) == 1 and apps[0].is_dir() and not apps[0].is_symlink(), "Archive must contain one physical-device app")
    app = apps[0]
    name = executable_name(plistlib.loads((app / "Info.plist").read_bytes()))
    check_executable((app / name).read_bytes())
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    destination = output_dir / f"RustEpubReader-iOS-arm64-{version}-unsigned.ipa"
    with tempfile.TemporaryDirectory(prefix=".unsigned-ipa-", dir=output_dir) as staging:
        temporary = Path(staging) / destination.name
        with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False) as bundle:
            for item in sorted(app.rglob("*")):
                if item.name == ".DS_Store" or item.name.startswith("._"):
                    continue
                target = (PurePosixPath("Payload") / app.name / item.relative_to(app).as_posix()).as_posix()
                if item.is_symlink():
                    require(item.resolve().is_relative_to(app.resolve()), "Bundle symlink escapes application")
                    entry = zipfile.ZipInfo(target)
                    entry.create_system = 3
                    entry.external_attr = item.lstat().st_mode << 16
                    bundle.writestr(entry, os.readlink(item).encode("utf-8"))
                else:
                    bundle.write(item, target)
        result = verify(temporary)
        receipt = Path(staging) / (destination.name + ".sha256")
        receipt.write_text(f"{result['sha256']}  {destination.name}\n")
        temporary.replace(destination)
        receipt.replace(output_dir / receipt.name)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    pack = commands.add_parser("pack")
    pack.add_argument("--archive", required=True, type=Path)
    pack.add_argument("--output-dir", required=True, type=Path)
    pack.add_argument("--version", required=True)
    check = commands.add_parser("verify")
    check.add_argument("ipa", type=Path)
    check.add_argument("--checksum", type=Path)
    args = parser.parse_args()
    try:
        result = package(args.archive, args.output_dir, args.version) if args.command == "pack" else verify(args.ipa, args.checksum)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, struct.error, plistlib.InvalidFileException) as error:
        parser.exit(1, f"Unsigned IPA check failed: {error}\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
