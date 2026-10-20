#!/usr/bin/env python3
"""Regression tests for provenance, scenario completeness and baseline rejection."""
import copy
import importlib.util
import json
import hashlib
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("native", Path(__file__).with_name("check-native-performance.py"))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def report(client, chapters=3000):
    indices = native.targets(chapters)
    walked = native.walk_count(chapters)
    case = {"turn_total_ms": [300] * 8}
    value = {"harness_schema": 3, "profile": "release", "passed": True, "book_sha256": "hash", "chapter_count": chapters, "run_id": "current", "viewport": {"width": 800}, "walk_chapters": walked, "steady_memory_samples": [{"rss_bytes": 100} for _ in range(5)], "walk_memory_samples": [{"chapters": i, "rss_bytes": 100} for i in list(range(25, walked, 25)) + [walked]]}
    if client != "desktop":
        value["walk_memory_samples"] = [{"chapters": p["chapters"], "memory": {"rss_bytes": 100}} for p in value["walk_memory_samples"]]
    if client == "desktop":
        value.update(cache_stats=[min(8, chapters), max(208, walked), chapters], desktop_resident_chapters=min(3, chapters), first_content_visible_ms=100,
                     baseline_rss_bytes=None, first_content_rss_bytes=100, after_walk_rss_bytes=120, peak_rss_bytes=130)
        fields = {"chapter_first_visible_ms": 20, "frame_samples": 100, "update_cpu_p95_ms": 3, "update_cpu_max_ms": 5}
        prefix, modes, key = "frame_callback", ["Realistic", "Slide"], "cases"
    else:
        value.update(cache_stats={"resident": min(8, chapters), "body_loads": 208})
        for stage in ["baseline", "first_content", "after_walk"]:
            value[stage + "_memory"] = {"rss_bytes": 100}
        if client == "ios":
            value.update(initial_core_cache={"body_loads": 0}, swift_resident_bodies=min(32, chapters), first_content_layout_draw_ms=100,
                         import_or_existing_entry_ms=50, metadata_open_ms=10)
            fields = {"chapter_first_layout_draw_ms": 20, "display_samples": 100}
            prefix, modes, key = "display_interval", ["curl", "slide"], "cases"
        else:
            value.update(first_content_draw_ms=100, core_and_first_body_ms=50,
                         scroll_cases=[{"chapter": c, "extra_body_loads": 2, "resident": 8, "ready_ms": 25} for c in indices])
            fields = {"first_layout_draw_ms": 20, "frame_count": 100}
            prefix, modes, key = "frame_duration", ["Realistic", "Slide"], "layouts"
    fields.update({prefix + "_p50_ms": 16, prefix + "_p95_ms": 20, prefix + "_max_ms": 25})
    value[key] = [dict(case, **fields, mode=m, chapter=c) for m in modes for c in indices]
    return value


