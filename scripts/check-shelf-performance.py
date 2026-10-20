#!/usr/bin/env python3
"""Collect native Release library metrics using only isolated synthetic shelves."""
import argparse
import copy
import importlib.util
import json
import math
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

# Frame extrema and work counts remain visible diagnostics. A single missed
# refresh is not a stable process-level latency statistic. Gate its *budget*
# instead, including the extra slots consumed by a long stall.
IOS_COMPARISON_POLICY={"schema":1,"stable_metrics":"median; 25%+5ms or 25%+8MiB",
    "cadence":"pooled missed-slot share <= worst baseline run * 1.25 + one baseline refresh slot",
    "extrema_and_work_counts":"reported as diagnostics, not continuous-latency regressions"}
IOS_DIAGNOSTICS={"ranking_evaluations","geometry_updates","total_cover_requests","display_interval_max_ms","callback_interval_max_ms"}


def cadence_sample(value,clock):
    fps=native.checked_number(value["viewport"].get("maximum_frames_per_second"),"refresh rate",positive=True)
    if fps>1000:raise ValueError("Invalid native refresh rate")
    period=1000/fps;trace=value[clock+"_intervals_ms"]
    missed=sum(max(0,math.floor(interval/period+0.5)-1) for interval in trace if interval>period*1.5)
    return {"intervals":len(trace),"missed_slots":missed,"total_slots":len(trace)+missed}


def compare_ios(current,baseline):
    if current.get("comparison_policy")!=IOS_COMPARISON_POLICY or baseline.get("comparison_policy")!=IOS_COMPARISON_POLICY:
        raise ValueError("Different shelf comparison policy: explicitly reanalyse the retained raw collection")
    if baseline.get("passed") is not True or any(not isinstance(r.get("books"),dict) or len(r["books"])!=1 for r in [current,baseline]):
        raise ValueError("Missing successful shelf baseline or publication coverage")
    if next(iter(current["books"].values()))["samples"]!=next(iter(baseline["books"].values()))["samples"]:
        raise ValueError("Different process counts: collect the same number of shelf runs")
    all_changes=native.core.compare(current,baseline)  # Also checks environment/corpus/complete metric coverage.
    regressions=[r for r in all_changes if r["metric"] not in IOS_DIAGNOSTICS]
    warnings=[dict(r,reason="Observed extremum/work-count change; inspect raw traces") for r in all_changes if r["metric"] in IOS_DIAGNOSTICS]
    for clock in ["display","callback"]:
        groups=[]
        for report in [baseline,current]:
            samples=report.get("cadence_samples",{}).get(clock)
            expected=next(iter(report["books"].values()))["samples"]
            if not isinstance(samples,list) or len(samples)!=expected or not 3<=len(samples)<=10:raise ValueError("Missing per-process cadence coverage")
            for s in samples:
                if (not isinstance(s,dict) or any(type(s.get(k)) is not int for k in ["intervals","missed_slots","total_slots"]) or
                        not 100<=s["intervals"]<=100000 or s["missed_slots"]<0 or s["total_slots"]!=s["intervals"]+s["missed_slots"]):
                    raise ValueError("Invalid cadence slot counts")
            groups.append(samples)
        old,new=groups
        worst=max(s["missed_slots"]/s["total_slots"] for s in old)
        quantum=1/min(s["total_slots"] for s in old)
        share=sum(s["missed_slots"] for s in new)/sum(s["total_slots"] for s in new)
        ceiling=worst*1.25+quantum
        if share>ceiling:
            regressions.append({"book":next(iter(current["books"])),"metric":clock+"_missed_cadence_share","baseline_worst_run":worst,"current":share,"ceiling":ceiling})
    return regressions,warnings


