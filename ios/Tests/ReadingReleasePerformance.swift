#if READER_PERFORMANCE_CHECK
import Foundation
import UIKit
import QuartzCore

@MainActor private final class ReadingFrameMeter:NSObject {
    var intervals:[Double]=[]
    var measuring=false {didSet {last=nil}}
    private var last:CFTimeInterval?
    var link:CADisplayLink?
    func start() {let value=CADisplayLink(target:self,selector:#selector(tick(_:)));value.add(to:.main,forMode:.common);link=value}
    @objc func tick(_ sender:CADisplayLink) {guard measuring else{return};if let last {intervals.append((sender.timestamp-last)*1000)};last=sender.timestamp}
    func stop(){link?.invalidate();link=nil}
}
@MainActor enum ReadingReleasePerformance {
    static func memory()->[String:UInt64] {
        var info=task_vm_info_data_t();var count=mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size/MemoryLayout<natural_t>.size)
        let result=withUnsafeMutablePointer(to:&info) {value in value.withMemoryRebound(to:integer_t.self,capacity:Int(count)) {task_info(mach_task_self_,task_flavor_t(TASK_VM_INFO),$0,&count)}}
        return result==KERN_SUCCESS ? ["resident_bytes":UInt64(info.resident_size),"physical_footprint_bytes":UInt64(info.phys_footprint)] : [:]
    }
    static func run() {
        Task { @MainActor in
            let files=FileManager.default,caches=files.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            var report:[String:Any]=["profile":"release","environment":"iOS simulator; production UIKit/CoreText reader"]
            let meter=ReadingFrameMeter();var checkWindow:UIWindow?
            do {
                for _ in 0..<100 {if UIApplication.shared.connectedScenes.contains(where:{$0 is UIWindowScene}) {break};try await Task.sleep(for:.milliseconds(20))}
                guard let scene=UIApplication.shared.connectedScenes.first(where:{$0 is UIWindowScene}) as? UIWindowScene else {throw CoreFailure(message:"Missing window scene")}
                let arguments=CommandLine.arguments
                let runIndex=arguments.firstIndex(of:"--performance-run")
                let requested=runIndex.flatMap {arguments.indices.contains($0+1) ? arguments[$0+1] : nil} ?? "default"
                let run=String(requested.filter {$0.isASCII && ($0.isLetter || $0.isNumber || $0=="-")}.prefix(64))
                let root=caches.appendingPathComponent("ReleasePerformanceLibrary-"+run)
                let client=CoreClient(root:root.path,device:"release-performance")
                let source=files.urls(for:.documentDirectory,in:.userDomainMask)[0].appendingPathComponent("release-performance.epub")
                report["baseline_memory"]=memory()
                let start=CACurrentMediaTime()
                let entry:LibraryEntry
                if CommandLine.arguments.contains("--performance-reopen") {
                    let entries:[LibraryEntry]=try client.call("library");guard let first=entries.first else {throw CoreFailure(message:"Missing imported benchmark book")};entry=first
                } else {entry=try await Task.detached {try client.call("import",["path":source.path],as:LibraryEntry.self)}.value}
                report["import_or_existing_entry_ms"]=(CACurrentMediaTime()-start)*1000
                let openStart=CACurrentMediaTime()
                let book:PreviewBook=try await Task.detached {try client.call("open",["id":entry.id],as:PreviewBook.self)}.value
                report["metadata_open_ms"]=(CACurrentMediaTime()-openStart)*1000
                let initialStats:[String:Int]=try client.call("chapter_cache_stats",["id":entry.id]);report["initial_core_cache"]=initialStats
                guard initialStats["body_loads"]==0 else {throw CoreFailure(message:"Open eagerly loaded chapter bodies")}
                let host=UIViewController(),window=UIWindow(windowScene:scene);checkWindow=window;window.rootViewController=host;window.makeKeyAndVisible()
                var options=ReaderOptions(fontSize:18,lineHeight:1.65,fontName:"Helvetica",theme:"light",sliding:false,cjkFontName:"PingFangSC-Regular",readingMode:.curl)
                let reader=NativeReadingController(book:book,anchor:.init(chapter:0),options:options)
                host.addChild(reader);host.view.addSubview(reader.view);reader.didMove(toParent:host)
                reader.view.frame=host.view.bounds
                if reader.view.bounds.width<100 {reader.view.frame=CGRect(x:0,y:0,width:402,height:874)}
                var ready=false;reader.visible={_,_ in ready=true}
                func styles(_ chapter:Int)async throws -> NativePresentation {try await Task.detached {try client.call("native_presentation",["id":entry.id,"chapter":chapter,"policy":"mixed","scripts":true,"events":[]],as:NativePresentation.self)}.value}
                reader.nativeStyles=[0:try await styles(0).styles]
                reader.view.setNeedsLayout();reader.view.layoutIfNeeded()
                for _ in 0..<100 {if ready {break};try await Task.sleep(for:.milliseconds(10))}
                guard ready && !reader.pages.isEmpty else {throw CoreFailure(message:"First content did not become visible")}
                func draw() {let image=UIGraphicsImageRenderer(size:reader.view.bounds.size).image {_ in reader.view.drawHierarchy(in:reader.view.bounds,afterScreenUpdates:true)};_ = image.size}
                draw();report["first_content_layout_draw_ms"]=(CACurrentMediaTime()-start)*1000;report["first_content_memory"]=memory()
                meter.start();var cases=[[String:Any]]();var jumpID=0
                for mode in [ReadingMode.curl,.slide] {
                    options.readingMode=mode;options.sliding=mode != .curl
                    for target in [3,book.chapters.count/2,book.chapters.count-1] {
                        let begin=CACurrentMediaTime();reader.nativeStyles=[target:try await styles(target).styles]
                        jumpID+=1;options.nativeRevision+=1;reader.update(options:options,jump:.init(chapter:target),jumpID:jumpID,annotations:.empty,revision:0)
                        reader.view.layoutIfNeeded();draw()
                        let first=(CACurrentMediaTime()-begin)*1000
                        meter.intervals=[];var durations=[Double]();meter.measuring=true
                        for turn in 0..<8 {
                            let previous=reader.anchor,tick=CACurrentMediaTime();reader.turnSpread(forward:turn<4)
                            for _ in 0..<200 {if reader.anchor != previous {break};try await Task.sleep(for:.milliseconds(5))}
                            guard reader.anchor != previous else {throw CoreFailure(message:"Turn did not commit in \(mode)/\(target)")}
                            durations.append((CACurrentMediaTime()-tick)*1000)
                            try await Task.sleep(for:.milliseconds(20))
                        }
                        meter.measuring=false
                        let frames=meter.intervals.sorted();guard !frames.isEmpty else {throw CoreFailure(message:"No display-link samples")}
                        cases.append(["mode":mode.rawValue,"chapter":target,"chapter_first_layout_draw_ms":first,"turn_total_ms":durations,"display_interval_p50_ms":frames[frames.count/2],"display_interval_p95_ms":frames[frames.count*95/100],"display_interval_max_ms":frames.last!,"display_samples":frames.count,"memory":memory()])
                    }
                }
                report["before_walk_memory"]=memory()
                try await Task.detached {for index in 3..<203 { _ = try book.chapter(at:index);let _:NativePresentation=try client.call("native_presentation",["id":entry.id,"chapter":index,"policy":"mixed","scripts":true,"events":[]])}}.value
                report["after_200_chapters_memory"]=memory();report["cache_stats"]=try client.call("chapter_cache_stats",["id":entry.id],as:[String:Int].self)
                report["chapter_count"]=book.chapters.count;report["swift_resident_bodies"]=book.chapterSource!.residentCount;report["cases"]=cases;report["passed"]=true
            } catch {report["passed"]=false;report["error"]=String(describing:error)}
            meter.stop()
            let data=try! JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]);try? data.write(to:caches.appendingPathComponent(CommandLine.arguments.contains("--performance-reopen") ? "release-performance-reopen.json":"release-performance.json"),options:.atomic)
            _ = checkWindow
        }
    }
}
#endif
