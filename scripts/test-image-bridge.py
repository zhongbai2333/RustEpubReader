#!/usr/bin/env python3
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location("image_bridge_checks",Path(__file__).with_name("check-image-bridge.py"))
checks=importlib.util.module_from_spec(spec);spec.loader.exec_module(checks)


def report():
    value=dict(passed=True,schema=2,run_id="owned-run",profile="release",physical_device=False,process_id=123,
               lifetime_checks=True,byte_and_metadata_parity=True,isolated_blank_host=True,release_network_checks=False,cases=[])
    for label,count in checks.COUNTS.items():
        value[label]=dict(sha256="a"*64,image_sha256=["b"*64]*count,prepare_and_first_legacy_chapter_ms=1.0,decoder_warmup_ms=2.0)
        for selected in (1,count):
            for iteration in range(7):
                for transport in ("base64","resource"):
                    row=dict(book=label,chapter=0 if label=="generated" else 7,image_count=count,selected=selected,
                             iteration=iteration,transport=transport,json_bytes=100,image_bytes=200,
                             dimensions=[[512,768] for _ in range(selected)])
                    row.update({key:1.0 for key in checks.METRICS});value["cases"].append(row)
    return value


class ImageBridgeChecks(unittest.TestCase):
    def validate(self,value):return checks.validate(value,"owned-run","release",dict(generated="a"*64,alice="a"*64))
    def test_complete_native_report(self):
        self.assertIs(self.validate(value:=report()),value)
        value["cases"].reverse();self.validate(value)
    def test_nonobject_envelopes_and_rows(self):
        for invalid in [None, [], True, "report"]:
            with self.assertRaises(ValueError):self.validate(invalid)
        data=report();data["cases"][0]=None
        with self.assertRaises(ValueError):self.validate(data)
        data=report();data["alice"]=[]
        with self.assertRaises(ValueError):self.validate(data)
    def test_failed_stale_or_nonisolated_reports_are_rejected(self):
        for key,value in [("passed",False),("run_id","stale"),("profile","debug"),("physical_device",True),
                          ("process_id",True),("process_id",0),("schema",1),("isolated_blank_host",False),("release_network_checks",True),("byte_and_metadata_parity",False),("lifetime_checks",False)]:
            with self.subTest(key=key,value=value):
                data=report();data[key]=value
                with self.assertRaises(ValueError):self.validate(data)
    def test_missing_duplicate_or_wrong_case(self):
        data=report();data["cases"].pop()
        with self.assertRaises(ValueError):self.validate(data)
        data=report();data["cases"][1]=copy.deepcopy(data["cases"][0])
        with self.assertRaises(ValueError):self.validate(data)
        for key,value in [("chapter",1),("image_count",11),("selected",True),("iteration",False),("transport","uri")]:
            data=report();data["cases"][0][key]=value
            with self.assertRaises(ValueError):self.validate(data)
    def test_cold_work_is_recorded_separately(self):
        for invalid in [None,-1,True,float("nan")]:
            data=report();data["generated"]["decoder_warmup_ms"]=invalid
            with self.assertRaises(ValueError):self.validate(data)
    def test_invalid_timings(self):
        for invalid in [float("nan"),float("inf"),-1,True,"1",60000]:
            data=report();data["cases"][0]["transfer_ms"]=invalid
            with self.assertRaises(ValueError):self.validate(data)
    def test_wrong_fingerprints_or_bounds(self):
        data=report();data["alice"]["sha256"]="c"*64
        with self.assertRaises(ValueError):self.validate(data)
        data=report();data["alice"]["image_sha256"]=["invalid"]*6
        with self.assertRaises(ValueError):self.validate(data)
        for key,value in [("json_bytes",0),("image_bytes",True),("dimensions",[]),("dimensions",[[0,768]]),("dimensions",[[True,768]])]:
            data=report();data["cases"][0][key]=value
            with self.assertRaises(ValueError):self.validate(data)
    def test_memory_observations_have_distinct_native_scope(self):
        for platform,metric,gc in [("android","pss_bytes",True),("ios","physical_footprint_bytes",False)]:
            data=dict(passed=True,schema=2,measurement="retained-native-chapter",run_id="memory",profile="release",
                      physical_device=False,isolated_blank_host=True,release_network_checks=False,process_id=123,
                      transport="resource",book_sha256="a"*64,image_count=12,chapter=0,bitmap_decode_measured=False,
                      probe_gc_requested=gc,json_bytes=100,baseline_memory={metric:1048576},steady_memory_samples=[{metric:1310720} for _ in range(5)])
            checks.validate_memory(data,"memory","release","a"*64,"resource",platform)
            for key,value in [("steady_memory_samples",[]),("baseline_memory",{metric:0}),("bitmap_decode_measured",True),
                              ("transport","base64"),("chapter",False),("image_count",13),("probe_gc_requested",not gc),("schema",1)]:
                invalid=copy.deepcopy(data);invalid[key]=value
                with self.assertRaises(ValueError):checks.validate_memory(invalid,"memory","release","a"*64,"resource",platform)
    def test_memory_summary_uses_process_medians(self):
        with tempfile.TemporaryDirectory() as directory:
            runs=[]
            for platform,metric in [("android","pss_bytes"),("ios","physical_footprint_bytes")]:
                for transport in ["base64","resource"]:
                    for index in range(3):
                        path=Path(directory)/f"{platform}-{transport}-{index}.json"
                        path.write_text(json.dumps(dict(baseline_memory={metric:100},steady_memory_samples=[{metric:110+index} for _ in range(5)])))
                        runs.append(dict(platform=platform,transport=transport,path=str(path)))
            for group in checks.summarize_memory(runs):
                self.assertEqual(group["samples"],3);self.assertEqual(group["steady_delta"],dict(median=11,min=10,max=12))
            self.assertEqual(len(checks.summarize_memory([run for run in runs if run["platform"]=="android"])),2)
    def test_steady_summary_excludes_first_iteration_without_best_of_selection(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"raw.json";data=report()
            for row in data["cases"]:
                row["total_ms"]=100 if row["iteration"]==0 else row["iteration"]
            path.write_text(json.dumps(data))
            runs=[dict(platform="ios",profile="release",path=str(path)) for _ in range(3)]
            all_groups=checks.summarize(runs);steady=checks.summarize(runs,True)
            self.assertEqual(len(all_groups),8);self.assertEqual(len(steady),8)
            for row in all_groups:self.assertEqual(row["samples"],21);self.assertEqual(row["total_ms"]["max"],100)
            for row in steady:self.assertEqual(row["samples"],18);self.assertEqual(row["total_ms"],dict(median=3.5,min=1,max=6))


if __name__=="__main__":unittest.main()
