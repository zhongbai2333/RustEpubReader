#if DEBUG
import Foundation
import UIKit
import CoreText
import SwiftUI

// Exercises the same UIKit controllers used by the reading screen, with a
// caller-supplied EPUB in this diagnostic installation's Documents folder.
enum ReadingModesRuntimeCheck {
    @MainActor static func run() {
        Task { @MainActor in
            var report: [String: Any] = [:]
            var window: UIWindow?
            do {
                for _ in 0..<50 {
                    if UIApplication.shared.connectedScenes.contains(where: { $0 is UIWindowScene }) { break }
                    try await Task.sleep(for: .milliseconds(100))
                }
                guard let scene = UIApplication.shared.connectedScenes.first(where: { $0 is UIWindowScene }) as? UIWindowScene else { throw CoreFailure(message: "No window scene") }
                let files = FileManager.default
                let documents = files.urls(for: .documentDirectory, in: .userDomainMask)[0]
                let caches = files.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                let root = caches.appendingPathComponent("ReadingModesLibrary")
                let client = CoreClient(root: root.path, device: "reading-modes-check")
                let entry: LibraryEntry = try client.call("import", ["path": documents.appendingPathComponent("reading-modes.epub").path])
                let book: PreviewBook = try client.call("open", ["id": entry.id])
                if CommandLine.arguments.contains("--verify-navigation") {
                    report = try await verifyNavigation(scene: scene, book: book)
                    report["passed"] = true
                    let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
                    try data.write(to: caches.appendingPathComponent("navigation-verification.json"), options: .atomic)
                    return
                }
                if CommandLine.arguments.contains("--verify-chapter-loading") {
                    report=try await verifyChapterLoading(scene:scene,book:book)
                    report["passed"]=true
                    let data=try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys])
                    try data.write(to:caches.appendingPathComponent("chapter-loading-verification.json"),options:.atomic)
                    return
                }
                if CommandLine.arguments.contains("--verify-page-footers") {
                    report = try await verifyPageFooters(scene: scene, book: book)
                    report["passed"] = true
                    let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
                    try data.write(to: caches.appendingPathComponent("page-footers-verification.json"), options: .atomic)
                    return
                }
                report["lazy_paging"] = try await verifyLazyPaging(scene: scene, entry: entry)
                report["full_book_search"] = try verifySearch(entry: entry)
                report["background_image"] = try verifyBackground(scene: scene, book: book)
                report["mixed_held_pan"] = try verifyMixedHeldPan(scene: scene, entry: entry)
                report["bounded_scroll"] = try await verifyBoundedScroll(scene: scene, entry: entry)
                report["short_chapters"] = try await verifyShortChapters(scene: scene, entry: entry)
                guard book.chapters.count >= 24 else { throw CoreFailure(message: "Fixture needs at least 24 chapters") }
                let initial = ReadingAnchor(chapter: 19, block: 1, charOffset: 3)
                var options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: false, cjkFontName: "PingFangSC-Regular", readingMode: .scroll)
                let controller = NativeReadingController(book: book, anchor: initial, options: options)
                let host = UIViewController()
                let checkWindow = UIWindow(windowScene: scene); window = checkWindow
                checkWindow.rootViewController = host; checkWindow.makeKeyAndVisible()
                host.addChild(controller); host.view.addSubview(controller.view); controller.didMove(toParent: host)
                controller.committed = { anchor in
                    let _: LibraryEntry? = try? client.call("save", ["id": book.id, "chapter": anchor.chapter, "block": anchor.block, "char_offset": anchor.charOffset])
                }
                @MainActor func layout(_ width: CGFloat, _ height: CGFloat) {
                    controller.view.frame = CGRect(x: 0, y: 0, width: width, height: height)
                    controller.view.setNeedsLayout(); controller.view.layoutIfNeeded()
                }
                func require(_ condition: Bool, _ message: String) throws { if !condition { throw CoreFailure(message: message) } }
                @MainActor func snapshot(_ name: String) {
                    let renderer = UIGraphicsImageRenderer(size: controller.view.bounds.size)
                    let image = renderer.image { _ in controller.view.drawHierarchy(in: controller.view.bounds, afterScreenUpdates: true) }
                    try? image.pngData()?.write(to: caches.appendingPathComponent(name))
                }
                layout(402, 874)
                try require(controller.anchor == initial, "Initial scroll anchor lost")
                for page in controller.pages {
                    let continuous = ReadingLayout.continuousPage(page)
                    let frame = CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(continuous.text), continuous.range, CGPath(rect: continuous.textRect, transform: nil), nil)
                    try require(CTFrameGetVisibleStringRange(frame).length == page.range.length, "Styled continuous row clipped text")
                }
                guard let table = controller.view.subviews.first(where: { $0 is ReadingScrollViewport }) as? ReadingScrollViewport else { throw CoreFailure(message: "No native continuous table") }
                try require(table.frame.maxY <= controller.view.bounds.height - controller.verticalInset + 0.5, "Scroll content entered footer area")
                @MainActor func scrollToChapter(_ chapter: Int) throws {
                    guard let index = controller.pages.firstIndex(where: { $0.chapter == chapter }) else { throw CoreFailure(message: "Missing chapter") }
                    let row = table.rectForRow(at: IndexPath(row: index, section: 0))
                    table.setContentOffset(CGPoint(x: 0, y: row.minY), animated: false)
                    table.layoutIfNeeded(); controller.scrollViewDidEndDecelerating(table)
                    try require(controller.anchor.chapter == chapter, "Scroll did not reach chapter \(chapter)")
                }
                let first = controller.pages.firstIndex(where: { $0.chapter == 19 })!
                let second = controller.pages.firstIndex(where: { $0.chapter == 20 })!
                let end = table.rectForRow(at: IndexPath(row: second - 1, section: 0)).maxY
                let start = table.rectForRow(at: IndexPath(row: second, section: 0)).minY
                try require(abs(end - start) < 0.5 && second > first, "Chapter rows are discontinuous")
                table.setContentOffset(CGPoint(x: 0, y: start - 80), animated: false); table.layoutIfNeeded()
                snapshot("scroll-chapter-boundary.png")
                try scrollToChapter(20); try scrollToChapter(19)
                let saved: [LibraryEntry] = try client.call("library")
                try require(saved.first(where: { $0.id == book.id })?.lastChapter == 19, "Reverse scroll progress not persisted")
                let target = ReadingAnchor(chapter: 21, block: 1, charOffset: 3)
                controller.update(options: options, jump: target, jumpID: 1, annotations: .empty, revision: 0)
                try require(controller.anchor == target, "Directory jump lost scalar anchor")
                for _ in 0..<3 {
                    for mode in [ReadingMode.curl, .slide, .cover, .none, .scroll] {
                        options.readingMode = mode; options.sliding = mode != .curl
                        controller.update(options: options, jump: nil, jumpID: 1, annotations: .empty, revision: 0)
                        try require(controller.anchor == target, "Mode switch lost anchor")
                        try require((controller.pager == nil) == (mode == .scroll), "Wrong renderer for mode")
                    }
                }
                for mode in [ReadingMode.cover, .none] {
                    options.readingMode=mode; options.sliding=true
                    controller.update(options:options,jump:target,jumpID:1,annotations:.empty,revision:0)
                    let start=controller.currentIndex
                    try require(controller.pager?.dataSource == nil,"Custom animation retained native sliding gestures")
                    controller.turnSpread(forward:true)
                    try await Task.sleep(for:.milliseconds(300))
                    try require(controller.currentIndex == start+1,"Custom animation did not advance one leaf")
                    controller.turnSpread(forward:false)
                    try await Task.sleep(for:.milliseconds(300))
                    try require(controller.currentIndex == start,"Custom animation did not restore previous leaf")
                    snapshot(mode.rawValue+"-turn.png")
                }
                controller.update(options:options,jump:target,jumpID:2,annotations:.empty,revision:0)
                options.readingMode = .slide; options.sliding = true
                controller.update(options: options, jump: nil, jumpID: 1, annotations: .empty, revision: 0)
                layout(874, 402)
                try require(controller.dual && controller.anchor == target, "Landscape slide spread failed")
                snapshot("landscape-slide.png")
                guard let pager = controller.pager, let spread = pager.viewControllers?.first as? SpreadController else { throw CoreFailure(message: "Slide did not contain two leaves") }
                try require(spread.leaves.count == 2 && spread.leaves[0].view.frame.maxX == spread.leaves[1].view.frame.minX, "Spread leaves overlap")
                let next = controller.pageViewController(pager, viewControllerAfter: spread) as? SpreadController
                try require(next?.index == spread.index + 2, "Slide skipped or duplicated a leaf")
                controller.pageViewController(pager, didFinishAnimating: true, previousViewControllers: [spread], transitionCompleted: false)
                try require(controller.anchor == target, "Canceled slide committed progress")
                options.readingMode = .curl; options.sliding = false
                controller.update(options: options, jump: nil, jumpID: 1, annotations: .empty, revision: 0)
                try require(controller.dual && controller.pager?.viewControllers?.count == 2 && controller.pager?.isDoubleSided == true, "Landscape curl spread failed")
                try await Task.sleep(for: .milliseconds(250))
                snapshot("landscape-curl.png")
                options.fontSize = 28; options.pageMargin = 48
                controller.update(options: options, jump: nil, jumpID: 1, annotations: .empty, revision: 0)
                layout(750, 402)
                try require(!controller.dual && controller.anchor == target, "Large font fallback failed")
                layout(402, 874)
                try require(!controller.dual && controller.anchor == target, "Portrait reflow anchor failed")
                let cachedBuilds = controller.pagingTypesetCount
                options.nativeRevision += 1
                controller.update(options: options, jump:nil, jumpID:1, annotations:.empty, revision:0)
                try require(controller.pagingTypesetCount == cachedBuilds, "Unchanged chapters were typeset again")
                options.readingMode = .scroll
                controller.update(options: options, jump: nil, jumpID: 1, annotations: .empty, revision: 0)
                let highlight = SpeechHighlight(start: .init(chapter: 23, block: 1), end: .init(chapter: 23, block: 2))
                controller.updateSpeech(highlight, following: true)
                try require(controller.anchor == highlight.start, "Native scroll speech follow failed")
                let lazy = report["lazy_paging"]!
                let search = report["full_book_search"]!
                let background = report["background_image"]!
                let bounded = report["bounded_scroll"]!
                let short = report["short_chapters"]!
                let mixed = report["mixed_held_pan"]!
                report = ["lazy_paging": lazy, "full_book_search": search, "background_image": background, "mixed_held_pan": mixed, "short_chapters": short, "bounded_scroll": bounded, "passed": true, "paged_chapter_cache": true, "chapter_count": book.chapters.count, "continuous_boundary": true, "styled_row_coverage": true, "footer_reserved": true,
                    "forward_and_reverse": true, "saved_progress": true, "directory_jump": true,
                    "mode_round_trips": 15, "cover_forward_reverse": true, "no_animation_forward_reverse": true, "landscape_slide": true, "landscape_curl": true,
                    "large_font_single_page": true, "portrait_anchor": true, "cancelled_turn": true, "speech_follow": true]
            } catch { report = ["passed": false, "error": String(describing: error)] }
            let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            if let data = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]) { try? data.write(to: caches.appendingPathComponent(CommandLine.arguments.contains("--verify-navigation") ? "navigation-verification.json" : CommandLine.arguments.contains("--verify-chapter-loading") ? "chapter-loading-verification.json" : CommandLine.arguments.contains("--verify-page-footers") ? "page-footers-verification.json" : "reading-modes-verification.json"), options: .atomic) }
            window?.isHidden = true
        }
    }
    @MainActor private static func verifyNavigation(scene: UIWindowScene, book: PreviewBook) async throws -> [String: Any] {
        func require(_ value: Bool, _ message: String) throws { if !value { throw CoreFailure(message: message) } }
        let host = UIViewController(), window = UIWindow(windowScene: scene)
        window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let origin = ReadingAnchor(chapter: 19, block: 3, charOffset: 37)
        let remote = ReadingAnchor(chapter: 21, block: 3, charOffset: 3)
        let sameChapter = ReadingAnchor(chapter: 21, block: 3, charOffset: 80)
        var cases = [[String: Any]]()
        for mode in [ReadingMode.curl, .slide, .cover, .none, .scroll] {
            var options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: mode != .curl, cjkFontName: "PingFangSC-Regular", readingMode: mode)
            let reader = NativeReadingController(book: book, anchor: origin, options: options)
            host.addChild(reader); host.view.addSubview(reader.view); reader.didMove(toParent: host)
            reader.view.frame = CGRect(x: 0, y: 0, width: 402, height: 874); reader.view.layoutIfNeeded()
            var completed = 0
            reader.navigated = { _, token in completed = token }
            var history = ReadingJumpHistory()
            history.record(.init(anchor: origin, chapterTitle: book.chapters[origin.chapter].title, pageLabel: reader.pageInfo(for: reader.currentIndex)), target: remote)
            for (index, target) in [remote, sameChapter, origin].enumerated() {
                reader.update(options: options, jump: target, jumpID: index + 1, annotations: .empty, revision: 0)
                for _ in 0..<100 {
                    if completed == index + 1 { break }
                    try await Task.sleep(for: .milliseconds(20))
                }
                try require(completed == index + 1 && reader.anchor == target, "Navigation completion lost exact anchor in \(mode.rawValue)")
                try require(reader.pages[reader.currentIndex].chapter == target.chapter, "Rendered page and saved chapter differ")
            }
            try require(history.pop()?.anchor == origin && history.previous == nil, "Return did not consume history")
            options.fontSize = 26; options.pageMargin = 40
            reader.update(options: options, jump: nil, jumpID: 3, annotations: .empty, revision: 0)
            try require(reader.anchor == origin, "Reflow changed stored origin")
            cases.append(["mode": mode.rawValue, "cross_chapter": true, "same_chapter_offset": true, "precise_return": true, "reflow": true])
            reader.willMove(toParent: nil); reader.view.removeFromSuperview(); reader.removeFromParent()
        }
        // Render the actual production reading workspace with a persisted return point.
        let library = PreviewLibrary()
        let entry: LibraryEntry = try library.client.call("import", ["path": book.entry.path])
        await library.reload()
        guard let preview = await library.open(entry) else { throw CoreFailure(message: "Cannot open preview") }
        library.save(remote, for: preview); await library.flush()
        var history = ReadingJumpHistory()
        history.record(.init(anchor: origin, chapterTitle: book.chapters[origin.chapter].title, pageLabel: L("reader.page_info", 3, 9)), target: remote)
        library.saveJumpHistory(history, for: preview.id)
        try require(library.jumpHistory(for: preview.id) == history, "Persisted history lost scalar position")
        UserDefaults.standard.set(preview.id, forKey: "reader.activeBookID")
        let account = GitHubAccount(library: library)
        let screen = UIHostingController(rootView: ReadingWorkspace(book: preview, library: library, account: account).environmentObject(AppearanceCoordinator()))
        window.rootViewController = screen
        try await Task.sleep(for: .milliseconds(700))
        let revision = library.progressRevision
        let synced = ReadingAnchor(chapter: 25, block: 3, charOffset: 8)
        // Seed the position that the sync layer has accepted before delivering its UI offer.
        let _: LibraryEntry = try library.client.call("save", ["id": preview.id, "chapter": synced.chapter, "block": synced.block, "char_offset": synced.charOffset])
        library.offer = CloudProgress(book_hash: String(repeating: "a", count: 64), chapter: synced.chapter, block: synced.block, char_offset: synced.charOffset, chapter_title: book.chapters[synced.chapter].title, timestamp: UInt64(Date().timeIntervalSince1970))
        for _ in 0..<150 {
            if library.anchors[preview.id] == synced { break }
            try await Task.sleep(for: .milliseconds(20))
        }
        try require(library.anchors[preview.id] == synced, "Workspace did not automatically apply remote offer")
        try require(library.progressRevision == revision, "Accepting a remote offer created a local revision")
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        @MainActor func snapshot(_ name: String) throws {
            screen.view.layoutIfNeeded()
            let renderer = UIGraphicsImageRenderer(size: screen.view.bounds.size)
            let image = renderer.image { _ in screen.view.drawHierarchy(in: screen.view.bounds, afterScreenUpdates: true) }
            try image.pngData()?.write(to: caches.appendingPathComponent(name))
        }
        try await Task.sleep(for: .milliseconds(400))
        try snapshot("navigation-expanded.png")
        // Wait for both the disclosure and the system glass transition to settle.
        try await Task.sleep(for: .milliseconds(4000))
        try snapshot("navigation-reader.png")
        return ["cases": cases, "persistent_history": true, "automatic_remote_offer": true, "no_local_echo": true, "preview_id": preview.id, "preview_anchor": [25, 3, 8]]
    }

    @MainActor private static func verifyChapterLoading(scene:UIWindowScene,book:PreviewBook) async throws -> [String:Any] {
        func require(_ value:Bool,_ message:String)throws {if !value {throw CoreFailure(message:message)}}
        try require(book.loadsChapters,"Open returned an eager publication")
        try require(book.chapters.allSatisfy {$0.blocks.isEmpty},"Catalogue contained chapter bodies")
        let source=book.chapterSource!
        try require(source.residentCount==0,"Open preloaded every chapter")
        let speech=ReadingSpeech(book:book)
        try require(source.residentCount==0 && speech.units.isEmpty,"Speech eagerly loaded publication")
        let host=UIViewController(),window=UIWindow(windowScene:scene)
        window.rootViewController=host;window.makeKeyAndVisible();defer {window.isHidden=true;speech.stop()}
        var cases=[[String:Any]]()
        let targets=Array(Set([min(1,book.chapters.count-1),book.chapters.count/2,book.chapters.count-1])).sorted()
        for mode in [ReadingMode.curl,.slide,.cover,.none,.scroll] {
            for target in targets {
                let options=ReaderOptions(fontSize:18,lineHeight:1.65,fontName:"Helvetica",theme:"light",sliding:mode != .curl,cjkFontName:"PingFangSC-Regular",readingMode:mode)
                let chapter=try book.chapter(at:target)
                try require(!chapter.blocks.isEmpty,"Real chapter body missing")
                let reader=NativeReadingController(book:book,anchor:.init(chapter:target),options:options)
                host.addChild(reader);host.view.addSubview(reader.view);reader.didMove(toParent:host)
                reader.view.frame=CGRect(x:0,y:0,width:402,height:874);reader.view.layoutIfNeeded()
                try require(reader.anchor.chapter==target && reader.pages.contains(where:{$0.chapter==target}),"Reader lost requested chapter")
                try require(source.residentCount<=32,"Decoded chapter cache grew without bound")
                cases.append(["mode":mode.rawValue,"chapter":target,"pages":reader.pages.count,"resident_chapters":source.residentCount])
                reader.willMove(toParent:nil);reader.view.removeFromSuperview();reader.removeFromParent()
            }
        }
        let last=book.chapters.count-1
        try require(!speech.preview(at:.init(chapter:last)).isEmpty,"Lazy speech cannot preview last chapter")
        try require(source.residentCount<=32,"Speech loaded all chapters")
        return ["chapter_count":book.chapters.count,"catalogue_only":true,"bounded_body_cache":true,"speech_not_eager":true,"last_chapter_speech_preview":true,"cases":cases]
    }
    @MainActor private static func pageInfo(_ leaf: PaperController, reader: NativeReadingController) -> String {
        guard let chapter=leaf.page?.chapter else {return ""}
        let chapterPages=reader.pages.enumerated().filter {$0.element.chapter == chapter}
        return L("reader.page_info", chapterPages.firstIndex(where: {$0.offset == leaf.index})!+1, chapterPages.count)
    }
    @MainActor private static func verifyLazyPaging(scene: UIWindowScene, entry: LibraryEntry) async throws -> [[String: Any]] {
        func require(_ value: Bool, _ message: String) throws { if !value { throw CoreFailure(message: message) } }
        let block = BookBlock(type: "Paragraph", core_block: 0, core_char_offset: 0, level: nil,
            spans: [BookSpan(text: "短章测试。Native paging retains every chapter.", style: "Normal", link_url: nil)], data: nil, alt: nil)
        let chapter = BookChapter(title: "Paging chapter", blocks: [block])
        let book = PreviewBook(entry: entry, chapters: Array(repeating: chapter, count: 3200), toc: [], cover: nil)
        let host = UIViewController(), window = UIWindow(windowScene: scene)
        window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true }
        var results = [[String: Any]]()
        for width: CGFloat in [402, 874] {
            for mode in [ReadingMode.curl, .slide, .cover, .none] {
                var options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: mode != .curl, cjkFontName: "PingFangSC-Regular", readingMode: mode)
                let reader = NativeReadingController(book: book, anchor: .init(chapter: 1600), options: options)
                host.addChild(reader); host.view.addSubview(reader.view); reader.didMove(toParent: host)
                reader.view.frame = CGRect(x: 0, y: 0, width: width, height: width == 402 ? 874 : 402)
                let start = CACurrentMediaTime(); reader.view.layoutIfNeeded()
                let elapsed = (CACurrentMediaTime()-start)*1000
                try require(reader.pagingTypesetCount < 8, "Initial paging typeset the whole book")
                let stride = reader.dual ? 2 : 1
                for _ in 0..<4 {
                    let before = reader.anchor
                    if mode.customTransition {
                        reader.turnSpread(forward: false)
                        for _ in 0..<100 {
                            if reader.anchor.chapter != before.chapter { break }
                            try await Task.sleep(for: .milliseconds(20))
                        }
                    } else {
                        guard let pager = reader.pager, let visible = pager.viewControllers?.first else {throw CoreFailure(message: "Missing lazy pager")}
                        guard var previous = reader.pageViewController(pager, viewControllerBefore: visible) else {throw CoreFailure(message: "Missing preceding chapter")}
                        var right: UIViewController?
                        if reader.dual && !options.sliding {
                            right = previous
                            previous = reader.pageViewController(pager, viewControllerBefore: previous)!
                        }
                        let leaves = (previous as? SpreadController)?.leaves ?? [previous as! PaperController]
                        try require(leaves.first?.page?.chapter == before.chapter-stride, "Prepend skipped a chapter")
                        reader.pageViewController(pager, didFinishAnimating: true, previousViewControllers: [visible], transitionCompleted: false)
                        try require(reader.anchor == before, "Cancelled boundary turn saved progress")
                        if reader.dual && !options.sliding {
                            pager.setViewControllers([previous, right!], direction: .reverse, animated: false)
                        } else { pager.setViewControllers([previous], direction: .reverse, animated: false) }
                        reader.pageViewController(pager, didFinishAnimating: true, previousViewControllers: [visible], transitionCompleted: true)
                    }
                    try require(reader.anchor.chapter == before.chapter-stride, "Lazy reverse \(mode.rawValue) width \(width): expected \(before.chapter-stride), got \(reader.anchor.chapter), index \(reader.currentIndex)")
                    try require(reader.pages[reader.currentIndex].anchor.chapter == reader.anchor.chapter, "Rebased page index lost its content")
                }
                for _ in 0..<6 {
                    let before = reader.anchor
                    if mode.customTransition {
                        reader.turnSpread(forward: true)
                        for _ in 0..<100 { if reader.anchor.chapter != before.chapter { break }; try await Task.sleep(for: .milliseconds(20)) }
                    } else {
                        guard let pager = reader.pager, let visible = pager.viewControllers else {throw CoreFailure(message: "Missing forward pager")}
                        let source = reader.dual && !options.sliding ? visible.last! : visible.first!
                        guard let next = reader.pageViewController(pager, viewControllerAfter: source) else {throw CoreFailure(message: "Missing following chapter")}
                        if reader.dual && !options.sliding {
                            let right = reader.pageViewController(pager, viewControllerAfter: next)!
                            pager.setViewControllers([next, right], direction: .forward, animated: false)
                        } else { pager.setViewControllers([next], direction: .forward, animated: false) }
                        reader.pageViewController(pager, didFinishAnimating: true, previousViewControllers: visible, transitionCompleted: true)
                    }
                    try require(reader.anchor.chapter == before.chapter+stride, "Append skipped a chapter in \(mode.rawValue)")
                }
                let target = ReadingAnchor(chapter: 2400)
                reader.update(options: options, jump: target, jumpID: 1, annotations: .empty, revision: 0)
                try require(reader.anchor == target && reader.residentPagedChapters.count < 8, "Directory jump loaded intervening chapters")
                let builds = reader.pagingTypesetCount
                options.nativeRevision += 1
                reader.update(options: options, jump: nil, jumpID: 1, annotations: .empty, revision: 0)
                try require(reader.pagingTypesetCount == builds, "Unchanged lazy chapter was typeset again")
                let speech = SpeechHighlight(start: .init(chapter: 2500, block: 0), end: .init(chapter: 2500, block: 0, charOffset: 2))
                reader.updateSpeech(speech, following: true)
                try require(reader.anchor == speech.start && reader.pages[reader.currentIndex].chapter == 2500, "Speech follow missed unloaded chapter")
                // Prepend one odd leaf, then reach the real book start without
                // displaying the first page twice in a two-page sliding spread.
                reader.update(options: options, jump: .init(chapter: 1), jumpID: 2, annotations: .empty, revision: 0)
                if mode.customTransition {
                    reader.turnSpread(forward: false)
                    for _ in 0..<100 { if reader.anchor.chapter == 0 { break }; try await Task.sleep(for: .milliseconds(20)) }
                    try require(reader.anchor.chapter == 0, "Book start \(mode.rawValue) width \(width), anchor \(reader.anchor.chapter), index \(reader.currentIndex), chapters \(reader.residentPagedChapters)")
                    reader.turnSpread(forward: true)
                    for _ in 0..<100 { if reader.anchor.chapter == 1 { break }; try await Task.sleep(for: .milliseconds(20)) }
                    try require(reader.anchor.chapter == 1, "Leading partial spread skipped a chapter")
                } else if let pager = reader.pager, let visible = pager.viewControllers?.first,
                          let previous = reader.pageViewController(pager, viewControllerBefore: visible) {
                    let leaves = (previous as? SpreadController)?.leaves ?? [previous as! PaperController]
                    try require(leaves.first?.page?.chapter == 0, "Native book start inaccessible")
                    if let spread = previous as? SpreadController {
                        let after = reader.pageViewController(pager, viewControllerAfter: spread) as? SpreadController
                        try require(after?.leaves.first?.page?.chapter == 1, "Partial spread repeated or skipped a chapter")
                    }
                    if reader.dual && !options.sliding {
                        guard let blank = reader.pageViewController(pager, viewControllerBefore: previous) as? PaperController else {throw CoreFailure(message: "No leading curl sentinel")}
                        try require(blank.page == nil && blank.footerText.isEmpty, "Leading curl sentinel contains duplicated content")
                        pager.setViewControllers([blank, previous], direction: .reverse, animated: false)
                        reader.pageViewController(pager, didFinishAnimating: true, previousViewControllers: [visible], transitionCompleted: true)
                        try require(reader.anchor.chapter == 0 && reader.currentIndex == 0, "Leading curl did not commit first content page")
                        let next = reader.pageViewController(pager, viewControllerAfter: previous)!
                        let right = reader.pageViewController(pager, viewControllerAfter: next)!
                        pager.setViewControllers([next, right], direction: .forward, animated: false)
                        reader.pageViewController(pager, didFinishAnimating: true, previousViewControllers: [blank, previous], transitionCompleted: true)
                        try require(reader.anchor.chapter == 1, "Curl skipped after leading blank")
                    }
                }
                results.append(["mode": mode.rawValue, "width": Int(width), "initial_layout_ms": elapsed,
                    "initial_typeset_chapters_under_8": true, "reverse_boundaries": 4, "forward_boundaries": 6, "cancelled_boundary": true,
                    "directory_jump": true, "speech_follow": true, "book_start": true])
                reader.willMove(toParent: nil); reader.view.removeFromSuperview(); reader.removeFromParent()
            }
        }
        return results
    }
    @MainActor private static func verifyPageFooters(scene: UIWindowScene, book: PreviewBook) async throws -> [String: Any] {
        func require(_ value: Bool, _ message: String) throws { if !value { throw CoreFailure(message: message) } }
        let host = UIViewController(), window = UIWindow(windowScene: scene)
        window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true }
        var results = [[String: Any]]()
        for mode in [ReadingMode.curl, .slide, .cover, .none] {
            var options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: mode != .curl, cjkFontName: "PingFangSC-Regular", readingMode: mode)
            let reader = NativeReadingController(book: book, anchor: .init(chapter: 0), options: options)
            host.addChild(reader); host.view.addSubview(reader.view); reader.didMove(toParent: host)
            defer { reader.willMove(toParent: nil); reader.view.removeFromSuperview(); reader.removeFromParent() }
            for size in [CGSize(width: 402, height: 874), CGSize(width: 874, height: 402)] {
                reader.view.frame = CGRect(origin: .zero, size: size)
                reader.view.setNeedsLayout(); reader.view.layoutIfNeeded()
                guard let pager = reader.pager else { throw CoreFailure(message: "No paged controller") }
                pager.view.layoutIfNeeded()
                let leaves = pager.viewControllers?.flatMap { ($0 as? SpreadController)?.leaves ?? [($0 as? PaperController)].compactMap { $0 } } ?? []
                try require(leaves.count == (reader.dual ? 2 : 1), "Wrong spread leaf count")
                for leaf in leaves where leaf.page != nil {
                    leaf.view.layoutIfNeeded()
                    let footer = leaf.pageFooter, bounds = leaf.view.bounds
                    if let page = leaf.page {
                        let top = bounds.height - page.textRect.maxY
                        let bottom = page.textRect.minY
                        try require(abs(top-bottom)<0.5, "Reading body has unequal top/bottom margins: \(top)/\(bottom)")
                        try require(top >= max(window.safeAreaInsets.top,window.safeAreaInsets.bottom), "Reading body enters the system safe area")
                    }
                    try require(footer.isDescendant(of: leaf.view) && footer.text == pageInfo(leaf, reader: reader), "Footer detached from paper or stale page number")
                    try require(!footer.isHidden && bounds.contains(footer.frame) && abs(bounds.maxX-footer.frame.maxX-16)<1, "Footer not inside bottom-right paper edge")
                    try require(abs(bounds.maxY-footer.frame.maxY-leaf.footerSafeBottom-8)<1, "Footer unsafe or wrong bottom inset")
                    let image = UIGraphicsImageRenderer(bounds: bounds).image { _ in leaf.view.drawHierarchy(in: bounds, afterScreenUpdates: true) }
                    let file = "footer-\(mode.rawValue)-\(Int(size.width))-\(leaf.index).png"
                    try image.pngData()?.write(to: FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent(file))
                }
                if let first = leaves.first, let next = reader.pageViewController(pager, viewControllerAfter: first) as? PaperController, next.page != nil {
                    next.loadViewIfNeeded()
                    try require(next.pageFooter.text != first.pageFooter.text, "Incoming page reused outgoing footer")
                }
                let heldIndex = reader.currentIndex, builds = reader.pagingTypesetCount
                let status = ReadingStatusInfo(time: "09:41", battery: 73)
                reader.updateImmersiveStatus(status)
                try require(leaves.allSatisfy { $0.immersiveFooter.isDescendant(of: $0.view) && !$0.immersiveFooter.isHidden }, "Status is not owned by the paper")
                reader.updateImmersiveStatus(nil)
                try require(leaves.allSatisfy { $0.immersiveFooter.isHidden } && reader.currentIndex == heldIndex && reader.pagingTypesetCount == builds, "Status toggle repaginated or moved reading position")
                reader.updateImmersiveStatus(status)
                for leaf in leaves where leaf.page != nil {
                    leaf.view.layoutIfNeeded()
                    let image = UIGraphicsImageRenderer(bounds: leaf.view.bounds).image { _ in leaf.view.drawHierarchy(in: leaf.view.bounds, afterScreenUpdates: true) }
                    try image.pngData()?.write(to: FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("status-\(mode.rawValue)-\(Int(size.width))-\(leaf.index).png"))
                }
                let start = reader.currentIndex
                try await Task.sleep(for: .milliseconds(80))
                reader.turnSpread(forward: true)
                for _ in 0..<200 {
                    if reader.currentIndex > start { break }
                    try await Task.sleep(for: .milliseconds(20))
                }
                try require(reader.currentIndex > start, "Page turn did not complete: \(mode.rawValue), width=\(size.width), index=\(reader.currentIndex), start=\(start)")
                let visible = pager.viewControllers?.flatMap { ($0 as? SpreadController)?.leaves ?? [($0 as? PaperController)].compactMap { $0 } } ?? []
                try require(visible.allSatisfy { $0.page == nil ? $0.footerText.isEmpty : $0.footerText == pageInfo($0, reader: reader) }, "Turn left stale footer")
                results.append(["mode": mode.rawValue, "width": Int(size.width), "dual": reader.dual, "paper_owned": true, "safe_bottom_right": true, "turn_updates": true, "status_on_paper": true, "status_toggle_no_reflow": true, "symmetric_body_margins": true])
            }
            options.theme = "dark"
            reader.update(options: options, jump: nil, jumpID: 0, annotations: .empty, revision: 0)
        }
        var scrollResults = [[String: Any]]()
        let scrollOptions = ReaderOptions(fontSize:18,lineHeight:1.65,fontName:"Helvetica",theme:"light",sliding:false,cjkFontName:"PingFangSC-Regular",readingMode:.scroll)
        let scrollReader = NativeReadingController(book:book,anchor:.init(chapter:0),options:scrollOptions)
        host.addChild(scrollReader);host.view.addSubview(scrollReader.view);scrollReader.didMove(toParent:host)
        defer {scrollReader.willMove(toParent:nil);scrollReader.view.removeFromSuperview();scrollReader.removeFromParent()}
        for size in [CGSize(width:402,height:874),CGSize(width:874,height:402)] {
            scrollReader.view.frame=CGRect(origin:.zero,size:size)
            scrollReader.view.setNeedsLayout();scrollReader.view.layoutIfNeeded()
            guard let viewport=scrollReader.view.subviews.first(where:{$0 is ReadingScrollViewport}) else {throw CoreFailure(message:"Missing continuous reading viewport")}
            let top=viewport.frame.minY,bottom=scrollReader.view.bounds.maxY-viewport.frame.maxY
            try require(abs(top-bottom)<0.5,"Continuous reading has unequal top/bottom margins: \(top)/\(bottom)")
            try require(top >= max(window.safeAreaInsets.top,window.safeAreaInsets.bottom),"Continuous reading enters the system safe area")
            let anchor=scrollReader.anchor,builds=scrollReader.scrollTypesetCount
            scrollReader.updateImmersiveStatus(ReadingStatusInfo(time:"09:41",battery:73))
            scrollReader.updateImmersiveStatus(nil)
            try require(scrollReader.anchor==anchor && scrollReader.scrollTypesetCount==builds,"Footer visibility moved the continuous reading position")
            scrollResults.append(["width":Int(size.width),"symmetric_body_margins":true,"top":top,"bottom":bottom,"status_toggle_no_reflow":true])
        }
        return ["scroll_margins":scrollResults,"cases": results, "count": results.count, "overlay_removed": true]
    }

    private static func verifySearch(entry: LibraryEntry) throws -> [String: Any] {
        func require(_ condition: Bool, _ message: String) throws { if !condition { throw CoreFailure(message: message) } }
        let first = "😀 e\u{301} 中文 ABC", second = " inline end"
        func block(_ text: String, core: Int, offset: Int = 0) -> BookBlock {
            BookBlock(type: "Paragraph", core_block: core, core_char_offset: offset, level: nil, spans: [BookSpan(text: text, style: "Normal", link_url: nil)], data: nil, alt: nil)
        }
        let book = PreviewBook(entry: entry, chapters: [BookChapter(title: "First", blocks: [block(first, core: 0), block(second, core: 0, offset: first.unicodeScalars.count)]), BookChapter(title: "Second", blocks: [block("Another abc", core: 2)])], toc: [], cover: nil)
        let hits = ReaderBookSearch.find(" abc ", in: book)
        try require(hits.hits.count == 2 && hits.hits[1].chapter == 1 && hits.hits[1].block == 2, "Search did not cover entire book or trim/case-fold query")
        let expected = first.unicodeScalars.distance(from: first.unicodeScalars.startIndex, to: first.range(of: "ABC")!.lowerBound)
        try require(hits.hits[0].offset == expected, "Search used byte or grapheme offset instead of scalar coordinate")
        let crossing = ReaderBookSearch.find("ABC inline", in: book)
        try require(crossing.hits.count == 1 && crossing.hits[0].offset == expected && crossing.hits[0].block == 0, "Search lost inline-image fragment coordinates")
        try require(ReaderBookSearch.find("é", in: book).hits.first?.offset == 2 && ReaderBookSearch.find("😀", in: book).hits.first?.offset == 0, "Canonical Unicode/emoji match failed")
        let bounded = ReaderBookSearch.find("abc", in: book, limit: 1)
        try require(bounded.hits.count == 1 && bounded.hasMore && ReaderBookSearch.find(" ", in: book).hits.isEmpty && ReaderBookSearch.find("missing", in: book).hits.isEmpty, "Search limits or empty states failed")
        return ["all_chapters": true, "case_insensitive": true, "scalar_offsets": true, "inline_image_fragments": true, "combining_accents_and_emoji": true, "bounded_results": true]
    }

    @MainActor private static func verifyBackground(scene: UIWindowScene, book: PreviewBook) throws -> [String: Any] {
        func require(_ condition: Bool, _ message: String) throws { if !condition { throw CoreFailure(message: message) } }
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let input = UIGraphicsImageRenderer(size: CGSize(width: 4096, height: 1024), format: format).image { context in
            UIColor(red: 0.74, green: 0.83, blue: 0.79, alpha: 1).setFill(); context.fill(CGRect(x: 0, y: 0, width: 4096, height: 1024))
            UIColor(red: 0.36, green: 0.57, blue: 0.49, alpha: 1).setFill(); context.fill(CGRect(x: 2048, y: 0, width: 2048, height: 1024))
        }
        let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let source = documents.appendingPathComponent("background-runtime-fixture.png")
        try input.pngData()!.write(to: source, options: .atomic)
        let name = try ReaderBackgroundStore.importFile(source)
        defer { ReaderBackgroundStore.remove(name) }
        guard let file = ReaderBackgroundStore.url(name), let persisted = UIImage(contentsOfFile: file.path), let image = ReaderBackgroundStore.image(name) else { throw CoreFailure(message: "Background image was not persisted") }
        try require(persisted.size == CGSize(width: 2048, height: 512), "Large image was not downsampled")
        var invalidRejected = false
        do { _ = try ReaderBackgroundStore.importImage(Data("not an image".utf8)) } catch { invalidRejected = true }
        try require(invalidRejected && FileManager.default.fileExists(atPath: file.path), "Failed import changed existing background")
        try require(ReaderBackgroundStore.image("../background-secret.png") == nil, "Unsafe filename accepted")
        var options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: false, readingMode: .scroll)
        let controller = NativeReadingController(book: book, anchor: .init(chapter: 19, block: 1, charOffset: 3), options: options)
        let host = UIViewController(), window = UIWindow(windowScene: scene)
        window.rootViewController = host; window.makeKeyAndVisible()
        host.addChild(controller); host.view.addSubview(controller.view); controller.didMove(toParent: host)
        controller.view.frame = CGRect(x: 0, y: 0, width: 402, height: 874); controller.view.layoutIfNeeded()
        defer { window.isHidden = true }
        guard let viewport = controller.view.subviews.first(where: { $0 is ReadingScrollViewport }) as? ReadingScrollViewport else { throw CoreFailure(message: "No background viewport") }
        controller.update(options: options, jump: nil, jumpID: 0, annotations: .empty, revision: 0)
        let anchor = controller.anchor, y = viewport.contentOffset.y, builds = controller.scrollTypesetCount
        options.backgroundImageName = name
        for opacity in [0.22, 0.8] {
            options.backgroundImageOpacity = opacity
            controller.update(options: options, jump: nil, jumpID: 0, annotations: .empty, revision: 0)
            try require(controller.anchor == anchor && abs(viewport.contentOffset.y - y) < 0.5 && controller.scrollTypesetCount == builds, "Background adjustment reflowed text or moved anchor")
            try require(viewport.backgroundColor == .clear && viewport.papers.allSatisfy { !$0.isOpaque }, "Scroll rows hid fixed background")
            let backdrop = controller.view.subviews.compactMap { $0 as? UIImageView }.first
            try require(backdrop?.image === image && abs((backdrop?.alpha ?? 0) - opacity) < 0.001, "Fixed background did not update")
        }
        for mode in [ReadingMode.slide, .curl] {
            options.readingMode = mode; options.sliding = mode != .curl
            controller.update(options: options, jump: nil, jumpID: 0, annotations: .empty, revision: 0)
            guard let leaf = controller.pager?.viewControllers?.first as? PaperController else { throw CoreFailure(message: "No paged background leaf") }
            leaf.loadViewIfNeeded()
            try require((leaf.view as? PaperSurface)?.backgroundImage === image, "Paged reader lost background")
            let pages = controller.pages.map { NSRange(location: $0.range.location, length: $0.range.length) }, previousLeaf = leaf
            options.backgroundImageOpacity = 0.4
            controller.update(options: options, jump: nil, jumpID: 0, annotations: .empty, revision: 0)
            try require(controller.anchor == anchor && controller.pages.map { NSRange(location: $0.range.location, length: $0.range.length) } == pages && controller.pager?.viewControllers?.first === previousLeaf && leaf.backgroundOpacity == 0.4, "Paged background update rebuilt pages")
            options.backgroundImageEnabled = false
            controller.update(options: options, jump: nil, jumpID: 0, annotations: .empty, revision: 0)
            try require(leaf.backgroundImage == nil && controller.anchor == anchor, "Disabling background failed")
            options.backgroundImageEnabled = true
        }
        return ["persistent_copy": true, "max_image_edge": 2048, "failed_import_preserves_image": true, "scroll_opacity_without_reflow": true, "slide_and_curl_background": true, "anchor_preserved": true]
    }

    @MainActor private static func verifyMixedHeldPan(scene: UIWindowScene, entry: LibraryEntry) throws -> [String: Any] {
        func require(_ condition: Bool, _ message: String) throws { if !condition { throw CoreFailure(message: message) } }
        let text = String(repeating: "Long chapter content 中文测试持续拖动，不松手。", count: 6)
        let blocks = (0..<24).map { i in BookBlock(type: "Paragraph", core_block: i, core_char_offset: 0, level: nil,
            spans: [BookSpan(text: text, style: "Normal", link_url: nil)], data: nil, alt: nil) }
        let long = BookChapter(title: "Long", blocks: blocks)
        let short = BookChapter(title: "Short bridge", blocks: [BookBlock(type: "Paragraph", core_block: 0, core_char_offset: 0, level: nil,
            spans: [BookSpan(text: "短章桥接。", style: "Normal", link_url: nil)], data: nil, alt: nil)])
        let publication = PreviewBook(entry: entry, chapters: (0..<32).map { $0.isMultiple(of: 2) ? long : short }, toc: [], cover: nil)
        let options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: false, cjkFontName: "PingFangSC-Regular", readingMode: .scroll)
        let controller = NativeReadingController(book: publication, anchor: .init(chapter: 0), options: options)
        let host = UIViewController(), window = UIWindow(windowScene: scene)
        window.rootViewController = host; window.makeKeyAndVisible()
        host.addChild(controller); host.view.addSubview(controller.view); controller.didMove(toParent: host)
        controller.view.frame = CGRect(x: 0, y: 0, width: 402, height: 874); controller.view.layoutIfNeeded()
        defer { window.isHidden = true }
        guard let viewport = controller.view.subviews.first(where: {$0 is ReadingScrollViewport}) as? ReadingScrollViewport else { throw CoreFailure(message: "No bridge viewport") }
        let last = controller.pages.lastIndex(where: {$0.chapter == 0})!
        let end = viewport.rectForRow(at: IndexPath(row: last, section: 0)).maxY
        controller.scrollViewWillBeginDragging(viewport)
        // The top remains in chapter 0. The bottom has passed short chapter 1 and
        // needs chapter 2 before any drag-end callback or chapter-change signal.
        let proposed = end - viewport.bounds.height * 0.3
        viewport.setContentOffset(CGPoint(x: 0, y: proposed), animated: false)
        try require(controller.anchor.chapter == 0, "Fixture did not keep the top in the original chapter")
        try require(controller.pages.contains(where: {$0.chapter == 2}), "Chapter after short bridge was not loaded while finger held and top stayed in original chapter")
        let next = viewport.papers.first(where: {$0.page?.chapter == 2 && $0.frame.intersects(viewport.bounds)})
        guard let next, next.layer.contents != nil else { throw CoreFailure(message: "Chapter after short bridge had no backing contents before release") }
        let format = UIGraphicsImageRendererFormat(); format.scale = 1; format.preferredRange = .standard
        let backingImage = UIGraphicsImageRenderer(size: next.bounds.size, format: format).image { next.layer.render(in: $0.cgContext) }
        guard let pixels = backingImage.cgImage, let bytes = pixels.dataProvider?.data, pixels.bitsPerPixel == 32,
              let data = CFDataGetBytePtr(bytes) else { throw CoreFailure(message: "No backing pixels") }
        var ink = 0
        for y in stride(from: 0, to: pixels.height, by: 2) { for x in stride(from: 0, to: pixels.width, by: 2) {
            let at = y * pixels.bytesPerRow + x * 4
            if data[at] < 160 && data[at + 1] < 160 && data[at + 2] < 160 && data[at + 3] > 160 { ink += 1 }
        } }
        try require(ink > 100, "Following chapter backing had no readable ink before release: \(ink)")
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        try backingImage.pngData()?.write(to: caches.appendingPathComponent("held-short-bridge.png"))
        try require(viewport.updatingDuringDrag && abs(viewport.contentOffset.y - proposed) < 0.5, "Bridge load interrupted held-pan coordinates")
        var checks = 1
        for _ in 0..<45 {
            let y = viewport.contentOffset.y + 24
            viewport.setContentOffset(CGPoint(x: 0, y: y), animated: false)
            for row in controller.pages.indices where viewport.rectForRow(at: IndexPath(row: row, section: 0)).intersects(viewport.bounds) {
                let page = controller.pages[row]
                try require(viewport.papers.contains(where: {$0.page?.chapter == page.chapter && $0.page?.range.location == page.range.location && $0.layer.contents != nil}), "Visible row stayed blank during uninterrupted pan")
            }
            try require(abs(viewport.contentOffset.y - y) < 0.5, "Bridge pan jumped")
            checks += 1
        }
        controller.scrollViewDidEndDragging(viewport, willDecelerate: false)
        return ["chapter_pattern": "long-short-long", "top_stayed_in_original_chapter": true, "visible_following_chapter_loaded_before_release": true,
            "backing_contents_before_release": true, "ink_pixels_before_release": ink, "uninterrupted_offset_checks": checks]
    }

    @MainActor private static func verifyShortChapters(scene: UIWindowScene, entry: LibraryEntry) async throws -> [String: Any] {
        func require(_ condition: Bool, _ message: String) throws { if !condition { throw CoreFailure(message: message) } }
        let block = BookBlock(type: "Paragraph", core_block: 0, core_char_offset: 0, level: nil,
            spans: [BookSpan(text: "短章也要连续滚动。Short chapters remain reachable.", style: "Normal", link_url: nil)], data: nil, alt: nil)
        let chapter = BookChapter(title: "Short chapter", blocks: [block])
        let options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: false, cjkFontName: "PingFangSC-Regular", readingMode: .scroll)
        let book = PreviewBook(entry: entry, chapters: Array(repeating: chapter, count: 3200), toc: [], cover: nil)
        let controller = NativeReadingController(book: book, anchor: .init(chapter: 1600), options: options)
        let host = UIViewController(), window = UIWindow(windowScene: scene)
        window.rootViewController = host; window.makeKeyAndVisible()
        host.addChild(controller); host.view.addSubview(controller.view); controller.didMove(toParent: host)
        controller.view.frame = CGRect(x: 0, y: 0, width: 402, height: 874); controller.view.layoutIfNeeded()
        defer { window.isHidden = true }
        guard let viewport = controller.view.subviews.first(where: {$0 is ReadingScrollViewport}) as? ReadingScrollViewport else { throw CoreFailure(message: "No short-chapter viewport") }
        var maximum = controller.residentScrollChapters.count
        controller.scrollViewWillBeginDragging(viewport)
        let ticks = viewport.trackingDisplayTicks
        func pumpTracking() {
            let deadline = Date(timeIntervalSinceNow: 0.1)
            while viewport.trackingDisplayTicks == ticks && Date() < deadline {
                _ = RunLoop.main.run(mode: RunLoop.Mode("UITrackingRunLoopMode"), before: deadline)
            }
        }
        pumpTracking()
        try require(viewport.trackingDisplayTicks > ticks, "Display refresh did not run while touch tracking")
        for direction in [1, -1] {
            let first = controller.anchor.chapter
            for _ in 0..<120 {
                let proposed = viewport.contentOffset.y + CGFloat(direction * 24)
                let maximumOffset = viewport.contentSize.height - viewport.bounds.height
                try require(proposed < maximumOffset, "Short chapter reached false content end")
                viewport.setContentOffset(CGPoint(x: 0, y: proposed), animated: false); viewport.layoutIfNeeded()
                try require(abs(viewport.contentOffset.y - proposed) < 0.5, "Window shift reset pan coordinates")
                try require(controller.pages.last.map { _ in viewport.rectForRow(at: IndexPath(row: controller.pages.count - 1, section: 0)).maxY >= viewport.bounds.maxY } ?? false, "Visible short chapters missing")
                let visibleChapters = Set(viewport.papers.filter {$0.frame.intersects(viewport.bounds)}.compactMap {$0.page?.chapter})
                for row in controller.pages.indices where viewport.rectForRow(at: IndexPath(row: row, section: 0)).intersects(viewport.bounds) {
                    try require(visibleChapters.contains(controller.pages[row].chapter), "New chapter did not draw while pan held")
                }
                maximum = max(maximum, controller.residentScrollChapters.count)
                try require(maximum < 60, "Short-chapter layouts grew with total book size")
            }
            try require(abs(controller.anchor.chapter - first) >= 10, "Short chapters could not advance/reverse")
        }
        controller.scrollViewDidEndDragging(viewport, willDecelerate: false)
        try require(!viewport.updatingDuringDrag, "Tracking refresh leaked after gesture end")
        for direction in [UIAccessibilityScrollDirection.down, .up] {
            let oldChapter = controller.anchor.chapter
            var previousY = viewport.contentOffset.y
            try require(viewport.accessibilityScroll(direction), "Native animated scroll rejected")
            for _ in 0..<20 {
                try await Task.sleep(for: .milliseconds(30))
                let newY = viewport.contentOffset.y
                try require(direction == .down ? newY >= previousY - 0.5 : newY <= previousY + 0.5, "Native animated scroll bounced at chapter boundary")
                previousY = newY
            }
            try require(controller.anchor.chapter != oldChapter, "Native animated scroll stopped before next short chapter")
        }
        controller.update(options: options, jump: .init(chapter: 0), jumpID: 1, annotations: .empty, revision: 0)
        let startDelta = viewport.contentOffset.y + viewport.adjustedContentInset.top
        try require(startDelta >= -0.5 && startDelta <= 32, "Book-start scroll limit wrong: \(startDelta)")
        controller.update(options: options, jump: .init(chapter: 3199), jumpID: 2, annotations: .empty, revision: 0)
        try require(viewport.contentOffset.y <= viewport.contentSize.height - viewport.bounds.height + 0.5, "Book-end scroll limit wrong")
        return ["chapters": 3200, "offset_steps": 240, "forward_and_reverse": true, "stable_pan_coordinates": true,
            "maximum_resident_chapters": maximum, "book_limits": true, "native_animation_samples": 40, "held_pan_draw_checks": 240, "tracking_mode_display_ticks": viewport.trackingDisplayTicks]
    }

    @MainActor private static func verifyBoundedScroll(scene: UIWindowScene, entry: LibraryEntry) async throws -> [[String: Any]] {
        func require(_ condition: Bool, _ message: String) throws { if !condition { throw CoreFailure(message: message) } }
        let paragraph = String(repeating: "Scrolling 😀 中文 keeps the same logical position across chapter windows. ", count: 4)
        let blocks = (0..<24).map { i in BookBlock(type: "Paragraph", core_block: i, core_char_offset: 0, level: nil,
            spans: [BookSpan(text: paragraph, style: "Normal", link_url: nil)], data: nil, alt: nil) }
        let chapter = BookChapter(title: "Bounded chapter", blocks: blocks)
        var results: [[String: Any]] = []
        for count in [32, 320, 3200] {
            let publication = PreviewBook(entry: entry, chapters: Array(repeating: chapter, count: count), toc: [], cover: nil)
            let options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "light", sliding: false, cjkFontName: "PingFangSC-Regular", readingMode: .scroll)
            let initial = ReadingAnchor(chapter: count / 2, block: 1, charOffset: 3)
            let controller = NativeReadingController(book: publication, anchor: initial, options: options)
            let host = UIViewController(), checkWindow = UIWindow(windowScene: scene)
            checkWindow.rootViewController = host; checkWindow.makeKeyAndVisible()
            let begin = CACurrentMediaTime()
            host.addChild(controller); host.view.addSubview(controller.view); controller.didMove(toParent: host)
            controller.view.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
            controller.view.setNeedsLayout(); controller.view.layoutIfNeeded()
            let elapsed = (CACurrentMediaTime() - begin) * 1000
            let initialBuilds = controller.scrollTypesetCount
            try require(initialBuilds == 3, "Initial layouts scale with book length: \(count), \(initialBuilds)")
            guard let table = controller.view.subviews.first(where: {$0 is ReadingScrollViewport}) as? ReadingScrollViewport else { throw CoreFailure(message: "Missing bounded table") }
            try await Task.sleep(for: .milliseconds(300))
            try require(controller.residentScrollChapters.count == 5, "Warm cache was not bounded at five")
            var commits: [ReadingAnchor] = []
            controller.committed = { commits.append($0) }
            let before = controller.scrollTypesetCount
            var changedLines: [ReadingAnchor] = []
            for _ in 0..<20 {
                let previous = controller.anchor
                table.setContentOffset(CGPoint(x: 0, y: table.contentOffset.y + 4), animated: false)
                if controller.anchor != previous { changedLines.append(controller.anchor) }
            }
            // Current sync semantics persist each changed logical line, so an
            // older queued callback cannot overtake newer remote progress.
            try require(controller.scrollTypesetCount == before && commits == changedLines && commits.count < 20, "Within-chapter scroll rebuilt layouts or failed logical-line saves")
            let savedLines = commits.count
            controller.scrollViewDidEndDecelerating(table)
            try require(commits.last == controller.anchor && commits.count == savedLines, "Scroll end duplicated a saved line")
            for direction in [1, -1] {
                for _ in 0..<10 {
                    let target = controller.anchor.chapter + direction
                    guard let rowIndex = controller.pages.firstIndex(where: {$0.chapter == target}) else { throw CoreFailure(message: "Neighbor missing") }
                    let page = controller.pages[rowIndex]
                    let map = ReadingLineMap(page)
                    let proposedY = table.rectForRow(at: IndexPath(row: rowIndex, section: 0)).minY + 80
                    let expected = map.anchor(at: 104)
                    let expectedPixel = table.rectForRow(at: IndexPath(row: rowIndex, section: 0)).minY + map.verticalOffset(for: expected) - proposedY
                    table.setContentOffset(CGPoint(x: 0, y: proposedY), animated: false); table.layoutIfNeeded()
                    try require(controller.anchor == expected, "Chapter shift lost logical position")
                    try require(abs(controller.scrollAnchorViewportY - expectedPixel) < 0.5, "Chapter shift jumped pixels")
                    try require(controller.residentScrollChapters.count <= 5 && Set(controller.pages.map(\.chapter)).count <= 3, "Chapter traversal grew resident window")
                }
            }
            let target = ReadingAnchor(chapter: count - 1, block: 5, charOffset: 7)
            controller.update(options: options, jump: target, jumpID: 1, annotations: .empty, revision: 0)
            try require(controller.anchor == target, "Far jump lost position")
            let savedCount = commits.count
            try await Task.sleep(for: .milliseconds(400))
            try require(commits.count == savedCount, "Stale pending save survived jump")
            var styled = options; styled.nativeRevision += 1
            let buildsBeforeStyle = controller.scrollTypesetCount
            let style = NativeStyle(align: nil, size: nil, line: nil, indent: nil, before: nil, after: nil, color: "#c02020", family: nil, bold: false, italic: false, hidden: false, action: nil, text: nil)
            controller.nativeStyles[target.chapter] = Array(repeating: style, count: blocks.count)
            controller.update(options: styled, jump: nil, jumpID: 1, annotations: .empty, revision: 0)
            try require(controller.scrollTypesetCount == buildsBeforeStyle + 1 && controller.anchor == target, "Native style update rebuilt unrelated chapters or lost anchor")
            table.setContentOffset(CGPoint(x: 0, y: table.contentOffset.y + 60), animated: false)
            NotificationCenter.default.post(name: UIApplication.willResignActiveNotification, object: nil)
            try require(commits.last == controller.anchor, "Background did not flush final anchor")
            results.append(["chapters": count, "initial_layouts": initialBuilds, "initial_ms": elapsed,
                "max_resident_chapters": 5, "visible_chapters": 3, "boundary_checks": 20, "pixel_tolerance": 0.5,
                "logical_line_save": true, "far_jump": true, "background_flush": true, "native_style_invalidation": true])
            checkWindow.isHidden = true
        }
        return results
    }

}
#endif
