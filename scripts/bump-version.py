#!/usr/bin/env python3
"""Update/check app versions without changing dependencies or creating Git tags.

Python 3.9+, standard library only. Run --help for examples and options.
"""

import argparse
import difflib
import os
from pathlib import Path
import re
import stat
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
FILES = ("Cargo.toml", "Cargo.lock", "android/gradle.properties",
         "ios/RustEpubReader.xcodeproj/project.pbxproj")
VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
WORKSPACE = r"(?ms)^\[workspace\.package\][^\n]*\n.*?(?=^\[|\Z)"
LOCK_PACKAGE = r"(?ms)^\[\[package\]\][^\n]*\n.*?(?=^\[\[package\]\]|\Z)"


def values(text, pattern, label, count=None):
    matches = re.findall(pattern, text, re.MULTILINE)
    if not matches or (count is not None and len(matches) != count):
        raise ValueError(f"{label}: expected {count or 'at least one'} value(s), found {len(matches)}")
    if len(set(matches)) != 1:
        raise ValueError(f"{label}: conflicting values: {matches}")
    return matches[0], len(matches)


def version_tuple(value):
    if not re.fullmatch(VERSION, value):
        raise ValueError(f"Expected a stable X.Y.Z version, got {value!r}")
    return tuple(map(int, value.split('.')))


def build_tuple(value):
    if not re.fullmatch(r"[1-9][0-9]*(?:\.(?:0|[1-9][0-9]*)){0,2}", value):
        raise ValueError(f"Expected a positive iOS build number (e.g. 2 or 1017.4), got {value!r}")
    return tuple(map(int, value.split('.')))


def replace_value(text, pattern, value):
    # Every pattern captures only the value to replace, preserving formatting.
    return re.sub(pattern, lambda m: m[0][:m.start(1)-m.start()] + value
                  + m[0][m.end(1)-m.start():], text, flags=re.MULTILINE)


def load(root):
    data = {name: (root / name).read_bytes() for name in FILES}
    texts = {name: content.decode('utf-8') for name, content in data.items()}
    workspace = re.findall(WORKSPACE, texts[FILES[0]])
    if len(workspace) != 1:
        raise ValueError("Cargo.toml: expected one [workspace.package] section")
    cargo, _ = values(workspace[0], r'^version[ \t]*=[ \t]*"([^"\r\n]+)"', 'workspace version', 1)
    packages = [p for p in re.findall(LOCK_PACKAGE, texts[FILES[1]])
                if re.search(r'^name = "rust_epub_reader"\r?$', p, re.MULTILINE)]
    if len(packages) != 1 or re.search(r'^source[ \t]*=', packages[0], re.MULTILINE):
        raise ValueError("Cargo.lock: expected one local rust_epub_reader package")
    locked, _ = values(packages[0], r'^version = "([^"\r\n]+)"', 'locked desktop version', 1)
    android, _ = values(texts[FILES[2]], r'^APP_VERSION_NAME=([^\r\n]+)', 'Android version', 1)
    code, _ = values(texts[FILES[2]], r'^APP_VERSION_CODE=([^\r\n]+)', 'Android version code', 1)
    ios, configurations = values(texts[FILES[3]], r'\bMARKETING_VERSION = ([^;]+);', 'iOS version')
    build, _ = values(texts[FILES[3]], r'\bCURRENT_PROJECT_VERSION = ([^;]+);', 'iOS build', configurations)
    versions = {'Cargo.toml': cargo, 'Cargo.lock': locked, 'Android': android, 'iOS': ios}
    if len(set(versions.values())) != 1:
        raise ValueError(f"App versions disagree; reconcile them before bumping: {versions}")
    version_tuple(cargo)
    if not re.fullmatch(r'[1-9][0-9]*', code) or int(code) > 2_100_000_000:
        raise ValueError(f"Invalid Android version code: {code!r}")
    build_tuple(build)
    return data, texts, workspace[0], packages[0], cargo, int(code), build


