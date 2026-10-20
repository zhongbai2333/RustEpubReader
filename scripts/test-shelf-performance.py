#!/usr/bin/env python3
import copy
import hashlib
import os
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
import zipfile

spec=importlib.util.spec_from_file_location("shelf",Path(__file__).with_name("shelf-performance-fixtures.py"))
shelf=importlib.util.module_from_spec(spec);spec.loader.exec_module(shelf)
spec2=importlib.util.spec_from_file_location("collector",Path(__file__).with_name("check-shelf-performance.py"))
collector=importlib.util.module_from_spec(spec2);spec2.loader.exec_module(collector)

class ShelfFixtureTests(unittest.TestCase):
    def test_reproducible_unique_real_covers_and_valid_portable_catalog(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            first=shelf.generate(root/"first",3); second=shelf.generate(root/"second",3)
            self.assertEqual(first,second)
            covers=[]
            for entry in first["books"]:
                path=root/"first"/entry["file"]
                self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(),entry["sha256"])
                with zipfile.ZipFile(path) as archive:
                    self.assertEqual(archive.read("mimetype"),b"application/epub+zip")
                    covers.append(archive.read("OEBPS/cover.png"))
            self.assertEqual(len(set(covers)),3)
            self.assertTrue(all(len(image)>50000 for image in covers))
            catalog,configs=shelf.catalog(first,root/"installed")
            self.assertEqual(len(catalog["books"]),3)
            self.assertEqual(len(configs),3)
            self.assertTrue(all(Path(book["path"]).is_relative_to((root/"installed").resolve()) for book in catalog["books"]))
            self.assertEqual(catalog["books"][0]["last_opened"],1700000000000)
            with self.assertRaises(FileExistsError):shelf.generate(root/"first",3)
    def test_duplicates_paths_counts_and_excessive_generation_are_rejected(self):
        entry={"id":"00000000-0000-4000-8000-000000000001","title":"test","file":"../../other.epub"}
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for manifest in [{"books":[entry],"count":1}, {"books":[],"count":1}]:
                with self.assertRaises(ValueError):shelf.catalog(manifest,root)
            entry["file"]="books/"+entry["id"]+".epub"
            with self.assertRaises(ValueError):shelf.catalog({"books":[entry,entry],"count":2},root)
            for count in [0,10001]:
                with self.assertRaises(ValueError):shelf.generate(root/str(count),count)

    def test_installation_has_separate_mutable_metadata_and_rejects_modified_or_escaped_assets(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);manifest=shelf.generate(root/"assets",2)
            collector.validate_manifest(root/"assets")
            collector.provision(root/"installed",manifest,root/"assets")
            entry=manifest["books"][0]
            original=(root/"assets"/entry["file"]).read_bytes()
            (root/"installed/books"/(entry["id"]+".json")).write_text("new progress")
            self.assertEqual((root/"assets"/entry["file"]).read_bytes(),original)
            with self.assertRaises(FileExistsError):collector.provision(root/"installed",manifest,root/"assets")
            source=root/"assets"/entry["file"]
            source.rename(root/"outside.epub");source.symlink_to(root/"outside.epub")
            with self.assertRaises(ValueError):collector.validate_manifest(root/"assets")
            source.unlink();source.write_bytes(original+b"changed")
            with self.assertRaises(ValueError):collector.validate_manifest(root/"assets")