class NativePerformanceTests(unittest.TestCase):
    book = {"sha256": "hash", "chapters": 3000}

    def test_host_quietness_uses_measured_idle_and_requires_consecutive_samples(self):
        self.assertEqual(native.parse_host_idle("CPU usage: 1% user, 1% sys, 98% idle\nCPU usage: 60% user, 20% sys, 20% idle\n"), 20)
        for bad in ("Load Avg: 0.1", "CPU usage: 0% user, 0% sys, 90% idle",
                    "CPU usage: 0% user, 0% sys, 98% idle\nCPU usage: 0% user, 0% sys, -1% idle",
                    "CPU usage: 0% user, 0% sys, 98% idle\nCPU usage: 0% user, 0% sys, 101% idle"):
            with self.assertRaises(ValueError): native.parse_host_idle(bad)
        observations = []
        readings = [80]*9 + [10] + [90]*10
        with patch.object(native, "run", side_effect=[f"CPU usage: 0% user, 0% sys, 98% idle\nCPU usage: 0% user, 0% sys, {idle}% idle".encode() for idle in readings]):
            native.wait_host_idle(observations)
        self.assertEqual([o["idle_percent"] for o in observations], readings)
        with self.assertRaisesRegex(RuntimeError, "did not become quiet"):
            native.wait_host_idle([], timeout=0)

    def test_ios_launch_waits_before_each_process_and_retains_failed_preflight(self):
        with tempfile.TemporaryDirectory() as directory:
            driver=object.__new__(native.IOS)
            driver.args=SimpleNamespace(output=Path(directory))
            driver.identifier="owned-simulator";driver.launched=False;driver.host_idle_samples=[]
            events=[]
            def quiet(observations):
                events.append("quiet");observations.append({"idle_percent":80})
            def command(*args,**kwargs):events.append(args[2]);return b""
            with patch.object(native,"wait_host_idle",side_effect=quiet),patch.object(native,"run",side_effect=command):
                driver.launch("--measure-release-shelf")
                driver.launch("--measure-release-shelf")
            self.assertEqual(events,["quiet","launch","terminate","quiet","launch"])
            def busy(observations):
                observations.append({"idle_percent":2});raise RuntimeError("busy host")
            with patch.object(native,"wait_host_idle",side_effect=busy),patch.object(native,"run",side_effect=command):
                with self.assertRaisesRegex(RuntimeError,"busy host"):driver.launch("--measure-release-shelf")
            self.assertEqual(events[-1],"terminate")
            self.assertLess(driver.host_idle_samples[-1]["samples"][0]["idle_percent"],native.HOST_IDLE_MINIMUM)
            self.assertEqual(json.loads((Path(directory)/"host-idle-samples.json").read_text()),driver.host_idle_samples)

    def test_visible_endurance_requires_real_turns_origin_and_each_memory_point(self):
        for client in ["desktop", "android", "ios"]:
            value = report(client, 1)
            key = "layouts" if client == "android" else "cases"
            value["scenario"] = "visible-endurance"
            if client == "android":
                value["scroll_cases"] = []
            for case in value[key]:
                case.update(turn_total_ms=[300] * 128, returned_to_origin=True,
                            styled_resident=1,
                            turn_memory_samples=[{"turns": n, **({"rss_bytes": 100} if client == "desktop" else {"memory": {"rss_bytes": 100}})} for n in range(16, 129, 16)])
            book = {"sha256": "hash", "chapters": 1}
            native.normalize(value, client, book, "current", "visible-endurance")
            for change in [lambda v: v[key][0].__setitem__("returned_to_origin", False),
                           lambda v: v[key][0]["turn_memory_samples"].pop(),
                           lambda v: v[key][0]["turn_total_ms"].pop(),
                           lambda v: v.__setitem__("scenario", "reader")]:
                bad = copy.deepcopy(value)
                change(bad)
                with self.subTest(client=client), self.assertRaises(ValueError):
                    native.normalize(bad, client, book, "current", "visible-endurance")

    def test_pinned_build_accepts_only_a_verified_immutable_artifact(self):
        with tempfile.TemporaryDirectory() as root:
            artifact = Path(root) / "app"
            artifact.write_bytes(b"recorded artifact")
            files = {"reader.rs": "a" * 64}
            source = {"files": files, "sha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()}
            build = {"schema": native.SCHEMA, "source": source, "passed": True, "artifact_sha256": native.artifact_hash(artifact)}
            native.validate_build(build, source, artifact)
            with self.assertRaises(ValueError):
                native.validate_build(build, {"new workspace": True}, artifact)
            native.validate_build(build, {"new workspace": True}, artifact, pinned=True)
            for bad in [dict(build, passed=False), dict(build, schema=1), dict(build, source={"files": files, "sha256": "wrong"})]:
                with self.assertRaises(ValueError):
                    native.validate_build(bad, source, artifact, pinned=True)
            artifact.write_bytes(b"replaced after build")
            with self.assertRaises(ValueError):
                native.validate_build(build, source, artifact, pinned=True)

    def test_all_clients_reject_wrong_source_stale_runs_debug_failed_and_old_reports(self):
        for client in ["desktop", "android", "ios"]:
            self.assertGreater(len(native.normalize(report(client), client, self.book, "current")), 30)
            for field, bad in [("book_sha256", "wrong"), ("run_id", "stale"), ("chapter_count", 2999),
                               ("harness_schema", 2), ("profile", "debug"), ("passed", False), ("viewport", {})]:
                with self.subTest(client=client, field=field), self.assertRaises(ValueError):
                    native.normalize(dict(report(client), **{field: bad}), client, self.book, "current")

    def test_missing_duplicate_cases_incomplete_turns_or_bad_frames_are_rejected(self):
        for client in ["desktop", "android", "ios"]:
            key = "layouts" if client == "android" else "cases"
            changes = [lambda v: v[key].pop(), lambda v: v[key].__setitem__(0, v[key][1]),
                       lambda v: v[key][0].__setitem__("turn_total_ms", [300]),
                       lambda v: v[key][0]["turn_total_ms"].__setitem__(0, float("nan"))]
            for change in changes:
                value = report(client)
                change(value)
                with self.subTest(client=client, change=changes.index(change)), self.assertRaises(ValueError):
                    native.normalize(value, client, self.book, "current")

    def test_eager_loading_and_unbounded_caches_fail(self):
        for client in ["desktop", "android", "ios"]:
            value = report(client)
            if client == "desktop":
                value["cache_stats"][0] = 9
            else:
                value["cache_stats"]["resident"] = 9
            with self.subTest(client=client), self.assertRaises(ValueError):
                native.normalize(value, client, self.book, "current")
        for field, bad in [("initial_core_cache", {"body_loads": 1}), ("swift_resident_bodies", 33)]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                native.normalize(dict(report("ios"), **{field: bad}), "ios", self.book, "current")
        value = report("android")
        value["scroll_cases"][0]["extra_body_loads"] = 3000
        with self.assertRaises(ValueError):
            native.normalize(value, "android", self.book, "current")

    def test_short_and_single_chapter_books_have_complete_realistic_scenarios(self):
        for chapters in [1, 2, 24, 3000]:
            for client in ["desktop", "android", "ios"]:
                book = {"sha256": "hash", "chapters": chapters}
                value = report(client, chapters)
                with self.subTest(client=client, chapters=chapters):
                    native.normalize(value, client, book, "current")
                    value["walk_chapters"] += 1
                    with self.assertRaises(ValueError):
                        native.normalize(value, client, book, "current")

    def test_memory_sampling_must_cover_walk_and_post_walk_settling(self):
        for client in ["desktop", "android", "ios"]:
            for key in ["walk_memory_samples", "steady_memory_samples"]:
                value = report(client)
                value[key].pop()
                with self.subTest(client=client, key=key), self.assertRaises(ValueError):
                    native.normalize(value, client, self.book, "current")

    def test_native_baseline_keeps_platform_backend_viewport_and_coverage_separate(self):
        baseline = {"environment": {"client": "android", "graphics_backend": "host", "viewport": {"width": 800}},
                    "fixture_manifest_sha256": "hash", "books": {"many/cold": {"metrics": {"first_ms": 100, "retained_bytes": 100_000_000}}}}
        current = copy.deepcopy(baseline)
        current["books"]["many/cold"]["metrics"]["first_ms"] = 160
        self.assertEqual(len(native.core.compare(current, baseline)), 1)
        for field, bad in [("client", "ios"), ("graphics_backend", "swiftshader"), ("viewport", {"width": 400})]:
            current = copy.deepcopy(baseline)
            current["environment"][field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError):
                native.core.compare(current, baseline)

    def test_artifact_fingerprint_covers_resources_not_only_executable(self):
        with tempfile.TemporaryDirectory() as root:
            app = Path(root) / "app"
            app.mkdir()
            (app / "binary").write_bytes(b"same")
            (app / "resource").write_bytes(b"before")
            before = native.artifact_hash(app)
            (app / "resource").write_bytes(b"after")
            self.assertNotEqual(before, native.artifact_hash(app))

    def test_reader_binding_changes_require_a_new_compatible_harness_baseline(self):
        path="android/app/src/performance/java/com/epub/reader/ui/reader/ReaderPerformanceActivity.kt"
        build={"source":{"files":{path:"a"*64}}}
        first=native.harness_sources(build,"android","visible-endurance")
        self.assertEqual(first,{path:"a"*64})
        with self.assertRaises(ValueError):native.harness_sources(build,"ios","visible-endurance")
        baseline={"environment":{"harness_sources":first},"fixture_manifest_sha256":"hash","books":{"book/cold":{"metrics":{"first_ms":100}}}}
        changed=copy.deepcopy(baseline);changed["environment"]["harness_sources"][path]="b"*64
        with self.assertRaises(ValueError):native.core.compare(changed,baseline)


if __name__ == "__main__":
    unittest.main()