def prepare(root, target=None, build_only=False, android_code=None, ios_build=None):
    data, texts, workspace, package, current, code, build = load(root)
    release = current
    if not build_only:
        if target in ('patch', 'minor', 'major'):
            parts = list(version_tuple(current))
            index = {'major': 0, 'minor': 1, 'patch': 2}[target]
            parts[index] += 1
            parts[index+1:] = [0] * (2-index)
            release = '.'.join(map(str, parts))
        else:
            release = target.removeprefix('v')
        if version_tuple(release) <= version_tuple(current):
            raise ValueError(f"New version {release} must be greater than {current}")
    next_code = code + 1 if android_code is None else android_code
    if not code < next_code <= 2_100_000_000:
        raise ValueError(f"Android code must be greater than {code} and at most 2100000000")
    parts = list(build_tuple(build))
    parts[-1] += 1
    next_build = '.'.join(map(str, parts)) if ios_build is None else ios_build
    # Missing components compare as zero, so 2 and 2.0 cannot pass as an increment.
    padded = lambda v: build_tuple(v) + (0,) * (3-len(build_tuple(v)))
    if padded(next_build) <= padded(build):
        raise ValueError(f"iOS build must be greater than {build}")
    changed = dict(texts)
    pattern = r'^version[ \t]*=[ \t]*"([^"\r\n]+)"'
    changed[FILES[0]] = texts[FILES[0]].replace(workspace, replace_value(workspace, pattern, release), 1)
    changed[FILES[1]] = texts[FILES[1]].replace(package, replace_value(package, pattern, release), 1)
    changed[FILES[2]] = replace_value(texts[FILES[2]], r'^APP_VERSION_NAME=([^\r\n]+)', release)
    changed[FILES[2]] = replace_value(changed[FILES[2]], r'^APP_VERSION_CODE=([^\r\n]+)', str(next_code))
    changed[FILES[3]] = replace_value(texts[FILES[3]], r'\bMARKETING_VERSION = ([^;]+);', release)
    changed[FILES[3]] = replace_value(changed[FILES[3]], r'\bCURRENT_PROJECT_VERSION = ([^;]+);', next_build)
    return data, changed, f"App {current} -> {release}; Android code {code} -> {next_code}; iOS build {build} -> {next_build}"


def stage(path, content):
    fd, name = tempfile.mkstemp(prefix='.reader-version-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(content)
        os.chmod(name, stat.S_IMODE(path.stat().st_mode))
    except BaseException:
        os.unlink(name)
        raise
    return Path(name)


def write_changes(root, originals, changed):
    staged = {}
    written = []
    try:
        for name, text in changed.items():
            if text.encode('utf-8') != originals[name]:
                staged[name] = stage(root / name, text.encode('utf-8'))
        if any((root / name).read_bytes() != content for name, content in originals.items()):
            raise ValueError("Version files changed during preparation; retry with the latest files")
        try:
            for name, temporary in staged.items():
                os.replace(temporary, root / name)
                written.append(name)
        except BaseException:
            for name in reversed(written):
                temporary = stage(root / name, originals[name])
                try:
                    os.replace(temporary, root / name)
                finally:
                    temporary.unlink(missing_ok=True)
            raise
    finally:
        for temporary in staged.values():
            temporary.unlink(missing_ok=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('version', nargs='?', help='patch, minor, major, or explicit X.Y.Z (optional v prefix)')
    parser.add_argument('--build-only', action='store_true', help='keep the app version; increment Android/iOS build numbers')
    parser.add_argument('--android-code', type=int, help='explicit new Android version code (must increase)')
    parser.add_argument('--ios-build', help='explicit new iOS build, e.g. 1017.4 (must increase)')
    parser.add_argument('--dry-run', action='store_true', help='show a diff without writing files')
    parser.add_argument('--check', action='store_true', help='check that all committed app version fields agree')
    parser.add_argument('--tag', help='with --check: require an exact release tag vX.Y.Z')
    args = parser.parse_args(argv)
    if args.check:
        if args.version or args.build_only or args.android_code is not None or args.ios_build or args.dry_run:
            parser.error('--check cannot be combined with update options')
    elif bool(args.version) == args.build_only or args.tag:
        parser.error('choose a version OR --build-only; --tag requires --check')
    try:
        if args.check:
            _, _, _, _, version, code, build = load(ROOT)
            if args.tag is not None and args.tag != 'v' + version:
                raise ValueError(f"Release tag {args.tag!r} does not match source version v{version}")
            print(f"OK: app {version}; Android code {code}; iOS build {build}")
            return 0
        original, changed, summary = prepare(ROOT, args.version, args.build_only, args.android_code, args.ios_build)
        print(summary)
        if args.dry_run:
            for name in FILES:
                sys.stdout.writelines(difflib.unified_diff(original[name].decode('utf-8').splitlines(True),
                                                         changed[name].splitlines(True), fromfile=name, tofile=name))
        else:
            write_changes(ROOT, original, changed)
            print('Updated version files. Review the diff before committing/tagging.')
        return 0
    except (ValueError, OSError, UnicodeError) as error:
        print(f"Version error: {error}", file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
