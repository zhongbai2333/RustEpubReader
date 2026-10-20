#!/usr/bin/env python3
"""Collect repeated Release core samples and compare only compatible baselines."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import os
import statistics
import subprocess


def validate_sample(value, book):
    if value.get("profile") != "release" or value.get("book_sha256") != book["sha256"]:
        raise ValueError("Wrong profile or EPUB fingerprint")
    if value.get("chapter_count") != book["chapters"]:
        raise ValueError("Chapter count changed")
    if value["cold_stats"][:2] != [0, 0]:
        raise ValueError("Opening eagerly materialized chapter bodies")
    if value["sequential_200_chapters"]["cache_stats"][0] > 8:
        raise ValueError("Chapter cache is no longer bounded")
    metrics = {
        "cold_catalogue_ms": value["cold_catalogue_ms"],
        "cached_catalogue_ms": value["cached_catalogue_ms"],
        "body_style_p95_ms": value["sequential_200_chapters"]["p95_body_style_ms"],
    }
    memory = value.get("rss_bytes", {})
    for source, target in [("after_200_chapters", "retained_rss_bytes"), ("peak", "peak_rss_bytes")]:
        if memory.get(source) is not None:
            metrics[target] = memory[source]
    for key, number in metrics.items():
        if not isinstance(number, (int, float)) or isinstance(number, bool) or not math.isfinite(number) or number < 0:
            raise ValueError(f"Invalid performance metric: {key}")
    return metrics


def compare(current, baseline, relative=0.25, absolute_ms=5.0):
    if current["environment"] != baseline["environment"]:
        raise ValueError("Different hardware/OS/toolchain/profile: establish a separate baseline")
    if current["fixture_manifest_sha256"] != baseline["fixture_manifest_sha256"]:
        raise ValueError("Different corpus: establish a separate baseline")
    if set(current["books"]) != set(baseline["books"]):
        raise ValueError("Incomplete performance corpus")
    regressions = []
    for book, entry in current["books"].items():
        if set(entry["metrics"]) != set(baseline["books"][book]["metrics"]):
            raise ValueError("Different metric coverage: establish a separate baseline")
        for metric, value in entry["metrics"].items():
            old = baseline["books"][book]["metrics"][metric]
            allowance = 8 * 1024 * 1024 if metric.endswith("_bytes") else absolute_ms
            ceiling = old * (1 + relative) + allowance
            if value > ceiling:
                regressions.append({"book": book, "metric": metric, "baseline": old, "current": value, "ceiling": ceiling})
    return regressions


def collect(args):
    args.output.mkdir(parents=True, exist_ok=False)
    manifest_path = args.fixtures / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("schema") != 1 or not manifest.get("books"):
        raise ValueError("Invalid fixture manifest")
    summary = {
        "schema": 1, "kind": "shared-core", "passed": False,
        "environment": {"system": platform.system(), "os_release": platform.release(),
                        "architecture": platform.machine(), "device": args.device,
                        "cpu": platform.processor(), "logical_cpus": os.cpu_count(),
                        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(), "profile": "release"},
        "fixture_manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "probe_sha256": hashlib.sha256(args.probe.read_bytes()).hexdigest(), "books": {},
    }
    try:
        for book in manifest["books"]:
            path = args.fixtures / book["file"]
            if path.parent.resolve() != args.fixtures.resolve() or hashlib.sha256(path.read_bytes()).hexdigest() != book["sha256"]:
                raise ValueError("Invalid fixture path or fingerprint")
            samples = []
            for index in range(args.runs):
                result = subprocess.run([str(args.probe.resolve()), str(path.resolve())], capture_output=True, timeout=120, check=True)
                (args.output / f'{path.stem}-{index}.log').write_bytes(result.stderr)
                value = json.loads(result.stdout)
                (args.output / f'{path.stem}-{index}.json').write_bytes(result.stdout)
                samples.append(validate_sample(value, book))
            summary["books"][book["file"]] = {"samples": len(samples), "metrics": {key: statistics.median(s[key] for s in samples) for key in samples[0]}}
        summary["baseline_compared"] = bool(args.baseline)
        summary["regressions"] = compare(summary, json.loads(args.baseline.read_text())) if args.baseline else []
        if summary["regressions"]:
            raise ValueError(f'Performance regressions: {summary["regressions"]}')
        summary["passed"] = True
    finally:
        (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--device", required=True, help="Stable local hardware identifier, without a hostname")
    parser.add_argument("--runs", type=int, default=3, choices=range(3, 11))
    parser.add_argument("--baseline", type=Path)
    collect(parser.parse_args())