def aggregate(samples):
    if any(set(s)!=set(samples[0]) for s in samples):raise ValueError("Shelf metric coverage changed")
    return {"samples":len(samples),"metrics":{name:statistics.median(s[name] for s in samples) for name in samples[0]}}


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
    if (value.get("passed") is not True or value.get("profile")!="release" or value.get("harness_schema")!=(3 if client=="ios" else 1) or
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
    if client=="ios":
        if value["frame_samples"]>100000:raise ValueError("Oversized shelf frame trace")
        for clock in ["display","callback"]:
            trace=value.get(clock+"_intervals_ms")
            if not isinstance(trace,list) or len(trace)!=value["frame_samples"]:raise ValueError("Incomplete shelf frame trace")
            ordered=sorted(native.checked_number(n,clock+" interval",positive=True) for n in trace)
            for quantile,index in [("p50",len(ordered)//2),("p95",len(ordered)*95//100),("max",len(ordered)-1)]:
                name=clock+"_interval_"+quantile+"_ms"
                metric(name,value.get(name))
                if not math.isclose(value[name],ordered[index],rel_tol=1e-9,abs_tol=1e-6):raise ValueError("Shelf frame summary does not match raw trace")
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
        directory=self.container/"Library/Caches"/("ShelfPerformanceLibrary-"+run_id)
        provision(directory,self.args.manifest,self.args.fixtures)
        path=self.container/"Library/Caches/shelf-performance.json";path.unlink(missing_ok=True)
        self.launch("--measure-release-shelf","--performance-run",run_id)
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
        report.update(source_sha256=build["source"]["sha256"],collection_source_initial_sha256=initial["sha256"],pinned_build=args.pinned_build,artifact_sha256=build["artifact_sha256"],build_command=build["command"],fixture_manifest_sha256=manifest_hash,collector_sha256=native.digest(Path(__file__)),collector_sources=native.collector_sources(Path(__file__)))
        driver={"desktop":Desktop,"android":Android,"ios":IOS}[args.platform](args)
        environment=dict(driver.environment,client=args.platform,device=args.device,scenario="dense-shelf",profile="release",harness_schema=3 if args.platform=="ios" else 1,build_command=build["command"],harness_sources=native.harness_sources(build,args.platform,"dense-shelf"))
        samples=[];viewport=None;cadence={clock:[] for clock in ["display","callback"]}
        for index in range(args.runs):
            run_id="shelf-"+str(index)+"-"+str(uuid.uuid4());value=driver.sample(index,run_id)
            (args.output/(run_id+".json")).write_text(json.dumps(value,indent=2)+"\n")
            samples.append(normalize(value,args.platform,args.manifest["count"],manifest_hash,run_id))
            if args.platform=="ios":
                for clock in cadence:cadence[clock].append(cadence_sample(value,clock))
            if viewport is not None and viewport!=value["viewport"]:raise ValueError("Shelf viewport changed during collection")
            viewport=value["viewport"]
        environment["viewport"]=viewport;report["environment"]=environment
        report["books"]["shelf-"+str(args.manifest["count"])]=aggregate(samples)
        report["baseline_compared"]=bool(args.baseline)
        if args.platform=="ios":
            report.update(comparison_policy=IOS_COMPARISON_POLICY,cadence_samples=cadence)
            report["regressions"],report["warnings"]=compare_ios(report,json.loads(args.baseline.read_text())) if args.baseline else ([],[])
        else:report["regressions"]=native.core.compare(report,json.loads(args.baseline.read_text())) if args.baseline else []
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


def reanalyse(args):
    """Explicit analysis migration; never overwrites or relabels a measured run."""
    args.output=args.output.resolve();args.output.mkdir(parents=True,exist_ok=False)
    result={"schema":1,"kind":"native-shelf","passed":False}
    try:
        directory=args.reanalyse.resolve();original_path=directory/"summary.json"
        original=json.loads(original_path.read_bytes());build=json.loads(args.build_record.read_bytes())
        native.validate_build(build,native.sources(),args.app.resolve(),args.pinned_build)
        if (original.get("kind")!="native-shelf" or original.get("reanalysis") or
                original.get("environment",{}).get("client")!="ios" or original["environment"].get("harness_schema")!=3 or
                original["environment"].get("device")!=args.device or
                original.get("artifact_sha256")!=build["artifact_sha256"] or original.get("source_sha256")!=build["source"]["sha256"] or
                original["environment"].get("harness_sources")!=native.harness_sources(build,"ios","dense-shelf") or
                not (original.get("passed") is True or original.get("passed") is False and original.get("error")=="Native shelf performance regression")):
            raise ValueError("Not a complete source-bound iOS shelf collection")
        if args.ios_device_type and args.ios_device_type!=original["environment"]["device_type"]:raise ValueError("Different requested iPad model")
        manifest=validate_manifest(args.fixtures.resolve());manifest_hash=native.digest(args.fixtures/"manifest.json")
        if manifest_hash!=original["fixture_manifest_sha256"]:raise ValueError("Different original corpus")
        paths=sorted(directory.glob("shelf-*.json"));key="shelf-"+str(manifest["count"])
        if not 3<=len(paths)<=10 or len(paths)!=args.runs or set(original["books"])!={key}:raise ValueError("Incomplete original shelf samples")
        samples=[];cadence={clock:[] for clock in ["display","callback"]};inputs=[]
        for path in paths:
            if not path.resolve().is_relative_to(directory) or path.stat().st_size>16*1024*1024:raise ValueError("Invalid shelf report path or size")
            value=json.loads(path.read_bytes())
            samples.append(normalize(value,"ios",manifest["count"],manifest_hash,path.stem))
            if value["viewport"]!=original["environment"]["viewport"]:raise ValueError("Original viewport changed")
            for clock in cadence:cadence[clock].append(cadence_sample(value,clock))
            inputs.append({"name":path.name,"sha256":native.digest(path)})
        measured=aggregate(samples)
        if measured!=original["books"][key]:raise ValueError("Raw samples do not reproduce the original summary")
        result=copy.deepcopy(original);result.pop("error",None)
        result.update(passed=False,comparison_policy=IOS_COMPARISON_POLICY,cadence_samples=cadence,
            reanalysis={"original_summary_sha256":native.digest(original_path),"original_collection_passed":original["passed"],
                "raw_reports":inputs,"analysis_sources":native.collector_sources(Path(__file__)),
                "revalidated_artifact_sha256":build["artifact_sha256"],
                "original_end_of_collection_source_check":original.get("source_changed_during_collection")},
            baseline_compared=bool(args.baseline))
        result["regressions"],result["warnings"]=compare_ios(result,json.loads(args.baseline.read_bytes())) if args.baseline else ([],[])
        if result["regressions"]:raise ValueError("Native shelf performance regression")
        result["passed"]=True
    except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError) as error:
        result["error"]=str(error);raise
    finally:(args.output/"summary.json").write_text(json.dumps(result,indent=2)+"\n")


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform",choices=["desktop","android","ios"])
    for name in ["app","fixtures","build-record","output"]:parser.add_argument("--"+name,type=Path,required=True)
    parser.add_argument("--device",required=True);parser.add_argument("--serial",default="emulator-5556")
    parser.add_argument("--ios-device-type",help="Exact compatible iPad Simulator type identifier")
    parser.add_argument("--runs",type=int,choices=range(3,11),default=3);parser.add_argument("--timeout",type=int,default=180)
    parser.add_argument("--baseline",type=Path);parser.add_argument("--pinned-build",action="store_true")
    parser.add_argument("--reanalyse",type=Path,help="Explicitly migrate retained iOS schema-3 raw samples to the current comparison policy; does not launch an app")
    args=parser.parse_args()
    if args.ios_device_type and args.platform!="ios":parser.error("--ios-device-type is only supported for iOS")
    if args.reanalyse and args.platform!="ios":parser.error("--reanalyse is only supported for iOS")
    reanalyse(args) if args.reanalyse else collect(args)
