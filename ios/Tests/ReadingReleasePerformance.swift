#if READER_PERFORMANCE_CHECK
import Foundation
import UIKit
import QuartzCore
import CryptoKit
import SwiftUI

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
        return result==KERN_SUCCESS ? ["peak_resident_bytes":UInt64(info.resident_size_peak),"resident_bytes":UInt64(info.resident_size),"physical_footprint_bytes":UInt64(info.phys_footprint)] : [:]
    }
    static func run() {
        precondition(!_isDebugAssertConfiguration(), "Release build required")
        Task { @MainActor in
            let files=FileManager.default,caches=files.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            var report:[String:Any]=["harness_schema":3,"profile":"release","environment":"iOS simulator; production UIKit/CoreText reader"]
            let meter=ReadingFrameMeter();var checkWindow:UIWindow?
            do {
                for _ in 0..<100 {if UIApplication.shared.connectedScenes.contains(where:{$0 is UIWindowScene}) {break};try await Task.sleep(for:.milliseconds(20))}
                guard let scene=UIApplication.shared.connectedScenes.first(where:{$0 is UIWindowScene}) as? UIWindowScene else {throw CoreFailure(message:"Missing window scene")}
                let arguments=CommandLine.arguments
                let endurance=arguments.contains("--performance-endurance")
                report["scenario"]=endurance ? "visible-endurance" : "reader"
                let runIndex=arguments.firstIndex(of:"--performance-run")
                let requested=runIndex.flatMap {arguments.indices.contains($0+1) ? arguments[$0+1] : nil} ?? "default"
                let run=String(requested.filter {$0.isASCII && ($0.isLetter || $0.isNumber || $0=="-")}.prefix(64))
                let reportIndex=arguments.firstIndex(of:"--performance-report-id")
                report["run_id"]=reportIndex.flatMap {arguments.indices.contains($0+1) ? arguments[$0+1] : nil} ?? requested
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
                report["viewport"]=["width_points":reader.view.bounds.width,"height_points":reader.view.bounds.height,"scale":scene.screen.scale,"maximum_frames_per_second":scene.screen.maximumFramesPerSecond,"reduce_motion":UIAccessibility.isReduceMotionEnabled]
                var ready=false;reader.visible={_,_ in ready=true}
                func styles(_ chapter:Int)async throws -> NativePresentation {try await Task.detached {try client.call("native_presentation",["id":entry.id,"chapter":chapter,"policy":"mixed","scripts":true,"events":[]],as:NativePresentation.self)}.value}
                reader.nativeStyles=[0:try await styles(0).styles]
                reader.view.setNeedsLayout();reader.view.layoutIfNeeded()
                for _ in 0..<100 {if ready {break};try await Task.sleep(for:.milliseconds(10))}
                guard ready && !reader.pages.isEmpty else {throw CoreFailure(message:"First content did not become visible")}
                func draw() {let image=UIGraphicsImageRenderer(size:reader.view.bounds.size).image {_ in reader.view.drawHierarchy(in:reader.view.bounds,afterScreenUpdates:true)};_ = image.size}
                draw();report["first_content_layout_draw_ms"]=(CACurrentMediaTime()-start)*1000;report["first_content_memory"]=memory()
                meter.start();var cases=[[String:Any]]();var jumpID=0,settledJumpID=0
                reader.navigated={_,token in settledJumpID=token}
                for mode in [ReadingMode.curl,.slide] {
                    options.readingMode=mode;options.sliding=mode != .curl
                    for target in (endurance ? [0] : Set([min(3,book.chapters.count-1),book.chapters.count/2,book.chapters.count-1]).sorted()) {
                        let begin=CACurrentMediaTime();reader.nativeStyles=[target:try await styles(target).styles]
                        jumpID+=1;options.nativeRevision+=1;reader.update(options:options,jump:.init(chapter:target),jumpID:jumpID,annotations:.empty,revision:0)
                        for _ in 0..<600 {if settledJumpID==jumpID {break};try await Task.sleep(for:.milliseconds(5))}
                        guard settledJumpID==jumpID && reader.anchor.chapter==target else {throw CoreFailure(message:"Chapter navigation did not settle in \(mode)/\(target)")}
                        reader.view.layoutIfNeeded();draw()
                        let first=(CACurrentMediaTime()-begin)*1000
                        meter.intervals=[];var durations=[Double]();meter.measuring=true
                        let turns=endurance ? 128 : 8, origin=reader.anchor
                        var turnMemory=[[String:Any]](), preparation=[Double](), preparedChapter = -1
                        for turn in 0..<turns {
                            if endurance && reader.anchor.chapter != preparedChapter {
                                let center=reader.anchor.chapter, beginPrepare=CACurrentMediaTime()
                                let indices=Array(max(0,center-2)..<min(book.chapters.count,center+3))
                                let neighbours=try await Task.detached { () -> [Int:[NativeStyle]] in
                                    var result=[Int:[NativeStyle]]()
                                    for index in indices {
                                        _ = try book.chapter(at:index)
                                        let presentation:NativePresentation=try client.call("native_presentation",["id":entry.id,"chapter":index,"policy":"mixed","scripts":true,"events":[]])
                                        guard presentation.errors.isEmpty else {throw CoreFailure(message:"Neighbour presentation failed")}
                                        result[index]=presentation.styles
                                    }
                                    return result
                                }.value
                                reader.nativeStyles=neighbours;preparedChapter=center
                                preparation.append((CACurrentMediaTime()-beginPrepare)*1000)
                            }
                            let previous=reader.anchor,tick=CACurrentMediaTime();reader.turnSpread(forward:turn<turns/2)
                            for _ in 0..<200 {if reader.anchor != previous {break};try await Task.sleep(for:.milliseconds(5))}
                            guard reader.anchor != previous else {throw CoreFailure(message:"Turn did not commit in \(mode)/\(target)")}
                            durations.append((CACurrentMediaTime()-tick)*1000)
                            if endurance && (turn+1)%16==0 {turnMemory.append(["turns":turn+1,"memory":memory()])}
                            try await Task.sleep(for:.milliseconds(20))
                        }
                        meter.measuring=false
                        if endurance && reader.anchor != origin {throw CoreFailure(message:"Endurance turns did not return to origin")}
                        let frames=meter.intervals.sorted();guard !frames.isEmpty else {throw CoreFailure(message:"No display-link samples")}
                        cases.append(["mode":mode.rawValue,"chapter":target,"chapter_first_layout_draw_ms":first,"turn_total_ms":durations,"display_interval_p50_ms":frames[frames.count/2],"display_interval_p95_ms":frames[frames.count*95/100],"display_interval_max_ms":frames.last!,"display_samples":frames.count,"memory":memory(),"turn_memory_samples":turnMemory,"returned_to_origin":reader.anchor==origin,"neighbour_preparation_ms":preparation])
                    }
                }
                report["before_walk_memory"]=memory()
                let walkStart=min(3,book.chapters.count-1),walkEnd=min(walkStart+200,book.chapters.count)
                var walkSamples=[[String:Any]]()
                for start in stride(from:walkStart,to:walkEnd,by:25) {
                    let end=min(start+25,walkEnd)
                    try await Task.detached {for index in start..<end { _ = try book.chapter(at:index);let _:NativePresentation=try client.call("native_presentation",["id":entry.id,"chapter":index,"policy":"mixed","scripts":true,"events":[]])}}.value
                    walkSamples.append(["chapters":end-walkStart,"memory":memory()])
                }
                report["walk_chapters"]=walkEnd-walkStart;report["walk_memory_samples"]=walkSamples
                report["after_walk_memory"]=memory();report["cache_stats"]=try client.call("chapter_cache_stats",["id":entry.id],as:[String:Int].self)
                report["chapter_count"]=book.chapters.count;report["swift_resident_bodies"]=book.chapterSource!.residentCount;
                try await Task.sleep(for:.milliseconds(750));var steady=[[String:UInt64]]()
                for _ in 0..<5 {steady.append(memory());try await Task.sleep(for:.milliseconds(150))};report["steady_memory_samples"]=steady
                let input=try FileHandle(forReadingFrom:source);defer {try? input.close()};var digest=SHA256();while let bytes=try input.read(upToCount:1024*1024),!bytes.isEmpty {digest.update(data:bytes)};report["book_sha256"]=digest.finalize().map {String(format:"%02x",$0)}.joined();report["cases"]=cases;report["passed"]=true
            } catch {report["passed"]=false;report["error"]=String(describing:error)}
            meter.stop()
            let data=try! JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]);try? data.write(to:caches.appendingPathComponent(CommandLine.arguments.contains("--performance-reopen") ? "release-performance-reopen.json":"release-performance.json"),options:.atomic)
            _ = checkWindow
        }
    }
}