class ShelfCollectorTests(unittest.TestCase):
    def report(self,client):
        key="swipes" if client=="android" else "scrolls"
        value={"passed":True,"profile":"release","harness_schema":1,"scenario":"dense-shelf","run_id":"current","book_count":2000,"fixture_manifest_sha256":"hash","viewport":{"width":800},"metadata_first_layout_ms":100,"first_covers_ready_ms":300,"initial_cover_requests":12,"total_cover_requests":200,"cover_cache_count":64,"frame_samples":500,key:30,"pending":2,"scroll_memory_samples":[{key:n,**({"rss_bytes":100} if client=="desktop" else {"memory":{"rss_bytes":100}})} for n in range(5,31,5)],"steady_memory_samples":[{"rss_bytes":100} for _ in range(5)]}
        if client=="ios":value.update(harness_schema=3,ranking_evaluations=8,geometry_updates=20,viewport={"width":800,"maximum_frames_per_second":60},
            display_intervals_ms=[16]*300+[20]*180+[25]*20,
            callback_intervals_ms=[18]*300+[22]*180+[30]*20,
            callback_interval_p50_ms=18,callback_interval_p95_ms=22,callback_interval_max_ms=30)
        if client=="desktop":
            value.update(baseline_rss_bytes=100,first_metadata_rss_bytes=120,peak_rss_bytes=140,update_cpu_p95_ms=3,update_cpu_max_ms=5)
        else:value.update(baseline_memory={"rss_bytes":100},first_metadata_memory={"rss_bytes":120})
        prefix={"desktop":"frame_callback","android":"frame_duration","ios":"display_interval"}[client]
        value.update({prefix+"_p50_ms":16,prefix+"_p95_ms":20,prefix+"_max_ms":25})
        return value
    def test_real_scenario_identity_bounds_frames_and_complete_memory_are_required(self):
        for client in ["desktop","android","ios"]:
            value=self.report(client)
            metrics=collector.normalize(value,client,2000,"hash","current")
            self.assertEqual(metrics["metadata_first_layout_ms"],100)
            self.assertEqual(metrics["scroll_sampled_max_rss_bytes"],100)
            self.assertEqual(metrics["steady_rss_bytes"],100)
            changes=[lambda v:v.__setitem__("run_id","stale"),lambda v:v.__setitem__("fixture_manifest_sha256","wrong"),lambda v:v.__setitem__("scenario","reader"),lambda v:v.__setitem__("profile","debug"),lambda v:v.__setitem__("passed",False),lambda v:v.__setitem__("total_cover_requests",2000),lambda v:v.__setitem__("initial_cover_requests",1500),lambda v:v.__setitem__("frame_samples",3),lambda v:v["scroll_memory_samples"].pop(),lambda v:v["steady_memory_samples"].pop(),lambda v:v["steady_memory_samples"][0].__setitem__("rss_bytes",float("nan"))]
            if client=="ios":
                changes += [lambda v:v.__setitem__("harness_schema",2),lambda v:v.pop("ranking_evaluations"),lambda v:v.__setitem__("geometry_updates",True)]
            if client in ["desktop","android"]:changes.append(lambda v:v.__setitem__("cover_cache_count",97))
            for change in changes:
                bad=copy.deepcopy(value);change(bad)
                with self.subTest(client=client,change=changes.index(change)),self.assertRaises(ValueError):collector.normalize(bad,client,2000,"hash","current")

    def test_ios_requires_both_clock_traces_and_recomputes_their_quantiles(self):
        value=self.report("ios")
        self.assertEqual(collector.normalize(value,"ios",2000,"hash","current")["callback_interval_max_ms"],30)
        changes=[lambda v:v.pop("callback_intervals_ms"),
                 lambda v:v["callback_intervals_ms"].pop(),
                 lambda v:v["display_intervals_ms"].__setitem__(0,float("nan")),
                 lambda v:v["callback_intervals_ms"].__setitem__(0,-1),
                 lambda v:v.__setitem__("callback_interval_p95_ms",18),
                 lambda v:v.__setitem__("display_interval_max_ms",24)]
        for change in changes:
            bad=copy.deepcopy(value);change(bad)
            with self.subTest(change=changes.index(change)),self.assertRaises(ValueError):collector.normalize(bad,"ios",2000,"hash","current")

    def comparison_report(self,misses=0):
        sample={"intervals":800,"missed_slots":misses,"total_slots":800+misses}
        return {"passed":True,"environment":{"same":"device"},"fixture_manifest_sha256":"same",
            "comparison_policy":copy.deepcopy(collector.IOS_COMPARISON_POLICY),
            "cadence_samples":{c:[copy.deepcopy(sample) for _ in range(3)] for c in ["display","callback"]},
            "books":{"shelf-2000":{"samples":3,"metrics":{"metadata_first_layout_ms":100,"display_interval_p95_ms":16.7,
                "display_interval_max_ms":17,"callback_interval_max_ms":17,"geometry_updates":500,"steady_resident_bytes":100000000}}}}

    def test_cadence_counts_duration_of_stalls_instead_of_only_number_of_events(self):
        value=self.report("ios");value["callback_intervals_ms"]=[1000/60]*497+[1000/30,1000/30,1000/12]
        self.assertEqual(collector.cadence_sample(value,"callback"),{"intervals":500,"missed_slots":6,"total_slots":506})

    def test_isolated_extremum_is_visible_but_sustained_latency_memory_and_cadence_still_fail(self):
        base=self.comparison_report();current=copy.deepcopy(base)
        metrics=current["books"]["shelf-2000"]["metrics"]
        metrics["display_interval_max_ms"]=34;metrics["geometry_updates"]=800
        current["cadence_samples"]["display"][0].update(missed_slots=1,total_slots=801)
        regressions,warnings=collector.compare_ios(current,base)
        self.assertEqual(regressions,[]);self.assertEqual({w["metric"] for w in warnings},{"display_interval_max_ms","geometry_updates"})
        for key,value in [("display_interval_p95_ms",40),("metadata_first_layout_ms",150),("steady_resident_bytes",160000000)]:
            bad=copy.deepcopy(current);bad["books"]["shelf-2000"]["metrics"][key]=value
            self.assertIn(key,{r["metric"] for r in collector.compare_ios(bad,base)[0]})
        bad=self.comparison_report(misses=5)
        self.assertEqual({r["metric"] for r in collector.compare_ios(bad,base)[0]}, {"display_missed_cadence_share","callback_missed_cadence_share"})
        # A single long stall cannot disappear in a median of per-run maxima.
        bad=self.comparison_report();bad["cadence_samples"]["display"][0].update(missed_slots=60,total_slots=860)
        self.assertEqual(collector.compare_ios(bad,base)[0][0]["metric"],"display_missed_cadence_share")

    def test_cadence_policy_coverage_and_counts_cannot_silently_fall_back(self):
        base=self.comparison_report()
        changes=[lambda v:v.pop("comparison_policy"),lambda v:v.pop("cadence_samples"),
            lambda v:v["books"]["shelf-2000"].__setitem__("samples",6),
            lambda v:v["cadence_samples"]["callback"].pop(),
            lambda v:v["cadence_samples"]["display"][0].__setitem__("missed_slots",-1),
            lambda v:v["cadence_samples"]["display"][0].__setitem__("total_slots",900)]
        for change in changes:
            bad=copy.deepcopy(base);change(bad)
            with self.subTest(change=changes.index(change)),self.assertRaises(ValueError):collector.compare_ios(bad,base)

    def test_rare_second_long_stall_in_valid_raw_trace_is_not_hidden_by_unchanged_p95(self):
        base=self.comparison_report();candidate=copy.deepcopy(base)
        value=self.report("ios")
        for clock in ["display","callback"]:
            value[clock+"_intervals_ms"]=[1000/60]*500
            for q in ["p50","p95","max"]:value[clock+"_interval_"+q+"_ms"]=1000/60
        for clock in ["display","callback"]:
            value[clock+"_intervals_ms"][0]=1000
            value[clock+"_interval_max_ms"]=1000
        collector.normalize(value,"ios",2000,"hash","current")
        for clock in ["display","callback"]:
            candidate["cadence_samples"][clock][0]=collector.cadence_sample(value,clock)
        self.assertEqual(value["callback_interval_p95_ms"],1000/60)
        self.assertEqual({r["metric"] for r in collector.compare_ios(candidate,base)[0]}, {"display_missed_cadence_share","callback_missed_cadence_share"})

    def test_explicit_reanalysis_preserves_input_and_rejects_trace_or_artifact_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);assets=root/"fixtures";manifest=shelf.generate(assets,3);source=root/"measured";source.mkdir()
            app=root/"fake-app";app.write_bytes(b"isolated-unit-artifact")
            files={"ios/Tests/ReadingReleasePerformance.swift":"0"*64};fingerprint=hashlib.sha256(json.dumps(files,sort_keys=True).encode()).hexdigest()
            build={"schema":3,"passed":True,"source":{"sha256":fingerprint,"files":files},"artifact_sha256":collector.native.artifact_hash(app)}
            build_path=root/"build.json";build_path.write_text(json.dumps(build));manifest_hash=collector.native.digest(assets/"manifest.json")
            samples=[]
            for i in range(3):
                value=self.report("ios");value.update(run_id=f"shelf-{i}",book_count=3,fixture_manifest_sha256=manifest_hash,initial_cover_requests=3,total_cover_requests=3)
                (source/f"shelf-{i}.json").write_text(json.dumps(value));samples.append(collector.normalize(value,"ios",3,manifest_hash,f"shelf-{i}"))
            original={"schema":1,"kind":"native-shelf","passed":True,"source_sha256":fingerprint,"artifact_sha256":build["artifact_sha256"],
                "fixture_manifest_sha256":manifest_hash,"environment":{"client":"ios","harness_schema":3,"device":"owned-unit","viewport":value["viewport"],"harness_sources":files},
                "books":{"shelf-3":collector.aggregate(samples)},"source_changed_during_collection":False}
            path=source/"summary.json";path.write_text(json.dumps(original));unchanged=path.read_bytes()
            def arguments(name):return SimpleNamespace(output=root/name,reanalyse=source,build_record=build_path,app=app,pinned_build=True,device="owned-unit",ios_device_type=None,fixtures=assets,runs=3,baseline=None)
            collector.reanalyse(arguments("accepted"));result=json.loads((root/"accepted/summary.json").read_text())
            self.assertTrue(result["passed"]);self.assertTrue(result["reanalysis"]["original_collection_passed"]);self.assertEqual(path.read_bytes(),unchanged)
            raw=source/"shelf-0.json";original_raw=raw.read_bytes();bad=json.loads(original_raw);bad["callback_intervals_ms"][0]=-1;raw.write_text(json.dumps(bad))
            with self.assertRaises(ValueError):collector.reanalyse(arguments("bad-trace"))
            self.assertFalse(json.loads((root/"bad-trace/summary.json").read_text())["passed"]);raw.write_bytes(original_raw)
            app.write_bytes(b"changed-artifact")
            with self.assertRaises(ValueError):collector.reanalyse(arguments("bad-app"))
            self.assertEqual(path.read_bytes(),unchanged)

if __name__=="__main__":unittest.main()

