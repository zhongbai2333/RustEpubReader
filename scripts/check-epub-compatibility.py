#!/usr/bin/env python3
"""Compare the actual eager/lazy native presentation of local EPUBs, offline."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("native_performance", HERE / "check-native-performance.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def compare(eager, lazy, inputs):
    if not isinstance(eager, list) or not isinstance(lazy, list) or len(eager) != len(inputs) or len(lazy) != len(inputs):
        raise ValueError("Incomplete corpus coverage")
    comparisons = []
    for original, deferred, (path, sha) in zip(eager, lazy, inputs, strict=True):
        for report, loading in [(original, "eager"), (deferred, "lazy")]:
            if not isinstance(report, dict) or "error" in report or report.get("path") != str(path) or report.get("sha256") != sha:
                raise ValueError("Input identity or parsing failed")
            if report.get("audit_schema") != 2 or report.get("loading") != loading or report.get("stable_source") is not True:
                raise ValueError("Canonical source was not verified")
            if type(report.get("chapters")) is not int or report["chapters"] < 1:
                raise ValueError("Missing chapter coverage")
            if not isinstance(report.get("canonical_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", report["canonical_sha256"]):
                raise ValueError("Missing canonical fingerprint")
            modes = report.get("modes")
            if not isinstance(modes, list) or len(modes) != 3 or any(not isinstance(m, dict) for m in modes) or [m.get("policy") for m in modes] != ["none", "mixed", "publisher"]:
                raise ValueError("Missing native style policy")
            if any(not isinstance(m.get("styles_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", m["styles_sha256"]) for m in modes):
                raise ValueError("Missing complete style fingerprint")
            if any(type(m.get("blocks")) is not int or m["blocks"] < 1 for m in modes):
                raise ValueError("Empty native presentation")
        if type(deferred.get("maximum_cached_chapters")) is not int or not 1 <= deferred["maximum_cached_chapters"] <= 8:
            raise ValueError("Unbounded or unmeasured lazy chapter cache")
        for key in ["chapters", "canonical_sha256", "modes"]:
            if original[key] != deferred[key]:
                raise ValueError("Eager/lazy mismatch: " + key + " in " + str(path))
        comparisons.append({"path": str(path), "sha256": sha, "chapters": original["chapters"], "canonical_sha256": original["canonical_sha256"], "policies": 3, "maximum_cached_chapters": deferred["maximum_cached_chapters"]})
    return comparisons


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--build-record", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("books", nargs="+", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    report = {"passed": False, "scope": "parser and native presentation; not platform rendering or script execution"}
    try:
        before = native.sources()
        build = json.loads(args.build_record.read_text())
        native.validate_build(build, before, args.binary.resolve())
        inputs = [(p.resolve(strict=True), native.digest(p)) for p in args.books]
        if len({p for p, _ in inputs}) != len(inputs):
            raise ValueError("Duplicate corpus input")
        report.update(source=before, build=build, inputs=[{"path": str(p), "sha256": sha} for p, sha in inputs])
        commands = []
        for loading in ["eager", "lazy"]:
            command = [str(args.binary.resolve()), *(["--lazy"] if loading == "lazy" else []), *[str(p) for p, _ in inputs]]
            with (args.output / (loading + ".json")).open("wb") as output, (args.output / (loading + ".log")).open("wb") as log:
                result = subprocess.run(command, stdout=output, stderr=log, timeout=args.timeout)
            commands.append({"loading": loading, "command": command, "exit_code": result.returncode})
            report["commands"] = commands
            if result.returncode:
                raise ValueError("Corpus audit failed: " + loading)
        report["comparisons"] = compare(json.loads((args.output / "eager.json").read_text()), json.loads((args.output / "lazy.json").read_text()), inputs)
        if any(native.digest(p) != sha for p, sha in inputs) or native.sources() != before:
            raise ValueError("Source or corpus changed during collection")
        report["passed"] = True
    except Exception as error:
        report["error"] = str(error)
    (args.output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    if not report["passed"]:
        raise SystemExit(report.get("error", "Corpus audit failed"))
    print("PASS: " + str(len(report["comparisons"])) + " EPUBs; eager/lazy canonical content and three style policies agree")


if __name__ == "__main__":
    main()