// Counters are lock-protected because thumbnail work runs on an independent actor.
final class ShelfProbeMetrics: @unchecked Sendable {
    static let shared=ShelfProbeMetrics()
    private let lock=NSLock()
    private var views=Set<String>(),queries=Set<String>(),ready=Set<String>()
    private var rankEvaluations=0,geometryUpdates=0,homeEvaluations=0
    func homeEvaluated() {lock.lock();defer {lock.unlock()};homeEvaluations += 1}
    func ranked() {lock.lock();defer {lock.unlock()};rankEvaluations += 1}
    func geometryUpdated() {lock.lock();defer {lock.unlock()};geometryUpdates += 1}
    func viewRequested(_ id:String) {lock.lock();defer {lock.unlock()};views.insert(id)}
    func requested(_ id:String) {lock.lock();defer {lock.unlock()};queries.insert(id)}
    func prepared(_ id:String) {lock.lock();defer {lock.unlock()};ready.insert(id)}
    func snapshot()->(views:Int,queries:Int,prepared:Int,ids:Set<String>,ranks:Int,geometry:Int,homes:Int) {lock.lock();defer {lock.unlock()};return (views.count,queries.count,ready.count,ready,rankEvaluations,geometryUpdates,homeEvaluations)}
}
@MainActor enum ShelfReleasePerformance {
    static var started:CFTimeInterval=0
    static var baseline=[String:UInt64]()
    static func beginProcess() {started=CACurrentMediaTime();baseline=ReadingReleasePerformance.memory()}
}
@MainActor struct ShelfPerformanceRoot:View {
    @StateObject private var session=ShelfPerformanceSession()
    var body:some View {
        AppHome(library:session.library)
            .onPreferenceChange(BookCoverFrames.self) {session.bounds=$0}
            .task {await session.run()}
    }
}
@MainActor final class ShelfPerformanceSession:ObservableObject {
    let library:PreviewLibrary
    private let root:URL,runID:String
    private var running=false
    var bounds=[String:CGRect]()
    init() {
        let args=CommandLine.arguments,index=args.firstIndex(of:"--performance-run")
        runID=index.flatMap {args.indices.contains($0+1) ? args[$0+1] : nil} ?? "default"
        precondition(!runID.isEmpty && runID.count<=80 && runID.allSatisfy {$0.isASCII && ($0.isLetter || $0.isNumber || $0=="-")})
        root=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask)[0].appendingPathComponent("ShelfPerformanceLibrary-"+runID)
        library=PreviewLibrary(rootOverride:root)
    }
    private func scrollViews(_ view:UIView)->[UIScrollView] { ((view as? UIScrollView).map {[$0]} ?? []) + view.subviews.flatMap {scrollViews($0)} }
    func run()async {
        guard !running else {return};running=true
        precondition(!_isDebugAssertConfiguration(),"Shelf performance requires Release")
        var report:[String:Any] = ["scenario":"dense-shelf","harness_schema":2,"profile":"release","run_id":runID,"baseline_memory":ShelfReleasePerformance.baseline,"input_kind":"native UIScrollView animated content offset","release_network_checks":false]
        let meter=ReadingFrameMeter();meter.start();meter.measuring=true
        do {
            let manifestData=try Data(contentsOf:root.appendingPathComponent("manifest.json"))
            guard manifestData.count<=4*1024*1024,let manifest=try JSONSerialization.jsonObject(with:manifestData) as? [String:Any],
                  manifest["schema"] as? Int == 1,manifest["scenario"] as? String == "dense-shelf",
                  let count=manifest["count"] as? Int,(1...10000).contains(count) else {throw CoreFailure(message:"Invalid shelf fixture")}
            report["book_count"]=count;report["fixture_manifest_sha256"]=SHA256.hash(data:manifestData).map {String(format:"%02x",$0)}.joined()
            func wait(_ ready:()->Bool)async throws {for _ in 0..<3000 {if ready(){return};try await Task.sleep(for:.milliseconds(10))};throw CoreFailure(message:"Shelf did not become ready")}
            try await wait {self.library.books.count==count && !self.bounds.isEmpty}
            guard let scene=UIApplication.shared.connectedScenes.first(where:{$0 is UIWindowScene}) as? UIWindowScene,
                  let window=scene.windows.first(where:{$0.isKeyWindow}) else {throw CoreFailure(message:"Missing shelf window")}
            try await Task.sleep(for:.milliseconds(35));window.layoutIfNeeded()
            func image()->UIImage {UIGraphicsImageRenderer(size:window.bounds.size).image {_ in window.drawHierarchy(in:window.bounds,afterScreenUpdates:true)}}
            report["metadata_first_layout_ms"]=(CACurrentMediaTime()-ShelfReleasePerformance.started)*1000
            report["first_metadata_memory"]=ReadingReleasePerformance.memory()
            let initial=ShelfProbeMetrics.shared.snapshot();report["initial_cover_requests"]=initial.views
            guard count<=96 || initial.views<min(count,1000) else {throw CoreFailure(message:"Shelf eagerly requested every cover")}
            try await wait {
                let ready=ShelfProbeMetrics.shared.snapshot().ids
                return self.bounds.filter {$0.key.hasPrefix("shelf:") && $0.value.intersects(window.bounds) && ready.contains(String($0.key.dropFirst(6)))}.count>=min(4,count)
            }
            try await Task.sleep(for:.milliseconds(35));window.layoutIfNeeded()
            report["first_covers_ready_ms"]=(CACurrentMediaTime()-ShelfReleasePerformance.started)*1000
            let centers=bounds.filter {$0.key.hasPrefix("shelf:") && $0.value.intersects(window.bounds)}.map {CGPoint(x:$0.value.midX,y:$0.value.midY)}
            let candidates=scrollViews(window).filter {scroll in scroll.contentSize.height>scroll.bounds.height+1 && centers.contains(where:{scroll.convert(scroll.bounds,to:window).contains($0)})}
            guard let scroll=candidates.max(by:{$0.contentSize.height<$1.contentSize.height}) else {throw CoreFailure(message:"Missing production shelf scroll view")}
            // Separate stack diagnostics attach only after metadata, covers and the
            // production scroll view are ready. Normal baseline runs never wait.
            if CommandLine.arguments.contains("--diagnose-shelf-scroll") {
                try Data(runID.utf8).write(to:root.appendingPathComponent("scroll-ready"),options:.atomic)
                try await wait {FileManager.default.fileExists(atPath:self.root.appendingPathComponent("scroll-go").path)}
            }
            meter.intervals=[];var curve=[[String:Any]](),forward=true
            for step in 1...30 {
                let old=scroll.contentOffset.y,maxY=max(0,scroll.contentSize.height-scroll.bounds.height+scroll.adjustedContentInset.bottom)
                if old>=maxY-1 {forward=false};if old<=0 {forward=true}
                let target=min(maxY,max(0,old+(forward ? 1 : -1)*scroll.bounds.height*0.7))
                scroll.setContentOffset(CGPoint(x:scroll.contentOffset.x,y:target),animated:true)
                try await Task.sleep(for:.milliseconds(450));window.layoutIfNeeded()
                if count>96 && abs(scroll.contentOffset.y-old)<1 {throw CoreFailure(message:"Shelf scrolling did not move")}
                if step%5==0 {let counters=ShelfProbeMetrics.shared.snapshot();curve.append(["scrolls":step,"memory":ReadingReleasePerformance.memory(),"requested":counters.views,"core_queries":counters.queries,"prepared":counters.prepared])}
            }
            meter.measuring=false;try await Task.sleep(for:.milliseconds(750))
            var steady=[[String:UInt64]]();for _ in 0..<5 {steady.append(ReadingReleasePerformance.memory());try await Task.sleep(for:.milliseconds(150))}
            let frames=meter.intervals.sorted(),counters=ShelfProbeMetrics.shared.snapshot()
            guard frames.count>=100,library.books.count==count,counters.prepared>=min(count,12),count<=96 || counters.views<count else {throw CoreFailure(message:"Shelf coverage incomplete or all covers eagerly loaded")}
            report["ranking_evaluations"]=counters.ranks;report["geometry_updates"]=counters.geometry
            report["home_body_evaluations"]=counters.homes
            report["scrolls"]=30;report["scroll_memory_samples"]=curve;report["steady_memory_samples"]=steady
            report["total_cover_requests"]=counters.views;report["thumbnail_core_queries"]=counters.queries;report["prepared_covers"]=counters.prepared
            report["configured_cover_cache_count_limit"]=64;report["configured_cover_cache_cost_limit"]=64*1024*1024
            report["frame_samples"]=frames.count;report["display_interval_p50_ms"]=frames[frames.count/2];report["display_interval_p95_ms"]=frames[frames.count*95/100];report["display_interval_max_ms"]=frames.last!
            report["viewport"]=["width_points":window.bounds.width,"height_points":window.bounds.height,"scale":scene.screen.scale,"maximum_frames_per_second":scene.screen.maximumFramesPerSecond,"reduce_motion":UIAccessibility.isReduceMotionEnabled,"content_size_category":UIApplication.shared.preferredContentSizeCategory.rawValue]
            guard let lastPNG=image().pngData() else {throw CoreFailure(message:"Cannot encode shelf screenshot")}
            try lastPNG.write(to:root.appendingPathComponent("shelf-scrolled.png"))
            report["passed"]=true
        } catch {report["passed"]=false;report["error"]=String(describing:error)}
        meter.stop()
        if let data=try? JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]) {try? data.write(to:root.deletingLastPathComponent().appendingPathComponent("shelf-performance.json"),options:.atomic)}
    }
}
#endif

