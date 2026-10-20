#!/usr/bin/env python3
import copy
import hashlib
import os
import importlib.util
import json
from pathlib import Path
import tempfile
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
        if client=="ios":value.update(harness_schema=2,ranking_evaluations=8,geometry_updates=20)
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
                changes += [lambda v:v.__setitem__("harness_schema",1),lambda v:v.pop("ranking_evaluations"),lambda v:v.__setitem__("geometry_updates",True)]
            if client in ["desktop","android"]:changes.append(lambda v:v.__setitem__("cover_cache_count",97))
            for change in changes:
                bad=copy.deepcopy(value);change(bad)
                with self.subTest(client=client,change=changes.index(change)),self.assertRaises(ValueError):collector.normalize(bad,client,2000,"hash","current")

if __name__=="__main__":unittest.main()

