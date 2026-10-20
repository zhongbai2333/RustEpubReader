#if DEBUG
import SwiftUI
import UIKit
import CoreText

/// Owned native reader for public XCTest touches, not simulated gesture states.
struct ReaderSelectionUITestHost: UIViewControllerRepresentable {
    func makeUIViewController(context:Context)->ReaderSelectionHostController {ReaderSelectionHostController()}
    func updateUIViewController(_ controller:ReaderSelectionHostController,context:Context) {}
}

/// Passive diagnostics: never recognizes, prevents or delays another recognizer.
private final class SelectionTouchTrace:UIGestureRecognizer {
    var record:((String,Set<UITouch>,UIEvent?)->Void)?
    override func canPrevent(_ preventedGestureRecognizer:UIGestureRecognizer)->Bool {false}
    override func canBePrevented(by preventingGestureRecognizer:UIGestureRecognizer)->Bool {false}
    override func touchesBegan(_ touches:Set<UITouch>,with event:UIEvent) {record?("began",touches,event)}
    override func touchesMoved(_ touches:Set<UITouch>,with event:UIEvent) {record?("moved",touches,event)}
    override func touchesEnded(_ touches:Set<UITouch>,with event:UIEvent) {record?("ended",touches,event);state = .failed}
    override func touchesCancelled(_ touches:Set<UITouch>,with event:UIEvent) {record?("cancelled",touches,event);state = .failed}
}

@MainActor final class ReaderSelectionHostController:UIViewController {
    private let status=UIButton(type:.system)
    private var reader:NativeReadingController?
    private var annotations:ReadingAnnotations?
    private var root:URL?
    private var expectedBlock=0
    private var expectedStart=0
    private let expectedText="文字 😀 中文"
    private var mode=ReadingMode.none
    private var runID=""
    private var handled=false
    private var tracedGesture:UILongPressGestureRecognizer?
    private var gestureEvents=[[String:Any]]()
    private var touchEvents=[[String:Any]]()
    private var touchTrace:SelectionTouchTrace?
    private var inputFocusConfirmed=false
    private var receivedFocusTaps=0
    private var ignoredFocusProbes=0
    private var focusTask:Task<Void,Never>?

    @objc private func confirmInputFocus() {
        guard !handled else{return}
        receivedFocusTaps+=1
        // Explicitly enabled only by isolated local fault-injection probes.
        let ignored=min(3,max(0,Int(ProcessInfo.processInfo.environment["READER_SELECTION_IGNORE_FOCUS_PROBES"] ?? "0") ?? 0))
        if receivedFocusTaps<=ignored {ignoredFocusProbes+=1;writeGestureTrace();return}
        inputFocusConfirmed=true
        writeGestureTrace()
        focusTask?.cancel()
        // Coordinate publication can be temporarily unavailable during a
        // window transition. Keep reporting real state, without another tap
        // or any selection action, within the setup handshake's budget.
        focusTask=Task {@MainActor [weak self] in
            for _ in 0..<200 {
                guard !Task.isCancelled,let self else{return}
                if self.publishTargets(),let window=self.reader?.view.window,
                   window.isKeyWindow,window.windowScene?.activationState == .foregroundActive {return}
                try? await Task.sleep(for:.milliseconds(50))
            }
        }
    }

