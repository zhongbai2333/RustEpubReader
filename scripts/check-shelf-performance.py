#!/usr/bin/env python3
"""Collect native Release library metrics using only isolated synthetic shelves."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import time
import uuid

ROOT=Path(__file__).resolve().parents[1]
def module(name,file):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file))
    result=importlib.util.module_from_spec(spec);spec.loader.exec_module(result);return result
native=module("native_performance","check-native-performance.py")
shelf=module("shelf_fixtures","shelf-performance-fixtures.py")


def validate_manifest(directory):
    path=directory/"manifest.json"
    if path.stat().st_size>4*1024*1024:raise ValueError("Shelf manifest too large")
    value=json.loads(path.read_bytes())
    if value.get("schema")!=1 or value.get("scenario")!="dense-shelf" or type(value.get("count")) is not int or not 1<=value["count"]<=10000:
        raise ValueError("Unsupported shelf fixture")
    shelf.catalog(value,directory)  # IDs, exact relative paths, counts and uniqueness.
    for entry in value["books"]:
        source=(directory/entry["file"]).resolve()
        if (not source.is_relative_to(directory.resolve()) or not source.is_file() or
                type(entry.get("bytes")) is not int or not 1<=entry["bytes"]<=1024*1024 or
                source.stat().st_size!=entry["bytes"] or native.digest(source)!=entry.get("sha256") or
                not isinstance(entry.get("title"),str) or len(entry["title"].encode())>4096):
            raise ValueError("Invalid or changed synthetic shelf publication")
    return value


def provision(directory,manifest,source):
    """Only new diagnostic directories; hard links never share mutable JSON state."""
    directory.mkdir(parents=True,exist_ok=False)
    (directory/"books").mkdir()
    for entry in manifest["books"]:
        target=directory/entry["file"]
        try:os.link(source/entry["file"],target)
        except OSError:shutil.copyfile(source/entry["file"],target)
    catalog,configs=shelf.catalog(manifest,directory)
    (directory/"library.json").write_text(json.dumps(catalog,ensure_ascii=False))
    for identity,config in configs.items():(directory/"books"/(identity+".json")).write_text(json.dumps(config,ensure_ascii=False))
    shutil.copyfile(source/"manifest.json",directory/"manifest.json")
    (directory/"shelf-performance.fixture.json").write_text(json.dumps({"schema":1,"isolated":True,"fixture_manifest_sha256":native.digest(source/"manifest.json")}))


def normalize(value,client,count,manifest_hash,run_id):
    if (value.get("passed") is not True or value.get("profile")!="release" or value.get("harness_schema")!=(2 if client=="ios" else 1) or
            value.get("scenario")!="dense-shelf" or value.get("run_id")!=run_id or type(value.get("book_count")) is not int or value.get("book_count")!=count or
            value.get("fixture_manifest_sha256")!=manifest_hash or not isinstance(value.get("viewport"),dict) or not value["viewport"]):
        raise ValueError("Wrong or failed native shelf report")
    result={}
    def metric(name,number,positive=True):result[name]=native.checked_number(number,name,positive=positive)
    if client=="ios":
        for name in ["ranking_evaluations","geometry_updates"]:
            if type(value.get(name)) is not int or value[name]<1:raise ValueError("Missing shelf invalidation counters")
            metric(name,value[name])
    metadata="metadata_first_layout_ms"
    metric(metadata,value.get(metadata));metric("first_covers_ready_ms",value.get("first_covers_ready_ms"))
    if value["first_covers_ready_ms"]<value[metadata]:raise ValueError("Invalid native readiness order")
    first=value.get("initial_cover_requests");total=value.get("total_cover_requests")
    if (type(first) is not int or type(total) is not int or not 0<=first<=total<=count or total<min(count,12) or
            count>96 and (first>=min(count,1000) or total>=count)):
        raise ValueError("Missing visible covers or eager whole-shelf loading")
    if client in ["android","desktop"]:
        limit=96 if client=="android" else 64
        if type(value.get("cover_cache_count")) is not int or not 1<=value["cover_cache_count"]<=limit:raise ValueError("Unbounded native cover cache")
        metric("cover_cache_count",value["cover_cache_count"])
    if client=="desktop":
        if type(value.get("pending")) is not int or not 0<=value["pending"]<=8:raise ValueError("Unbounded cover queue")
        for name in ["baseline_rss_bytes","first_metadata_rss_bytes","peak_rss_bytes","update_cpu_p95_ms","update_cpu_max_ms"]:metric(name,value.get(name))
    else:
        for stage in ["baseline","first_metadata"]:
            counters=value.get(stage+"_memory",{})
            if not counters or not any(n is not None for n in counters.values()):raise ValueError("Missing shelf memory")
            for key,number in counters.items():
                if number is not None:metric(stage+"_"+key,number)
    number="swipes" if client=="android" else "scrolls"
    if value.get(number)!=30 or type(value.get("frame_samples")) is not int or value["frame_samples"]<100:raise ValueError("Incomplete native scrolling")
    prefix={"desktop":"frame_callback","android":"frame_duration","ios":"display_interval"}[client]
    for quantile in ["p50","p95","max"]:metric(prefix+"_"+quantile+"_ms",value.get(prefix+"_"+quantile+"_ms"))
    if not value[prefix+"_p50_ms"]<=value[prefix+"_p95_ms"]<=value[prefix+"_max_ms"]:raise ValueError("Invalid native frame quantiles")
    points=value.get("scroll_memory_samples",[])
    if [p.get(number) for p in points]!=list(range(5,31,5)):raise ValueError("Incomplete scrolling memory curve")
    counters={}
    for point in points:
        values={"rss_bytes":point.get("rss_bytes")} if client=="desktop" else point.get("memory",{})
        if not values or not any(v is not None for v in values.values()):raise ValueError("No scrolling memory measurement")
        for key,amount in values.items():
            if amount is not None:counters.setdefault(key,[]).append(native.checked_number(amount,key,positive=True))
    for key,numbers in counters.items():
        if len(numbers)!=6:raise ValueError("Memory counter coverage changed")
        metric("scroll_sampled_max_"+key,max(numbers))
    samples=value.get("steady_memory_samples",[])
    if len(samples)!=5 or not samples[0] or any(set(s)!=set(samples[0]) for s in samples):raise ValueError("Missing steady shelf memory")
    for name in samples[0]:metric("steady_"+name,statistics.median(native.checked_number(s[name],name,positive=True) for s in samples))
    metric("total_cover_requests",total)
    return result


class Desktop(native.Desktop):
    def sample(self,index,run_id):
        directory=self.args.output/("ReaderShelfPerformance-"+str(index));provision(directory,self.args.manifest,self.args.fixtures)
        report=self.args.output/(run_id+".json")
        env=dict(os.environ,READER_SHELF_REPORT=str(report),READER_PERFORMANCE_RUN=run_id)
        with (self.args.output/(run_id+".log")).open("wb") as log:
            process=subprocess.Popen([str(self.exe),"--measure-shelf"],cwd=directory,env=env,stdout=log,stderr=log)
            try:return native.wait_json(report,self.args.timeout,process)
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:process.wait(timeout=10)
                    except subprocess.TimeoutExpired:process.kill();process.wait(timeout=10)


class Android(native.Android):
    def sample(self,index,run_id):
        self.device("shell","am","force-stop",native.PACKAGE)
        self.device("shell","pm","clear",native.PACKAGE)
        remote=self.external+"/dense-shelf";self.device("shell","mkdir","-p",remote)
        self.device("push",self.args.fixtures/"manifest.json",remote+"/manifest.json")
        self.device("push",self.args.fixtures/"books",remote+"/books")
        self.device("shell","am","start","-W","-n",native.PACKAGE+"/com.zhongbai233.epub.reader.ui.reader.ShelfPerformanceActivity","--es","performance_run",run_id)
        path=self.external+"/shelf-performance.json";deadline=time.monotonic()+self.args.timeout
        while time.monotonic()<deadline:
            result=subprocess.run([str(self.adb),"-s",self.args.serial,"exec-out","cat",path],capture_output=True,timeout=15)
            if result.returncode==0:
                try:
                    value=json.loads(result.stdout)
                    (self.args.output/(run_id+"-shelf-scrolled.png")).write_bytes(self.device("exec-out","screencap","-p"))
                    return value
                except json.JSONDecodeError:pass
            time.sleep(.3)
        raise TimeoutError("Native shelf report not produced")


class IOS(native.IOS):
    def __init__(self,args):super().__init__(args,reader_fixture=False)
    def sample(self,index,run_id):
        subprocess.run(["xcrun","simctl","terminate",self.identifier,native.PACKAGE],capture_output=True,timeout=20)
        directory=self.container/"Library/Caches"/("ShelfPerformanceLibrary-"+run_id)
        provision(directory,self.args.manifest,self.args.fixtures)
        path=self.container/"Library/Caches/shelf-performance.json";path.unlink(missing_ok=True)
        native.run("xcrun","simctl","launch",self.identifier,native.PACKAGE,"--measure-release-shelf","--performance-run",run_id)
        value=native.wait_json(path,self.args.timeout)
        for image in directory.glob("shelf-*.png"):shutil.copyfile(image,self.args.output/(run_id+"-"+image.name))
        return value


def collect(args):
    args.app=args.app.resolve();args.fixtures=args.fixtures.resolve();args.output=args.output.resolve()
    args.output.mkdir(parents=True,exist_ok=False);driver=None
    report={"schema":1,"kind":"native-shelf","passed":False,"books":{},"unmeasured":["physical_device","direct_gpu_present_latency","keyboard_input_latency"]}
    try:
        initial=native.sources();build=json.loads(args.build_record.read_bytes());native.validate_build(build,initial,args.app,args.pinned_build)
        args.manifest=validate_manifest(args.fixtures);manifest_hash=native.digest(args.fixtures/"manifest.json")
        report.update(source_sha256=build["source"]["sha256"],collection_source_initial_sha256=initial["sha256"],pinned_build=args.pinned_build,artifact_sha256=build["artifact_sha256"],build_command=build["command"],fixture_manifest_sha256=manifest_hash,collector_sha256=native.digest(Path(__file__)))
        driver={"desktop":Desktop,"android":Android,"ios":IOS}[args.platform](args)
        environment=dict(driver.environment,client=args.platform,device=args.device,scenario="dense-shelf",profile="release",harness_schema=2 if args.platform=="ios" else 1,build_command=build["command"],harness_sources=native.harness_sources(build,args.platform,"dense-shelf"))
        samples=[];viewport=None
        for index in range(args.runs):
            run_id="shelf-"+str(index)+"-"+str(uuid.uuid4());value=driver.sample(index,run_id)
            (args.output/(run_id+".json")).write_text(json.dumps(value,indent=2)+"\n")
            samples.append(normalize(value,args.platform,args.manifest["count"],manifest_hash,run_id))
            if viewport is not None and viewport!=value["viewport"]:raise ValueError("Shelf viewport changed during collection")
            viewport=value["viewport"]
        if any(set(s)!=set(samples[0]) for s in samples):raise ValueError("Shelf metric coverage changed")
        environment["viewport"]=viewport;report["environment"]=environment
        report["books"]["shelf-"+str(args.manifest["count"])]=dict(samples=len(samples),metrics={name:statistics.median(s[name] for s in samples) for name in samples[0]})
        report["baseline_compared"]=bool(args.baseline)
        report["regressions"]=native.core.compare(report,json.loads(args.baseline.read_text())) if args.baseline else []
        if report["regressions"]:raise ValueError("Native shelf performance regression")
        final=native.sources();report["collection_source_final_sha256"]=final["sha256"];report["source_changed_during_collection"]=initial!=final
        if (not args.pinned_build and initial!=final) or native.artifact_hash(args.app)!=report["artifact_sha256"]:raise ValueError("Shelf build changed during collection")
        report["passed"]=True
    except (OSError,ValueError,KeyError,RuntimeError,TimeoutError,subprocess.SubprocessError) as error:
        report["error"]=str(error);raise
    finally:
        try:
            if driver:driver.close()
        finally:(args.output/"summary.json").write_text(json.dumps(report,indent=2)+"\n")


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform",choices=["desktop","android","ios"])
    for name in ["app","fixtures","build-record","output"]:parser.add_argument("--"+name,type=Path,required=True)
    parser.add_argument("--device",required=True);parser.add_argument("--serial",default="emulator-5556")
    parser.add_argument("--runs",type=int,choices=range(3,11),default=3);parser.add_argument("--timeout",type=int,default=180)
    parser.add_argument("--baseline",type=Path);parser.add_argument("--pinned-build",action="store_true")
    collect(parser.parse_args())