#if READER_PERFORMANCE_CHECK
import ImageIO

// Actual owned C buffers and UIKit/ImageIO decoding. No claim about UI frames.
enum ImageBridgePerformance {
    static func run() {
        precondition(!_isDebugAssertConfiguration(), "Release build required")
        Task.detached(priority:.userInitiated) {
            let files=FileManager.default
            let root=files.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            let reportURL=root.appendingPathComponent("image-bridge-performance.json")
            var report:[String:Any] = ["schema":2,"physical_device":false,"process_id":ProcessInfo.processInfo.processIdentifier,"run_id":ProcessInfo.processInfo.environment["READER_PERFORMANCE_RUN"] ?? "missing","profile":ProcessInfo.processInfo.environment["READER_RUST_PROFILE"] ?? "release"]
            do {
                guard Bundle.main.bundleIdentifier=="local.reader.performance" else {throw CoreFailure(message:"Isolated app required")}
                report["isolated_blank_host"]=true;report["release_network_checks"]=false
                try await Task.sleep(for:.seconds(2))
                guard let config=try JSONSerialization.jsonObject(with:Data(contentsOf:root.appendingPathComponent("image-bridge-config.json"))) as? [String:Any],let specs=config["books"] as? [[String:Any]] else {throw CoreFailure(message:"Missing image bridge config")}
                let client=CoreClient(root:root.appendingPathComponent("ImageBridgeLibrary").path,device:"image-bridge-check")
                if let transport=config["memory_transport"] as? String {
                    let value=try await memoryProbe(client:client,root:root,specs:specs,transport:transport)
                    report.merge(value){_,new in new}
                    try JSONSerialization.data(withJSONObject:report,options:[.sortedKeys,.prettyPrinted]).write(to:reportURL,options:.atomic)
                    return
                }
                var cases:[[String:Any]]=[]
                for spec in specs {
                    guard let file=spec["file"] as? String,URL(fileURLWithPath:file).lastPathComponent==file,file.hasSuffix(".epub"),let chapter=spec["chapter"] as? Int,let label=spec["label"] as? String else {throw CoreFailure(message:"Invalid image bridge config")}
                    let source=root.appendingPathComponent(file)
                    let entry:LibraryEntry=try client.call("import",["path":source.path])
                    let begin=CACurrentMediaTime()
                    let _:PreviewBook=try client.call("open",["id":entry.id])
                    let (baseline,_)=try read(client,id:entry.id,chapter:chapter,session:nil)
                    let prepareMS=(CACurrentMediaTime()-begin)*1000
                    let session:String=try client.call("image_transport",["action":"begin","id":entry.id])
                    defer {let _:Bool?=try? client.call("image_transport",["action":"close","id":entry.id,"session":session])}
                    let prepared:Bool=try client.call("image_transport",["action":"prepare","id":entry.id,"session":session]);guard prepared else {throw CoreFailure(message:"Image session not prepared")}
                    let (header,_)=try read(client,id:entry.id,chapter:chapter,session:session)
                    let baseImages=baseline.blocks.filter{$0.type=="Image"};let newImages=header.blocks.filter{$0.type=="Image"}
                    guard !baseImages.isEmpty,baseImages.count==newImages.count else {throw CoreFailure(message:"Image count changed")}
                    var signatures:[String]=[]
                    var restored:[BookBlock]=[]
                    for block in header.blocks {
                        if block.type=="Image" {
                            guard let reference=block.data else {throw CoreFailure(message:"Missing image reference")}
                            let data=try client.resourceImage(id:entry.id,reference:reference)
                            signatures.append(SHA256.hash(data:data).map{String(format:"%02x",$0)}.joined())
                            restored.append(BookBlock(native_style:block.native_style,type:block.type,core_block:block.core_block,core_char_offset:block.core_char_offset,level:block.level,spans:block.spans,data:data.base64EncodedString(),alt:block.alt))
                        }else{restored.append(block)}
                    }
                    guard BookChapter(title:header.title,blocks:restored)==baseline else {throw CoreFailure(message:"Chapter metadata or image bytes changed")}
                    guard let first=newImages.first?.data else {throw CoreFailure(message:"Missing image reference")}
                    do {_=try client.resourceImage(id:entry.id+"-foreign",reference:first);throw CoreFailure(message:"Foreign image accepted")}catch let e as CoreFailure {guard e.message=="Image resource unavailable" else {throw e}}
                    do {_=try client.resourceImage(id:entry.id,reference:"img1:malformed");throw CoreFailure(message:"Malformed image accepted")}catch let e as CoreFailure {guard e.message=="Image resource unavailable" else {throw e}}
                    let decoderWarmup=CACurrentMediaTime()
                    for binary in [false,true] {
                        let data:Data
                        if binary {data=try client.resourceImage(id:entry.id,reference:first)}else{
                            guard let encoded=baseImages.first?.data,let decoded=Data(base64Encoded:encoded) else {throw CoreFailure(message:"Invalid warmup image")};data=decoded
                        }
                        guard let source=CGImageSourceCreateWithData(data as CFData,nil),let bitmap=CGImageSourceCreateThumbnailAtIndex(source,0,[kCGImageSourceCreateThumbnailFromImageAlways:true,kCGImageSourceCreateThumbnailWithTransform:true,kCGImageSourceShouldCacheImmediately:true,kCGImageSourceThumbnailMaxPixelSize:2048] as CFDictionary),bitmap.width>0 else {throw CoreFailure(message:"Image decoder warmup failed")}
                    }
                    let decoderWarmupMS=(CACurrentMediaTime()-decoderWarmup)*1000
                    for selected in [1,baseImages.count] {
                        for iteration in 0..<7 {
                            for binary in iteration.isMultiple(of:2) ? [false,true] : [true,false] {
                                let row:[String:Any]=try autoreleasepool {
                                    let before=memory();let start=CACurrentMediaTime()
                                    let (decoded,jsonBytes)=try read(client,id:entry.id,chapter:chapter,session:binary ? session:nil)
                                    let images=Array(decoded.blocks.filter{$0.type=="Image"}.prefix(selected));let parsed=CACurrentMediaTime()
                                    var transfer=0.0,bounds=0.0,decode=0.0,totalBytes=0
                                    var dimensions:[[Int]]=[]
                                    for image in images {
                                        guard let encoded=image.data else {throw CoreFailure(message:"Missing image")}
                                        var at=CACurrentMediaTime()
                                        let data:Data
                                        if binary {data=try client.resourceImage(id:entry.id,reference:encoded)}else{guard let value=Data(base64Encoded:encoded) else {throw CoreFailure(message:"Invalid Base64")};data=value}
                                        transfer += CACurrentMediaTime()-at;totalBytes += data.count
                                        at=CACurrentMediaTime()
                                        guard let source=CGImageSourceCreateWithData(data as CFData,[kCGImageSourceShouldCache:false] as CFDictionary),let properties=CGImageSourceCopyPropertiesAtIndex(source,0,nil) as? [CFString:Any],let width=properties[kCGImagePropertyPixelWidth] as? Int,let height=properties[kCGImagePropertyPixelHeight] as? Int,width>0,height>0 else {throw CoreFailure(message:"Image bounds unavailable")}
                                        bounds += CACurrentMediaTime()-at;dimensions.append([width,height])
                                        at=CACurrentMediaTime()
                                        guard let bitmap=CGImageSourceCreateThumbnailAtIndex(source,0,[kCGImageSourceCreateThumbnailFromImageAlways:true,kCGImageSourceCreateThumbnailWithTransform:true,kCGImageSourceShouldCacheImmediately:true,kCGImageSourceThumbnailMaxPixelSize:2048] as CFDictionary),bitmap.width>0 else {throw CoreFailure(message:"Image decode failed")}
                                        decode += CACurrentMediaTime()-at
                                    }
                                    let end=CACurrentMediaTime()
                                    return ["book":label,"chapter":chapter,"image_count":baseImages.count,"selected":selected,"iteration":iteration,"transport":binary ? "resource":"base64","json_bytes":jsonBytes,"image_bytes":totalBytes,"encode_parse_ms":(parsed-start)*1000,"transfer_ms":transfer*1000,"bounds_ms":bounds*1000,"decode_ms":decode*1000,"total_ms":(end-start)*1000,"memory_before":before,"memory_after":memory(),"dimensions":dimensions]
                                }
                                cases.append(row)
                            }
                        }
                    }
                    report[label] = ["prepare_and_first_legacy_chapter_ms":prepareMS,"decoder_warmup_ms":decoderWarmupMS,"sha256":SHA256.hash(data:try Data(contentsOf:source)).map{String(format:"%02x",$0)}.joined(),"image_sha256":signatures]
                    let next:String=try client.call("image_transport",["action":"begin","id":entry.id])
                    let oldClose:Bool=try client.call("image_transport",["action":"close","id":entry.id,"session":session]);guard !oldClose else {throw CoreFailure(message:"Late close accepted")}
                    do {_=try client.resourceImage(id:entry.id,reference:first);throw CoreFailure(message:"Replaced session accepted")}catch let e as CoreFailure {guard e.message=="Image resource unavailable" else {throw e}}
                    do {let _:Bool=try client.call("image_transport",["action":"prepare","id":entry.id,"session":session]);throw CoreFailure(message:"Late prepare accepted")}catch let e as CoreFailure {guard e.message=="Image session expired" else {throw e}}
                    let ready:Bool=try client.call("image_transport",["action":"prepare","id":entry.id,"session":next]);let closed:Bool=try client.call("image_transport",["action":"close","id":entry.id,"session":next]);guard ready && closed else {throw CoreFailure(message:"Session close failed")}
                    do {_=try read(client,id:entry.id,chapter:chapter,session:next);throw CoreFailure(message:"Closed chapter accepted")}catch let e as CoreFailure {guard e.message=="Image session expired" else {throw e}}
                }
                report["cases"]=cases;report["byte_and_metadata_parity"]=true;report["lifetime_checks"]=true;report["passed"]=true
            }catch{report["passed"]=false;report["error"]=String(describing:error)}
            if let data=try? JSONSerialization.data(withJSONObject:report,options:[.sortedKeys,.prettyPrinted]){try? data.write(to:reportURL,options:.atomic)}
        }
    }
    private static func memoryProbe(client:CoreClient,root:URL,specs:[[String:Any]],transport:String) async throws -> [String:Any] {
        guard ["base64","resource"].contains(transport),let spec=specs.first,let file=spec["file"] as? String,URL(fileURLWithPath:file).lastPathComponent==file,file.hasSuffix(".epub"),let index=spec["chapter"] as? Int,let label=spec["label"] as? String else {throw CoreFailure(message:"Invalid image memory config")}
        let source=root.appendingPathComponent(file)
        let entry:LibraryEntry=try client.call("import",["path":source.path])
        let _:PreviewBook=try client.call("open",["id":entry.id])
        var session:String?
        if transport=="resource" {
            let token:String=try client.call("image_transport",["action":"begin","id":entry.id]);session=token
            let _:Bool=try client.call("image_transport",["action":"prepare","id":entry.id,"session":token])
        }
        defer {if let session {let _:Bool?=try? client.call("image_transport",["action":"close","id":entry.id,"session":session])}}
        try await Task.sleep(for:.milliseconds(300));let baseline=memory()
        let (chapter,jsonBytes)=try autoreleasepool {try read(client,id:entry.id,chapter:index,session:session)}
        let count=chapter.blocks.filter{$0.type=="Image"}.count
        guard count>1 else {throw CoreFailure(message:"Memory probe needs an illustrated chapter")}
        let after=withExtendedLifetime(chapter){memory()}
        var samples:[[String:UInt64]]=[]
        for _ in 0..<5 {try await Task.sleep(for:.milliseconds(300));samples.append(withExtendedLifetime(chapter){memory()})}
        let hash=SHA256.hash(data:try Data(contentsOf:source)).map{String(format:"%02x",$0)}.joined()
        return withExtendedLifetime(chapter) {
            ["measurement":"retained-native-chapter","transport":transport,"book":label,"book_sha256":hash,"chapter":index,"image_count":count,"json_bytes":jsonBytes,"baseline_memory":baseline,"after_chapter_memory":after,"steady_memory_samples":samples,"bitmap_decode_measured":false,"probe_gc_requested":false,"passed":true]
        }
    }
    private static func read(_ client:CoreClient,id:String,chapter:Int,session:String?) throws -> (BookChapter,Int) {
        var request:[String:Any] = ["root":client.root,"device":client.device,"id":id,"chapter":chapter,"op":session == nil ? "chapter":"image_transport"]
        if let session {request["action"]="chapter";request["session"]=session}
        let input=try JSONSerialization.data(withJSONObject:request)
        guard let result=String(decoding:input,as:UTF8.self).withCString({reader_ios_request($0)}) else {throw CoreFailure(message:"No chapter result")}
        defer {reader_ios_free(result)}
        let data=Data(bytes:result,count:strlen(result));let envelope=try JSONDecoder().decode(CoreEnvelope<BookChapter>.self,from:data)
        guard envelope.ok,let chapter=envelope.result else {throw CoreFailure(message:envelope.error ?? "Chapter failed")}
        return(chapter,data.count)
    }
    private static func memory()->[String:UInt64] {
        var info=task_vm_info_data_t();var count=mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size/MemoryLayout<natural_t>.size)
        let result=withUnsafeMutablePointer(to:&info){value in value.withMemoryRebound(to:integer_t.self,capacity:Int(count)){task_info(mach_task_self_,task_flavor_t(TASK_VM_INFO),$0,&count)}}
        return result==KERN_SUCCESS ? ["peak_resident_bytes":UInt64(info.resident_size_peak),"resident_bytes":UInt64(info.resident_size),"physical_footprint_bytes":UInt64(info.phys_footprint)] : [:]
    }
}
#endif