    @objc private func traceSelection(_ gesture:UILongPressGestureRecognizer) {
        let point=gesture.location(in:gesture.view)
        var ancestors=[[String:Any]]()
        var parent=gesture.view?.superview
        while let current=parent {
            ancestors += (current.gestureRecognizers ?? []).map {["type":String(describing:type(of:$0)),"state":$0.state.rawValue,"enabled":$0.isEnabled]}
            parent=current.superview
        }
        gestureEvents.append(["state":gesture.state.rawValue,"time":ProcessInfo.processInfo.systemUptime,"x":point.x,"y":point.y,"ancestors":ancestors])
        if [.began,.ended,.cancelled,.failed].contains(gesture.state) {writeGestureTrace()}
    }
    private func writeGestureTrace() {
        let value:[String:Any]=["mode":mode.rawValue,"run_id":runID,"events":gestureEvents,"touches":touchEvents,
            "focus":["confirmed":inputFocusConfirmed,"received_taps":receivedFocusTaps,"ignored_probes":ignoredFocusProbes,
                     "key_window":view.window?.isKeyWindow == true,"foreground_scene":view.window?.windowScene?.activationState == .foregroundActive]]
        if let bytes=try? JSONSerialization.data(withJSONObject:value,options:[.prettyPrinted,.sortedKeys]) {
            let cache=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            try? bytes.write(to:cache.appendingPathComponent("selection-gesture-"+mode.rawValue+".json"),options:.atomic)
            try? bytes.write(to:cache.appendingPathComponent("selection-run-"+runID+"-"+mode.rawValue+".json"),options:.atomic)
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .white
        status.accessibilityIdentifier="selection-status"
        status.isAccessibilityElement=true
        status.titleLabel?.numberOfLines=2;status.titleLabel?.font = .systemFont(ofSize:12)
        status.backgroundColor = .white;status.setTitleColor(.black,for:.normal)
        status.contentHorizontalAlignment = .leading
        status.addTarget(self,action:#selector(confirmInputFocus),for:.touchUpInside)
        do {
            guard ["local.reader.ui-files","local.reader.columns"].contains(Bundle.main.bundleIdentifier ?? "") else {throw CoreFailure(message:"Selection UI requires an owned diagnostic bundle")}
            let args=CommandLine.arguments
            func value(_ key:String)->String? {guard let at=args.firstIndex(of:key),args.indices.contains(at+1) else{return nil};return args[at+1]}
            guard let selected=value("--selection-mode"),let parsed=ReadingMode(rawValue:selected),
                  let identity=value("--selection-run"),UUID(uuidString:identity) != nil else {throw CoreFailure(message:"Missing selection UI case")}
            mode=parsed;runID=identity
            let files=FileManager.default
            let cache=files.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            guard let encoded=ProcessInfo.processInfo.environment["READER_SELECTION_EPUB_BASE64"],let fixture=Data(base64Encoded:encoded),!fixture.isEmpty else {throw CoreFailure(message:"Missing selection fixture from XCTest")}
            let input=cache.appendingPathComponent("selection.epub")
            try fixture.write(to:input,options:.atomic)
            root=cache.appendingPathComponent("SelectionLibrary-"+runID)
            let library=PreviewLibrary(rootOverride:root!)
            let entry:LibraryEntry=try library.client.call("import",["path":input.path])
            let book:PreviewBook=try library.client.call("open",["id":entry.id])
            let source=try book.chapter(at:0)
            guard let index=source.blocks.firstIndex(where:{($0.spans ?? []).map(\.text).joined().contains(expectedText)}) else {throw CoreFailure(message:"Selection source prefix missing")}
            expectedBlock=source.blocks[index].core_block ?? index
            let raw=(source.blocks[index].spans ?? []).map(\.text).joined()
            guard let range=raw.range(of:expectedText) else {throw CoreFailure(message:"Selection source range missing")}
            expectedStart=raw[..<range.lowerBound].unicodeScalars.count
            let presentation:NativePresentation=try library.client.call("native_presentation",["id":entry.id,"chapter":0,"policy":"mixed","scripts":false,"events":[]])
            let chapter=BookChapter(title:source.title,blocks:source.blocks.map {block in
                var value=block
                if let at=block.core_block,presentation.styles.indices.contains(at) {value.native_style=presentation.styles[at]}
                return value
            })
            let shown=PreviewBook(entry:entry,chapters:[chapter],toc:[],cover:nil)
            annotations=ReadingAnnotations(library:library,book:book)
            var options=ReaderOptions(fontSize:24,lineHeight:1.5,fontName:"Helvetica",theme:"light",sliding:mode == .slide,cjkFontName:"PingFangSC-Regular",readingMode:mode)
            options.forceSingleColumn=true
            let native=NativeReadingController(book:shown,anchor:.init(chapter:0),options:options)
            native.selected = {[weak self] in self?.selected($0)}
            addChild(native);view.addSubview(native.view);native.didMove(toParent:self);reader=native
            view.addSubview(status);status.setTitle("Preparing native selection",for:.normal)
            Task {@MainActor [weak self] in
                try? await Task.sleep(for:.milliseconds(800))
                for _ in 0..<100 {
                    guard let self else{return}
                    if self.publishTargets() {return}
                    try? await Task.sleep(for:.milliseconds(50))
                }
                self?.fail("Native selection target did not become visible")
            }
        } catch {fail((error as? CoreFailure)?.message ?? error.localizedDescription)}
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        reader?.view.frame=view.bounds
        status.frame=CGRect(x:16,y:view.bounds.height-view.safeAreaInsets.bottom-40,width:view.bounds.width-32,height:36)
        view.bringSubviewToFront(status)
    }
    private func surfaces(_ view:UIView)->[PaperSurface] {
        (view as? PaperSurface).map {[$0]} ?? view.subviews.flatMap(surfaces)
    }
    private func point(_ offset:Int,in surface:PaperSurface,page:ReadingPage)->CGPoint? {
        let frame=CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(page.text),page.range,CGPath(rect:page.textRect,transform:nil),nil)
        let lines=CTFrameGetLines(frame) as! [CTLine]
        var origins=Array(repeating:CGPoint.zero,count:lines.count);CTFrameGetLineOrigins(frame,CFRange(location:0,length:0),&origins)
        for (index,line) in lines.enumerated() {
            let range=CTLineGetStringRange(line)
            if offset<range.location || offset>=range.location+range.length {continue}
            var ascent:CGFloat=0,descent:CGFloat=0;CTLineGetTypographicBounds(line,&ascent,&descent,nil)
            let x=CTLineGetOffsetForStringIndex(line,offset,nil)
            let next=CTLineGetOffsetForStringIndex(line,offset+1,nil)
            return CGPoint(x:page.textRect.minX+origins[index].x+x+max(1,(next-x)*0.2),y:surface.bounds.height-page.textRect.minY-origins[index].y-(ascent-descent)*0.5)
        }
        return nil
    }
    private func publishTargets()->Bool {
        guard let reader,reader.isViewLoaded,let window=reader.view.window else{return false}
        for surface in surfaces(reader.view) {
            guard let page=surface.page,page.chapter==0,surface.bounds.width>0,surface.bounds.height>0,
                  let coordinate=page.content.coordinates.first(where:{$0.block==expectedBlock}) else{continue}
            let start=coordinate.start+coordinate.utf16(atScalar:expectedStart)
            let end=coordinate.start+coordinate.utf16(atScalar:expectedStart+expectedText.unicodeScalars.count)-1
            guard start>=page.range.location,end<page.range.location+page.range.length,
                  let first=point(start,in:surface,page:page),let last=point(end,in:surface,page:page) else{continue}
            let visible=surface.convert(surface.bounds,to:view).intersection(view.bounds.inset(by:view.safeAreaInsets))
            guard visible.contains(surface.convert(first,to:view)),visible.contains(surface.convert(last,to:view)) else{continue}
            // Curl pagers can retain an overlapping/back paper in their view
            // hierarchy. Only publish coordinates of the actual touch surface.
            guard window.hitTest(surface.convert(first,to:window),with:nil) === surface,
                  window.hitTest(surface.convert(last,to:window),with:nil) === surface else{continue}
            let value:[String:Any]=["mode":mode.rawValue,"start":[first.x/surface.bounds.width,first.y/surface.bounds.height],"end":[last.x/surface.bounds.width,last.y/surface.bounds.height],"expected_text":expectedText,"input_focus_confirmed":inputFocusConfirmed,"key_window":window.isKeyWindow,"foreground_scene":window.windowScene?.activationState == .foregroundActive]
            guard let bytes=try? JSONSerialization.data(withJSONObject:value),let encoded=String(data:bytes,encoding:.utf8) else{return false}
            surface.isAccessibilityElement=true;surface.accessibilityIdentifier="native-selection-paper";surface.accessibilityValue=encoded
            if let gesture=surface.gestureRecognizers?.compactMap({$0 as? UILongPressGestureRecognizer}).first,gesture !== tracedGesture {
                tracedGesture?.removeTarget(self,action:#selector(traceSelection(_:)))
                tracedGesture=gesture;gesture.addTarget(self,action:#selector(traceSelection(_:)))
            }
            if touchTrace?.view !== surface {
                if let touchTrace {touchTrace.view?.removeGestureRecognizer(touchTrace)}
                let trace=SelectionTouchTrace();trace.cancelsTouchesInView=false;trace.delaysTouchesBegan=false;trace.delaysTouchesEnded=false
                trace.record = {[weak self,weak surface] phase,touches,event in
                    guard let self,let surface else{return}
                    for touch in touches where self.touchEvents.count<512 {
                        let point=touch.location(in:surface)
                        self.touchEvents.append(["phase":phase,"time":touch.timestamp,"x":point.x,"y":point.y,"in_window":surface.window != nil,"event_time":event?.timestamp ?? 0])
                    }
                    if phase != "moved" {self.writeGestureTrace()}
                }
                surface.addGestureRecognizer(trace);touchTrace=trace
            }
            status.accessibilityValue=encoded;status.setTitle("Ready",for:.normal);writeGestureTrace();return true
        }
        return false
    }
    private func selected(_ passage:SelectedPassage) {
        guard !handled else{return};handled=true
        focusTask?.cancel()
        Task {@MainActor in
            guard let annotations else {fail("Selection annotation model missing");return}
            guard passage.text==expectedText,passage.start==ReadingAnchor(chapter:0,block:expectedBlock,charOffset:expectedStart),
                  passage.end==ReadingAnchor(chapter:0,block:expectedBlock,charOffset:expectedStart+expectedText.unicodeScalars.count) else {
                fail("Selected text/anchors differ: \(passage.text) \(passage.start) -> \(passage.end)");return
            }
            guard await annotations.highlight(passage,color:"Yellow"),let saved=annotations.value.highlights.last,
                  saved.start==passage.start,saved.end==passage.end,annotations.excerpt(saved)==expectedText else {fail("Actual annotation save/excerpt diverged");return}
            do {
                let reloaded:ReaderAnnotations=try annotations.library.client.call("annotations",["id":annotations.book.id])
                guard reloaded.highlights.contains(where:{$0.id==saved.id && $0.start==passage.start && $0.end==passage.end}) else{throw CoreFailure(message:"Selection was not persisted")}
                write(["passed":true,"mode":mode.rawValue,"run_id":runID,"text":passage.text,"start_block":saved.start_block,"start_scalar":saved.start_offset,"end_block":saved.end_block,"end_scalar":saved.end_offset,"native_touch":true,"persisted":true])
                status.setTitle("Passed: "+expectedText,for:.normal)
            } catch {fail((error as? CoreFailure)?.message ?? error.localizedDescription)}
        }
    }
    private func write(_ value:[String:Any]) {
        guard let bytes=try? JSONSerialization.data(withJSONObject:value,options:[.prettyPrinted,.sortedKeys]) else{return}
        status.accessibilityValue=String(data:bytes,encoding:.utf8)
        let cache=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask)[0]
        try? bytes.write(to:cache.appendingPathComponent("selection-ui-"+mode.rawValue+".json"),options:.atomic)
    }
    private func fail(_ message:String) {write(["passed":false,"mode":mode.rawValue,"error":message]);status.setTitle("Failed: "+message,for:.normal)}
}
// This host only reports the existing native navigation suite. XCTest enables
// the real simulator setting through Settings and restores it afterwards.
struct ReaderMotionUITestHost:UIViewControllerRepresentable {
    func makeUIViewController(context:Context)->ReaderMotionHostController {ReaderMotionHostController()}
    func updateUIViewController(_ controller:ReaderMotionHostController,context:Context) {}
}
@MainActor final class ReaderMotionHostController:UIViewController {
    private let status=UILabel()
    private var started=false
    override func viewDidLoad() {
        super.viewDidLoad();view.backgroundColor = .white
        status.accessibilityIdentifier="motion-status";status.numberOfLines=0;status.isAccessibilityElement=true
        status.text="Checking native reduced motion";status.textColor = .black
        view.addSubview(status)
    }
    override func viewDidLayoutSubviews() {super.viewDidLayoutSubviews();status.frame=view.bounds.insetBy(dx:30,dy:80)}
    override func viewDidAppear(_ animated:Bool) {
        super.viewDidAppear(animated)
        guard !started else{return};started=true
        guard ["local.reader.ui-files","local.reader.columns"].contains(Bundle.main.bundleIdentifier ?? "") else {status.text="Failed: diagnostic bundle required";return}
        guard let window=view.window else {status.text="Failed: diagnostic window missing";return}
        do {
            guard let encoded=ProcessInfo.processInfo.environment["READER_NAVIGATION_EPUB_BASE64"],let fixture=Data(base64Encoded:encoded),!fixture.isEmpty else {throw CoreFailure(message:"Missing navigation fixture from XCTest")}
            let documents=FileManager.default.urls(for:.documentDirectory,in:.userDomainMask)[0]
            try FileManager.default.createDirectory(at:documents,withIntermediateDirectories:true)
            try fixture.write(to:documents.appendingPathComponent("reading-modes.epub"),options:.atomic)
        } catch {status.text="Failed: \(error)";return}
        // The suite temporarily makes its own UIKit windows key. Keep this host
        // alive until it returns and explicitly refresh accessibility on iOS 26.
        ReadingModesRuntimeCheck.run(reducedMotion:true) {[self,window] report in
            let message = report["passed"] as? Bool == true ? "Passed native reduced motion" : "Failed native reduced motion"
            status.text=message;status.accessibilityLabel=message
            if let data=try? JSONSerialization.data(withJSONObject:report,options:.sortedKeys) {status.accessibilityValue=String(data:data,encoding:.utf8)}
            window.makeKeyAndVisible()
            view.setNeedsLayout();view.layoutIfNeeded()
            UIAccessibility.post(notification:.screenChanged,argument:status)
        }
    }
}
#endif
