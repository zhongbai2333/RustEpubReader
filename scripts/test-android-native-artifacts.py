#!/usr/bin/env python3
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
import warnings
import zipfile

spec=importlib.util.spec_from_file_location("native_artifacts",Path(__file__).with_name("android-native-artifacts.py"))
artifacts=importlib.util.module_from_spec(spec);spec.loader.exec_module(artifacts)


class NativeArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.addCleanup(self.temporary.cleanup)
        self.root=Path(self.temporary.name);self.target=self.root/"target";self.output=self.root/"jni";self.assets=self.root/"assets"
        for abi,triple in artifacts.TARGETS.items():
            for profile in ["release","release-fast"]:
                path=self.target/triple/profile/"libandroid_bridge.so";path.parent.mkdir(parents=True)
                path.write_bytes((abi+":"+profile).encode()*64)
    def publish(self,profile):return artifacts.publish(profile,self.target,self.output,self.assets)
    def apk(self,name,metadata,abis=None,replace=None):
        path=self.root/name
        with zipfile.ZipFile(path,"w") as archive:
            if metadata is not None:archive.writestr(artifacts.ASSET,json.dumps(metadata))
            for abi in (abis or artifacts.TARGETS):
                data=(self.output/abi/"libandroid_bridge.so").read_bytes()
                archive.writestr(f"lib/{abi}/libandroid_bridge.so",replace if replace is not None else data)
        return path
    def test_cached_profile_switch_uses_content_not_newer_destination_mtime(self):
        self.publish("release-fast")
        for abi,triple in artifacts.TARGETS.items():
            os.utime(self.target/triple/"release/libandroid_bridge.so",(100,100))
            os.utime(self.output/abi/"libandroid_bridge.so",(3000000000,3000000000))
        value=self.publish("release")
        self.assertEqual(value["profile"],"release")
        for abi,triple in artifacts.TARGETS.items():self.assertEqual((self.output/abi/"libandroid_bridge.so").read_bytes(),(self.target/triple/"release/libandroid_bridge.so").read_bytes())
        value=self.publish("release-fast")
        for abi in artifacts.TARGETS:self.assertEqual(artifacts.file_hash(self.output/abi/"libandroid_bridge.so"),value["libraries"][abi]["sha256"])
        self.assertEqual(json.loads((self.assets/"reader-native.json").read_text()),value)
    def test_identical_publication_preserves_cached_outputs(self):
        self.publish("release");files=[*self.output.glob("*/*.so"),self.assets/"reader-native.json"]
        before={str(path):path.stat().st_mtime_ns for path in files};self.publish("release")
        self.assertEqual(before,{str(path):path.stat().st_mtime_ns for path in files})
    def test_missing_abi_is_preflighted_before_publication(self):
        self.publish("release-fast");before={str(path):path.read_bytes() for path in [*self.output.glob("*/*.so"),self.assets/"reader-native.json"]}
        (self.target/artifacts.TARGETS["x86_64"]/"release/libandroid_bridge.so").unlink()
        with self.assertRaises(FileNotFoundError):self.publish("release")
        self.assertEqual(before,{str(Path(path)):Path(path).read_bytes() for path in before})
    def test_changed_source_cannot_overwrite_destination(self):
        value=self.publish("release");destination=self.output/"arm64-v8a/libandroid_bridge.so";destination.write_bytes(b"previous publication");before=destination.read_bytes()
        source=self.root/"changed.so";source.write_bytes(b"wrong")
        with self.assertRaises(ValueError):artifacts.atomic_copy(source,destination,value["libraries"]["arm64-v8a"])
        self.assertEqual(destination.read_bytes(),before);self.assertEqual(list(destination.parent.glob(".reader-rust-*")),[])
    def test_verified_apk_binds_actual_libraries_to_selected_cargo_profile(self):
        value=self.publish("release");path=self.apk("valid.apk",value)
        self.assertEqual(set(artifacts.verify_apk(path,"release",artifacts.describe("release",self.target))["packaged_abis"]),set(artifacts.TARGETS))
        with self.assertRaises(ValueError):artifacts.verify_apk(path,"release-fast")
        wrong=copy.deepcopy(value);wrong["libraries"]["x86_64"]["sha256"]="f"*64
        with self.assertRaises(ValueError):artifacts.verify_apk(path,"release",wrong)
        with self.assertRaises(ValueError):artifacts.verify_apk(self.apk("wrong-bytes.apk",value,replace=b"stale"),"release")
    def test_missing_duplicate_or_malformed_provenance_is_rejected(self):
        value=self.publish("release")
        with self.assertRaises(ValueError):artifacts.verify_apk(self.apk("missing.apk",None),"release")
        for change in [lambda x:x.update(schema=True),lambda x:x["libraries"].pop("x86_64"),lambda x:x["libraries"]["arm64-v8a"].update(bytes=True),lambda x:x["libraries"]["arm64-v8a"].update(sha256="x"*64)]:
            invalid=copy.deepcopy(value);change(invalid)
            with self.assertRaises(ValueError):artifacts.validate_metadata(invalid,"release")
        path=self.apk("duplicate.apk",value)
        with warnings.catch_warnings():
            warnings.simplefilter("ignore",UserWarning)
            with zipfile.ZipFile(path,"a") as archive:archive.writestr(artifacts.ASSET,json.dumps(value))
        with self.assertRaises(ValueError):artifacts.verify_apk(path,"release")
    def test_output_listing_cannot_escape_or_omit_an_abi(self):
        value=self.publish("release");self.apk("universal.apk",value)
        listing=self.root/"output-metadata.json";listing.write_text(json.dumps(dict(elements=[dict(outputFile="universal.apk")])))
        self.assertEqual(set(artifacts.verify_outputs(listing,"release",self.target)["verified_abis"]),set(artifacts.TARGETS))
        listing.write_text(json.dumps(dict(elements=[dict(outputFile="../outside.apk")])))
        with self.assertRaises(ValueError):artifacts.verify_outputs(listing,"release",self.target)
        self.apk("arm64.apk",value,abis=["arm64-v8a"]);listing.write_text(json.dumps(dict(elements=[dict(outputFile="arm64.apk")])))
        with self.assertRaises(ValueError):artifacts.verify_outputs(listing,"release",self.target)


if __name__=="__main__":unittest.main()
