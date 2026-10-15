import SwiftUI
import UIKit
import CoreText

struct ReaderWindowViewport: Equatable { var bounds: CGRect = .zero; var safe: UIEdgeInsets = .zero }

struct NativeReader: UIViewControllerRepresentable {
    let book: PreviewBook
    let initial: ReadingAnchor
    let options: ReaderOptions
    let annotations: ReaderAnnotations
    let annotationRevision: Int
    let speechHighlight: SpeechHighlight?
    let followSpeech: Bool
    let selected: (SelectedPassage) -> Void
    let jump: ReadingAnchor?
    let jumpID: Int
    let toggleTools: () -> Void
    let turning: () -> Void
    let visible: (ReadingAnchor, String) -> Void
    let committed: (ReadingAnchor) -> Void
    var publicationNote: ((Int,String)->Void)? = nil
    var publicationEvent: ((Int,Int)->Void)? = nil
    var nativeStyles: [Int: [NativeStyle]] = [:]
    var viewportChanged: (ReaderWindowViewport) -> Void = { _ in }
    var contentVisible = true
    var immersiveStatus: ReadingStatusInfo? = nil
    func makeUIViewController(context: Context) -> NativeReadingController {
        let controller=NativeReadingController(book: book, anchor: initial, options: options)
        controller.contentVisible=contentVisible
        return controller
    }
    func updateUIViewController(_ controller: NativeReadingController, context: Context) {
        controller.contentVisible = contentVisible
        controller.viewportChanged = viewportChanged
        controller.toggleTools = toggleTools; controller.turning = turning
        controller.visible = visible; controller.committed = committed
        controller.selected = selected
        controller.book = book; controller.publicationNote = publicationNote; controller.publicationEvent = publicationEvent; controller.nativeStyles = nativeStyles
        controller.update(options: options, jump: jump, jumpID: jumpID, annotations: annotations, revision: annotationRevision)
        controller.updateSpeech(speechHighlight, following: followSpeech)
        controller.updateImmersiveStatus(immersiveStatus)
    }
}

final class NativeReadingController: UIViewController, UIPageViewControllerDataSource, UIPageViewControllerDelegate, UIGestureRecognizerDelegate, UIScrollViewDelegate {
    var book: PreviewBook
    var contentVisible=true {
        didSet {if isViewLoaded {UIView.performWithoutAnimation {view.alpha=contentVisible ? 1 : 0}}}
    }
    var publicationNote: ((Int,String)->Void)? = nil
    var publicationEvent: ((Int,Int)->Void)?
    var nativeStyles: [Int: [NativeStyle]] = [:]
    var anchor: ReadingAnchor
    var options: ReaderOptions
    var pages: [ReadingPage] = []
    var annotations = ReaderAnnotations.empty
    var annotationRevision = -1
    var selected: ((SelectedPassage) -> Void)?
    var pager: UIPageViewController?
    var dual = false
    private var continuous: ReadingScrollViewport?
    private let backdrop = UIImageView()
    private let paperLeaves = NSHashTable<PaperController>.weakObjects()
    private var scrollChapterStarts: [Int: CGFloat] = [:]
    private var scrollBookStart: CGFloat?
    private var scrollBookEnd: CGFloat?
    private var scrollLoadedRange = 0..<0
    private var scrollReset = false
    private var restoringScroll = false
    private var restoredScrollOffset: CGFloat?
    private var scrollPages: [ReadingPage] = []
    private let pagingCache = ReadingChapterCache<BookChapter, [ReadingPage]>()
    private var pagingCacheOptions: ReaderOptions?
    private var pagingTextRect = CGRect.zero
    var pagingTypesetCount: Int { pagingCache.builds }
    private let scrollCache = ReadingChapterCache<BookChapter, ScrollChapterLayout>()
    private var scrollActive: [Int: ScrollChapterLayout] = [:]
    private var scrollRowIndices: [Int] = []
    private var scrollCenter = -1
    private var scrollCacheOptions: ReaderOptions?
    private var scrollTextRect = CGRect.zero
    private var initialVisibilityPending: DispatchWorkItem?
    private var firstContentReported = false
    var viewportChanged: ((ReaderWindowViewport) -> Void)?
    private var reportedViewport = ReaderWindowViewport()
    private var pendingScrollSave: DispatchWorkItem?
    private var pendingWarmup: DispatchWorkItem?
    private var lastSavedScrollAnchor: ReadingAnchor?
    var residentScrollChapters: [Int] { scrollCache.chapters }
    var scrollTypesetCount: Int { scrollCache.builds }
    var scrollAnchorViewportY: CGFloat { scrollAnchorPixelY() }
    var currentIndex = 0
    var lastSize: CGSize = .zero
    var layoutOptions: ReaderOptions?
    var lastJumpID = 0
    var verticalInset: CGFloat = 80
    private var contentTopInset: CGFloat = 80
    private var horizontalInset: CGFloat = 0
    private var footerSafeBottom: CGFloat = 0
    private var immersiveStatus: ReadingStatusInfo?
    private var readerFrame: CGRect { view.bounds.insetBy(dx: horizontalInset, dy: 0) }
    private var scrollFrame: CGRect {
        let bounds = readerFrame
        return CGRect(x: bounds.minX, y: contentTopInset, width: bounds.width,
                      height: max(60, bounds.height - contentTopInset - verticalInset))
    }
    var toggleTools: (() -> Void)?
    var turning: (() -> Void)?
    var visible: ((ReadingAnchor, String) -> Void)?
    var committed: ((ReadingAnchor) -> Void)?
    private var speechHighlight: SpeechHighlight?
    private var transitioning = false
    init(book: PreviewBook, anchor: ReadingAnchor, options: ReaderOptions) {
        self.book = book; self.anchor = anchor; self.options = options
        super.init(nibName: nil, bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        view.accessibilityIdentifier="book-reading-content"
        view.alpha=contentVisible ? 1 : 0
        view.backgroundColor = options.paper
        backdrop.contentMode = .scaleAspectFill; backdrop.clipsToBounds = true
        backdrop.isUserInteractionEnabled = false; view.insertSubview(backdrop, at: 0)
        updatePaperAppearance()
        NotificationCenter.default.addObserver(self, selector: #selector(flushScrollPosition), name: UIApplication.willResignActiveNotification, object: nil)
    }
    deinit { initialVisibilityPending?.cancel(); pendingScrollSave?.cancel(); pendingWarmup?.cancel(); NotificationCenter.default.removeObserver(self) }
    override func viewWillDisappear(_ animated: Bool) { super.viewWillDisappear(animated); flushScrollPosition() }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        if let window=view.window {
            let viewport=ReaderWindowViewport(bounds:window.bounds,safe:window.safeAreaInsets)
            if viewport != reportedViewport { reportedViewport=viewport; DispatchQueue.main.async { [weak self] in self?.viewportChanged?(viewport) } }
        }
        backdrop.frame = readerFrame
        rebuildIfNeeded()
        pager?.view.frame = readerFrame
        continuous?.frame = scrollFrame
    }
    func update(options: ReaderOptions, jump: ReadingAnchor?, jumpID: Int, annotations: ReaderAnnotations, revision: Int) {
        if revision != annotationRevision { self.annotations = annotations; annotationRevision = revision; layoutOptions = nil; scrollCache.clear(); pagingCache.clear() }
        if self.options.readingMode == .scroll && options.readingMode != .scroll { flushScrollPosition(); pendingWarmup?.cancel() }
        let languageChanged=self.options.language != options.language
        self.options = options
        if isViewLoaded { updatePaperAppearance() }
        if jumpID != lastJumpID, let jump {
            pendingScrollSave?.cancel(); pendingScrollSave = nil
            lastJumpID = jumpID
            scrollReset = true
            anchor = jump
            layoutOptions = nil
        }
        if isViewLoaded { rebuildIfNeeded(); if languageChanged {reportVisible()} }
    }
    func updateSpeech(_ highlight: SpeechHighlight?, following: Bool) {
        speechHighlight = highlight
        if following, !transitioning, let highlight, !pages.isEmpty, let table = continuous {
            let previous = anchor
            if scrollCenter != highlight.start.chapter { changeScrollWindow(around: highlight.start.chapter, target: highlight.start, pixelY: 24, table: table) }
            let target = ReadingLayout.pageIndex(for: highlight.start, in: pages)
            if target != currentIndex || previous != highlight.start {
                pendingScrollSave?.cancel(); pendingScrollSave = nil
                restoreScroll(highlight.start, in: table); flushScrollPosition()
            }
        }
        if following, !transitioning, let highlight, !pages.isEmpty, let pager {
            var target = ReadingLayout.pageIndex(for: highlight.start, in: pages)
            if dual { target = target / 2 * 2 }
            if target != currentIndex {
                let direction: UIPageViewController.NavigationDirection = target > currentIndex ? .forward : .reverse
                anchor = highlight.start; currentIndex = target
                pager.setViewControllers(controllers(at: target), direction: direction, animated: false)
                committed?(highlight.start); reportVisible()
            }
        }
        pager?.viewControllers?.forEach { controller in
            if let leaf = controller as? PaperController { leaf.speechHighlight = highlight }
            (controller as? SpreadController)?.leaves.forEach { $0.speechHighlight = highlight }
        }
        continuous?.papers.forEach { $0.speechHighlight = highlight }
    }
    func updateImmersiveStatus(_ info: ReadingStatusInfo?) {
        guard immersiveStatus != info else { return }
        immersiveStatus = info
        for leaf in paperLeaves.allObjects { leaf.statusInfo = info }
    }
    private func updatePaperAppearance() {
        let image = options.backgroundImage
        if backdrop.image !== image { backdrop.image = image }
        backdrop.alpha = min(1, max(0, options.backgroundImageOpacity)); backdrop.isHidden = image == nil
        view.backgroundColor = options.paper
        continuous?.backgroundColor = image == nil ? options.paper : .clear
        continuous?.papers.forEach { $0.backgroundColor = image == nil ? options.paper : .clear; $0.isOpaque = image == nil }
        for leaf in paperLeaves.allObjects {
            leaf.backgroundImage = image; leaf.backgroundOpacity = options.backgroundImageOpacity
            leaf.footerInk = options.ink
            leaf.footerText = pages.indices.contains(leaf.index) ? L("reader.page_info", leaf.index+1, pages.count) : ""
        }
    }
    private func chapterSource(_ chapter: Int) -> BookChapter {
        let source = book.chapters[chapter]
        guard let styles = nativeStyles[chapter] else { return source }
        return BookChapter(title: source.title, blocks: source.blocks.enumerated().map { index, block in
            var value = block
            let logical = block.core_block ?? index
            value.native_style = styles.indices.contains(logical) ? styles[logical] : nil
            return value
        })
    }
    private func layoutChapter(_ source: BookChapter, chapter: Int, in rect: CGRect) -> LaidOutChapter {
        let content = BookTypesetter.chapters([source], options: options, rect: rect)[0]
        let text = NSMutableAttributedString(attributedString: content.text)
        for item in annotations.highlights where item.chapter == chapter {
            let begin = content.utf16(for: item.start), end = content.utf16(for: item.end)
            if end > begin && end <= text.length { text.addAttribute(NSAttributedString.Key("ReaderHighlight"), value: item.tint.withAlphaComponent(0.25), range: NSRange(location: begin, length: end-begin)) }
        }
        for item in annotations.corrections ?? [] where item.chapter == chapter && item.status == "accepted" {
            let at = content.utf16(for: .init(chapter: chapter, block: item.block_idx, charOffset: item.char_offset))
            if at < text.length && item.original.utf16.count == item.corrected.utf16.count && at+item.original.utf16.count <= text.length {
                text.replaceCharacters(in: NSRange(location: at, length: item.original.utf16.count), with: item.corrected)
                text.addAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, range: NSRange(location: at, length: item.corrected.utf16.count))
            }
        }
        return LaidOutChapter(text: text, coordinates: content.coordinates)
    }
    private func pagedChapters(in rect: CGRect) -> [ReadingPage] {
        var signature = options.textLayoutOptions; signature.nativeRevision = 0
        if pagingCacheOptions != signature || pagingTextRect != rect { pagingCache.clear() }
        pagingCacheOptions = signature; pagingTextRect = rect
        return book.chapters.indices.flatMap { chapter in
            let source = chapterSource(chapter)
            return pagingCache.value(chapter:chapter,source:source) {
                ReadingLayout.paginate([layoutChapter(source,chapter:chapter,in:rect)],in:rect,chapterOffset:chapter)
            }
        }
    }
    private func rebuildIfNeeded() {
        guard view.window != nil else { return }
        let size = view.bounds.size
        guard size.width > 100 && size.height > 100 else { return }
        guard size != lastSize || layoutOptions != options.textLayoutOptions else { return }
        if transitioning && size == lastSize { return }
        if let viewport = continuous, (viewport.isDragging || viewport.isDecelerating),
           size == lastSize, layoutOptions?.nativeRevision != options.nativeRevision { return }
        let sizeChanged = size != lastSize
        lastSize = size; layoutOptions = options.textLayoutOptions; transitioning = false
        view.backgroundColor = options.paper
        // Freeze safe-area and header budgets for this size. Hiding system or app
        // chrome must never repaginate; only actual size/options changes do so.
        let screenSafe = view.window?.safeAreaInsets ?? view.safeAreaInsets
        if sizeChanged {
            verticalInset = max(68, max(screenSafe.top, screenSafe.bottom) + 24)
            // Reserve the floating header even when controls are hidden. Its
            // visibility must not repaginate or move a continuous scroll anchor.
            contentTopInset = max(verticalInset, screenSafe.top + 68)
            horizontalInset = max(screenSafe.left, screenSafe.right)
            footerSafeBottom = screenSafe.bottom
        }
        let contentWidth = size.width - horizontalInset * 2
        dual = options.readingMode != .scroll && ReadingSpread.columns(width: contentWidth, height: size.height, margin: options.pageMargin, fontSize: options.fontSize) == 2
        let leafWidth = dual ? contentWidth / 2 : contentWidth
        let textWidth = min(options.readingMode == .scroll ? options.fontSize * 34 : 620, leafWidth - options.pageMargin * 2)
        let textRect = CGRect(x: (leafWidth-textWidth)/2, y: verticalInset, width: textWidth, height: max(60, size.height-contentTopInset-verticalInset))
        if options.readingMode == .scroll {
            var signature = options.textLayoutOptions; signature.nativeRevision = 0
            let sameGeometry = scrollCacheOptions == signature && scrollTextRect == textRect
            let pixelY = sameGeometry && !scrollReset ? scrollAnchorPixelY() : 24
            if !sameGeometry { scrollCache.clear(); scrollChapterStarts.removeAll(); scrollBookStart = nil; scrollBookEnd = nil }
            scrollReset = false
            scrollCacheOptions = signature; scrollTextRect = textRect
            if let old = pager {
                old.dataSource = nil; old.delegate = nil; old.willMove(toParent: nil)
                old.view.removeFromSuperview(); old.removeFromParent(); pager = nil
            }
            let table: ReadingScrollViewport
            if let existing = continuous { table = existing; table.frame = scrollFrame }
            else {
                pages.removeAll(); scrollPages.removeAll(); scrollRowIndices.removeAll()
                table = ReadingScrollViewport(frame: scrollFrame)
                table.delegate = self
                table.contentInsetAdjustmentBehavior = .never
                table.showsVerticalScrollIndicator = false
                continuous = table; view.addSubview(table)
                let tap = UITapGestureRecognizer(target: self, action: #selector(centerTapped))
                tap.delegate = self; tap.cancelsTouchesInView = false; table.addGestureRecognizer(tap)
            }
            table.backgroundColor = options.backgroundImage == nil ? options.paper : .clear
            changeScrollWindow(around: anchor.chapter, target: anchor, pixelY: pixelY, table: table)
            return
        }
        scrollCache.clear(); scrollActive.removeAll(); scrollPages.removeAll(); scrollRowIndices.removeAll(); scrollCenter = -1; scrollChapterStarts.removeAll()
        continuous?.delegate = nil; continuous?.removeFromSuperview(); continuous = nil
        pages = pagedChapters(in: textRect)
        guard !pages.isEmpty else { return }
        currentIndex = ReadingLayout.pageIndex(for: anchor, in: pages)
        if dual { currentIndex = currentIndex / 2 * 2 }
        if let old = pager {
            old.dataSource = nil; old.delegate = nil
            old.willMove(toParent: nil); old.view.removeFromSuperview(); old.removeFromParent()
        }
        pager = nil
        let controller = UIPageViewController(transitionStyle: options.sliding ? .scroll : .pageCurl, navigationOrientation: .horizontal, options: options.sliding ? nil : [.spineLocation: NSNumber(value: dual ? UIPageViewController.SpineLocation.mid.rawValue : UIPageViewController.SpineLocation.min.rawValue)])
        controller.isDoubleSided = dual && !options.sliding
        controller.dataSource = options.readingMode.customTransition ? nil : self; controller.delegate = self
        pager = controller
        addChild(controller); controller.view.frame = readerFrame
        controller.view.backgroundColor = options.paper
        view.addSubview(controller.view); controller.didMove(toParent: self)
        controller.setViewControllers(controllers(at: currentIndex), direction: .forward, animated: false)
        let tap = UITapGestureRecognizer(target: self, action: #selector(centerTapped))
        tap.delegate = self; tap.cancelsTouchesInView = false
        controller.view.addGestureRecognizer(tap)
        if options.readingMode.customTransition {
            for direction in [UISwipeGestureRecognizer.Direction.left, .right] {
                let swipe = UISwipeGestureRecognizer(target: self, action: #selector(customSwiped))
                swipe.direction = direction; swipe.delegate = self
                controller.view.addGestureRecognizer(swipe)
            }
        }
        reportVisible()
    }
    private func leaf(_ index: Int) -> PaperController {
        let page = pages.indices.contains(index) ? pages[index] : nil
        let leaf = PaperController(index: index, page: page, paper: options.paper, selected: { [weak self] in self?.selected?($0) })
        leaf.speechHighlight = speechHighlight
        leaf.footerText = page == nil ? "" : L("reader.page_info", index+1, pages.count)
        leaf.footerInk = options.ink; leaf.footerSafeBottom = footerSafeBottom
        leaf.statusInfo = immersiveStatus
        leaf.backgroundImage = options.backgroundImage; leaf.backgroundOpacity = options.backgroundImageOpacity
        paperLeaves.add(leaf)
        return leaf
    }
    private func controllers(at index: Int) -> [UIViewController] {
        if dual && options.sliding { return [SpreadController(index: index, leaves: [leaf(index), leaf(index + 1)])] }
        return dual ? [leaf(index), leaf(index + 1)] : [leaf(index)]
    }
    private var leafCount: Int { dual ? pages.count + pages.count % 2 : pages.count }
    func pageViewController(_ pageViewController: UIPageViewController, viewControllerBefore viewController: UIViewController) -> UIViewController? {
        if let spread = viewController as? SpreadController { return spread.index > 0 ? controllers(at: spread.index - 2).first : nil }
        guard let leaf = viewController as? PaperController, leaf.index > 0 else { return nil }
        return self.leaf(leaf.index-1)
    }
    func pageViewController(_ pageViewController: UIPageViewController, viewControllerAfter viewController: UIViewController) -> UIViewController? {
        if let spread = viewController as? SpreadController { return spread.index + 2 < leafCount ? controllers(at: spread.index + 2).first : nil }
        guard let leaf = viewController as? PaperController, leaf.index+1 < leafCount else { return nil }
        return self.leaf(leaf.index+1)
    }
    func pageViewController(_ pageViewController: UIPageViewController, willTransitionTo pendingViewControllers: [UIViewController]) { transitioning = true; turning?() }
    func pageViewController(_ pageViewController: UIPageViewController, didFinishAnimating finished: Bool, previousViewControllers: [UIViewController], transitionCompleted completed: Bool) {
        guard pageViewController === pager else { return }
        transitioning = false
        let indices = pageViewController.viewControllers?.compactMap { ($0 as? PaperController)?.index ?? ($0 as? SpreadController)?.index } ?? []
        guard let next = ReadingLayout.completedAnchor(completed: completed, visible: indices, pages: pages) else { return }
        anchor = next; currentIndex = indices.min() ?? currentIndex
        committed?(next); reportVisible(); rebuildIfNeeded()
    }
    private func reportVisible() {
        guard pages.indices.contains(currentIndex) else { return }
        let visibleEnd=min(pages.count,currentIndex+(dual ? 2 : 1))
        let firstPages=Array(pages[currentIndex..<visibleEnd])
        let imagesReady=firstPages.allSatisfy {BookTypesetter.prefetch($0)}
        if !firstContentReported && !imagesReady {
            initialVisibilityPending?.cancel()
            let retry=DispatchWorkItem { [weak self] in self?.reportVisible() }
            initialVisibilityPending=retry
            DispatchQueue.main.asyncAfter(deadline:.now()+0.016,execute:retry)
            return
        }
        firstContentReported=true
        for index in max(0,currentIndex-1)..<min(pages.count,currentIndex+4) { _ = BookTypesetter.prefetch(pages[index]) }
        let current = anchor
        let end = min(pages.count, currentIndex+(dual ? 2 : 1))
        let label = options.readingMode == .scroll ? L("reader.chapter_indicator", current.chapter+1, book.chapters.count) : dual ? "\(currentIndex+1)–\(end) / \(pages.count)" : "\(currentIndex+1) / \(pages.count)"
        let callback = visible
        DispatchQueue.main.async { callback?(current, label) }
    }
    private func scrollPage(_ index: Int) -> ReadingPage { scrollPages[index] }
    private func scrollLayout(_ chapter: Int) -> ScrollChapterLayout {
        let source = chapterSource(chapter)
        return scrollCache.value(chapter: chapter, source: source) {
            let content = layoutChapter(source, chapter: chapter, in: scrollTextRect)
            let chapterPages = ReadingLayout.paginate([content], in: scrollTextRect, chapterOffset: chapter)
            return ScrollChapterLayout(chapterPages.enumerated().map { index, page in
                ReadingLayout.continuousPage(page, topInset: index == 0 ? 24 : 0, bottomInset: index + 1 == chapterPages.count ? 24 : 0)
            })
        }
    }
    private func scrollMap(_ row: Int) -> ReadingLineMap {
        scrollActive[scrollPages[row].chapter]!.map(scrollRowIndices[row])
    }
    private func scrollAnchorPixelY() -> CGFloat {
        guard let table = continuous, pages.indices.contains(currentIndex) else { return 24 }
        return table.rectForRow(at: IndexPath(row: currentIndex, section: 0)).minY + scrollMap(currentIndex).verticalOffset(for: anchor) - table.contentOffset.y
    }
    private func changeScrollWindow(around chapter: Int, target: ReadingAnchor, pixelY: CGFloat, table: ReadingScrollViewport) {
        pendingWarmup?.cancel()
        let count = book.chapters.count
        guard count > 0 else { return }
        scrollCenter = min(count - 1, max(0, chapter))
        let center = scrollLayout(scrollCenter)
        if scrollChapterStarts[scrollCenter] == nil { scrollBookStart = nil; scrollBookEnd = nil }
        let centerStart = scrollChapterStarts[scrollCenter] ?? 1_000_000
        let localRow = ReadingLayout.pageIndex(for: target, in: center.pages)
        let preceding = center.pages.prefix(localRow).reduce(CGFloat(0)) { $0 + ($1.surfaceHeight ?? 0) }
        let localOffset = center.pages.isEmpty ? 0 : center.map(localRow).verticalOffset(for: target)
        let targetY = centerStart + preceding + localOffset
        // A chapter count cannot bound the visible window: several short chapters
        // may fit on one screen. Fill both directions by viewport height instead.
        var first = scrollCenter, last = scrollCenter
        var start = centerStart, end = centerStart + center.height
        var layouts: [Int: ScrollChapterLayout] = [scrollCenter: center]
        while first > 0 && (first == scrollCenter || targetY - start < table.bounds.height * 1.5) {
            first -= 1; let layout = scrollLayout(first); layouts[first] = layout; start -= layout.height
        }
        while last + 1 < count && (last == scrollCenter || end - targetY < table.bounds.height * 2) {
            last += 1; let layout = scrollLayout(last); layouts[last] = layout; end += layout.height
        }
        scrollLoadedRange = first..<(last + 1)
        scrollCache.retain(max(0, first - 1)..<min(count, last + 2))
        scrollActive = layouts; scrollPages.removeAll(); scrollRowIndices.removeAll()
        var frames: [CGRect] = []; var starts: [Int: CGFloat] = [:]; var y = start
        for index in scrollLoadedRange {
            let layout = layouts[index]!; starts[index] = y
            for (row, page) in layout.pages.enumerated() {
                frames.append(CGRect(x: 0, y: y, width: table.bounds.width, height: page.surfaceHeight!))
                scrollPages.append(page); scrollRowIndices.append(row); y += page.surfaceHeight!
            }
        }
        scrollChapterStarts = starts; pages = scrollPages
        guard !pages.isEmpty else { return }
        restoringScroll = true
        // The outer scroll coordinates and overlapping paper views stay stable.
        // No reloadData or offset rebasing occurs as a gesture crosses a chapter.
        if first == 0 { scrollBookStart = start }
        if last + 1 == count { scrollBookEnd = end }
        // Keep discovered book limits even after those chapters leave the cache;
        // changing the inset mid-pan would translate the finger's scroll origin.
        table.contentInset.top = -(scrollBookStart ?? 0)
        table.contentSize = CGSize(width: table.bounds.width, height: scrollBookEnd ?? (end + table.bounds.height * 8))
        table.configure(pages: scrollPages, frames: frames) { [weak self] row, surface in
            guard let self else { return }
            surface.page = self.scrollPage(row); surface.cachedFrame = self.scrollMap(row).frame
            surface.backgroundColor = self.options.backgroundImage == nil ? self.options.paper : .clear
            surface.isOpaque = self.options.backgroundImage == nil
            surface.selected = self.selected; surface.speechHighlight = self.speechHighlight
        }
        restoreScroll(target, in: table, pixelY: pixelY)
        scheduleWarmup()
    }
    private func scheduleWarmup() {
        pendingWarmup?.cancel()
        let center = scrollCenter
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.options.readingMode == .scroll, self.scrollCenter == center,
                  let table = self.continuous, !table.isDragging, !table.isDecelerating else { return }
            for index in max(0, self.scrollLoadedRange.lowerBound - 1)..<min(self.book.chapters.count, self.scrollLoadedRange.upperBound + 1) {
                _ = self.scrollLayout(index)
            }
        }
        pendingWarmup = work; DispatchQueue.main.asyncAfter(deadline: .now() + 0.2, execute: work)
    }
    private func restoreScroll(_ target: ReadingAnchor, in table: ReadingScrollViewport, pixelY: CGFloat = 24) {
        restoringScroll = true
        currentIndex = ReadingLayout.pageIndex(for: target, in: pages); anchor = target
        let row = table.rectForRow(at: IndexPath(row: currentIndex, section: 0))
        let y = row.minY + scrollMap(currentIndex).verticalOffset(for: target) - pixelY
        let minY = -table.adjustedContentInset.top
        let maxY = max(minY, table.contentSize.height - table.bounds.height + table.adjustedContentInset.bottom)
        // Adjust the origin without starting a new scroll animation during a window shift.
        let nextY = min(maxY, max(minY, y))
        if abs(table.bounds.origin.y - nextY) > 0.5 { table.bounds.origin.y = nextY }
        restoredScrollOffset = table.bounds.origin.y
        restoringScroll = false; reportVisible()
    }
    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        pendingWarmup?.cancel(); continuous?.startDisplayUpdates(); turning?()
    }
    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        continuous?.renderVisibleRows()
        if let restoredScrollOffset, abs(scrollView.contentOffset.y - restoredScrollOffset) < 0.5 { return }
        restoredScrollOffset = nil
        guard !restoringScroll, let table = continuous, scrollView === table,
              let path = table.indexPathForRow(at: CGPoint(x: 1, y: max(0, table.contentOffset.y + 24))) else { return }
        let row = table.rectForRow(at: path)
        let next = scrollMap(path.row).anchor(at: max(0, table.contentOffset.y + 24 - row.minY))
        let refill = scrollWindowNeedsRefill(table)
        guard next != anchor || refill else { return }
        if next != anchor {
            currentIndex = path.row; anchor = next
            queueScrollSave(); reportVisible()
        }
        if next.chapter != scrollCenter || refill {
            let pixelY = scrollAnchorPixelY()
            changeScrollWindow(around: next.chapter, target: next, pixelY: pixelY, table: table)
        }
    }
    private func scrollWindowNeedsRefill(_ viewport: ReadingScrollViewport) -> Bool {
        guard !pages.isEmpty else { return false }
        let first = viewport.rectForRow(at: IndexPath(row: 0, section: 0)).minY
        let last = viewport.rectForRow(at: IndexPath(row: pages.count - 1, section: 0)).maxY
        let margin = viewport.bounds.height * 0.75
        // The bottom can pass a short bridge chapter while the top is still in
        // the preceding long chapter. Extend by visible geometry, not just by
        // a change in the chapter at the progress/anchor line.
        return (scrollLoadedRange.upperBound < book.chapters.count && viewport.bounds.maxY + margin > last)
            || (scrollLoadedRange.lowerBound > 0 && viewport.bounds.minY - margin < first)
    }
    private func queueScrollSave() {
        pendingScrollSave?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.flushScrollPosition() }
        pendingScrollSave = work; DispatchQueue.main.asyncAfter(deadline: .now() + 0.35, execute: work)
    }
    @objc private func flushScrollPosition() {
        pendingScrollSave?.cancel(); pendingScrollSave = nil
        guard options.readingMode == .scroll, anchor != lastSavedScrollAnchor else { return }
        lastSavedScrollAnchor = anchor; committed?(anchor)
    }
    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate { continuous?.stopDisplayUpdates(); flushScrollPosition(); rebuildIfNeeded(); scheduleWarmup() }
    }
    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) { continuous?.stopDisplayUpdates(); flushScrollPosition(); rebuildIfNeeded(); scheduleWarmup() }
    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) { flushScrollPosition(); rebuildIfNeeded(); scheduleWarmup() }
    @objc private func centerTapped(_ gesture: UITapGestureRecognizer) {
        func activate(_ root:UIView)->Bool {
            if let paper=root as? PaperSurface,let (chapter,href)=paper.note(at:gesture.location(in:paper)){publicationNote?(chapter,href);return true}
            if let paper=root as? PaperSurface, let (chapter,node)=paper.action(at:gesture.location(in:paper)) { publicationEvent?(chapter,node);return true }
            for child in root.subviews where activate(child) {return true}
            return false
        }
        if activate(view) { return }
        let x = gesture.location(in: view).x / view.bounds.width
        if pager != nil && options.sliding && (x < 0.32 || x > 0.68) { turnSpread(forward: x > 0.68) }
        else { toggleTools?() }
    }
    @objc private func customSwiped(_ gesture: UISwipeGestureRecognizer) {
        guard gesture.state == .ended else {return}
        turnSpread(forward: gesture.direction == .left)
    }
    func turnSpread(forward: Bool) {
        guard let pager, !transitioning else { return }
        let next = currentIndex + (forward ? 1 : -1) * (dual ? 2 : 1)
        guard pages.indices.contains(next) else { return }
        transitioning = true; turning?()
        let finish: (Bool) -> Void = { [weak self, weak pager] completed in
            guard let self, pager === self.pager else { return }
            self.transitioning = false
            guard completed else { return }
            self.currentIndex = next; self.anchor = self.pages[next].anchor
            self.committed?(self.anchor); self.reportVisible(); self.rebuildIfNeeded()
        }
        if options.readingMode == .cover && !UIAccessibility.isReduceMotionEnabled,
           let previous = pager.view.snapshotView(afterScreenUpdates: false) {
            previous.frame = pager.view.frame
            view.addSubview(previous)
            pager.setViewControllers(controllers(at: next), direction: forward ? .forward : .reverse, animated: false)
            pager.view.layoutIfNeeded()
            if forward { view.insertSubview(previous, belowSubview: pager.view); pager.view.transform = CGAffineTransform(translationX: pager.view.bounds.width, y: 0) }
            UIView.animate(withDuration: 0.22, delay: 0, options: [.curveEaseOut]) {
                if forward { pager.view.transform = .identity }
                else { previous.transform = CGAffineTransform(translationX: previous.bounds.width, y: 0) }
            } completion: { _ in
                previous.removeFromSuperview(); pager.view.transform = .identity; finish(true)
            }
        } else {
            pager.setViewControllers(controllers(at: next), direction: forward ? .forward : .reverse,
                                     animated: options.readingMode != .none && !UIAccessibility.isReduceMotionEnabled, completion: finish)
        }
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        var target=touch.view
        while let value=target {
            if let paper=value as? PaperSurface, (paper.action(at:touch.location(in:paper)) != nil || paper.note(at:touch.location(in:paper)) != nil) {return true}
            target=value.superview
        }
        let x = touch.location(in: view).x / view.bounds.width
        return (pager != nil && options.sliding) || (x >= 0.32 && x <= 0.68)
    }
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool { true }
}

final class SpreadController: UIViewController {
    let index: Int
    let leaves: [PaperController]
    init(index: Int, leaves: [PaperController]) { self.index = index; self.leaves = leaves; super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        for leaf in leaves { addChild(leaf); view.addSubview(leaf.view); leaf.didMove(toParent: self) }
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        for (offset, leaf) in leaves.enumerated() { leaf.view.frame = CGRect(x: CGFloat(offset) * view.bounds.width / 2, y: 0, width: view.bounds.width / 2, height: view.bounds.height) }
    }
}

final class PaperController: UIViewController {
    private var surface: PaperSurface?
    // This label belongs to the leaf, so UIKit curls/slides and cover snapshots
    // carry the outgoing and incoming page numbers with their own paper.
    let pageFooter = UILabel()
    let immersiveFooter = PaperImmersiveStatus()
    var statusInfo: ReadingStatusInfo? { didSet { if oldValue != statusInfo { immersiveFooter.update(statusInfo, ink: footerInk) } } }
    var footerText = "" { didSet { guard oldValue != footerText else { return }; pageFooter.text = footerText; pageFooter.isHidden = footerText.isEmpty; surface?.accessibilityValue = footerText } }
    var footerInk: UIColor = .secondaryLabel { didSet { pageFooter.textColor = footerInk.withAlphaComponent(0.38); immersiveFooter.update(statusInfo, ink: footerInk) } }
    var footerSafeBottom: CGFloat = 0
    let index: Int
    let page: ReadingPage?
    let paper: UIColor
    var backgroundImage: UIImage? { didSet { surface?.backgroundImage = backgroundImage } }
    var backgroundOpacity = 0.22 { didSet { surface?.backgroundImageOpacity = backgroundOpacity } }
    let selected: (SelectedPassage) -> Void
    var speechHighlight: SpeechHighlight? {
        didSet { surface?.speechHighlight = speechHighlight }
    }
    init(index: Int, page: ReadingPage?, paper: UIColor, selected: @escaping (SelectedPassage) -> Void) {
        self.index = index; self.page = page; self.paper = paper; self.selected = selected
        super.init(nibName: nil, bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func loadView() {
        let surface = PaperSurface()
        self.surface = surface
        surface.page = page; surface.backgroundColor = paper; surface.selected = selected
        surface.backgroundImage = backgroundImage; surface.backgroundImageOpacity = backgroundOpacity
        surface.speechHighlight = speechHighlight
        surface.isAccessibilityElement = page != nil
        surface.accessibilityIdentifier = "reading-page-\(index)"
        if let page { surface.accessibilityLabel = BookTypesetter.accessibilityText(page.text.attributedSubstring(from:NSRange(location:page.range.location,length:page.range.length))) }
        if page?.surfaceHeight != nil {
            let scroll = UIScrollView()
            scroll.contentInsetAdjustmentBehavior = .never; scroll.backgroundColor = paper
            scroll.addSubview(surface); view = scroll
        } else { view = surface }
        pageFooter.text = footerText; pageFooter.isHidden = footerText.isEmpty
        pageFooter.font = .monospacedDigitSystemFont(ofSize: 12, weight: .regular)
        pageFooter.textColor = footerInk.withAlphaComponent(0.38)
        pageFooter.textAlignment = .right
        pageFooter.isUserInteractionEnabled = false; pageFooter.isAccessibilityElement = false
        pageFooter.accessibilityIdentifier = "reading-page-footer"
        surface.accessibilityValue = footerText
        pageFooter.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(pageFooter)
        immersiveFooter.translatesAutoresizingMaskIntoConstraints = false
        immersiveFooter.update(statusInfo, ink: footerInk); view.addSubview(immersiveFooter)
        // The overflow fallback may scroll its body; its footer stays on the
        // page viewport while still participating in the outer page turn.
        let guide: UILayoutGuide = (view as? UIScrollView)?.frameLayoutGuide ?? view.layoutMarginsGuide
        let trailing = view is UIScrollView ? guide.trailingAnchor : view.trailingAnchor
        let bottom = view is UIScrollView ? guide.bottomAnchor : view.bottomAnchor
        NSLayoutConstraint.activate([
            pageFooter.trailingAnchor.constraint(equalTo: trailing, constant: -16),
            pageFooter.bottomAnchor.constraint(equalTo: bottom, constant: -max(0, footerSafeBottom)-8),
            pageFooter.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 16),
            immersiveFooter.leadingAnchor.constraint(equalTo: view is UIScrollView ? guide.leadingAnchor : view.leadingAnchor, constant: 16),
            immersiveFooter.bottomAnchor.constraint(equalTo: pageFooter.bottomAnchor)
        ])
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        if let scroll = view as? UIScrollView, let surface, let page {
            let height = max(view.bounds.height, page.textRect.maxY + 32)
            surface.frame = CGRect(x:0,y:0,width:view.bounds.width,height:height)
            scroll.contentSize = surface.bounds.size
        }
    }
}
final class PaperSurface: UIView {
    var page: ReadingPage? { didSet { cachedFrame = nil; setNeedsDisplay() } }
    var cachedFrame: CTFrame?
    var backgroundImage: UIImage? { didSet { if oldValue !== backgroundImage { setNeedsDisplay() } } }
    var backgroundImageOpacity = 0.22 { didSet { if oldValue != backgroundImageOpacity { setNeedsDisplay() } } }
    var selected: ((SelectedPassage) -> Void)?
    var speechHighlight: SpeechHighlight? { didSet { if oldValue != speechHighlight { setNeedsDisplay() } } }
    private var selection: NSRange?
    private var firstIndex = 0
    override init(frame: CGRect) {
        super.init(frame: frame)
        NotificationCenter.default.addObserver(self, selector: #selector(illustrationLoaded), name: BookTypesetter.illustrationReady, object: nil)
        let press = UILongPressGestureRecognizer(target: self, action: #selector(selectText(_:))); press.minimumPressDuration = 0.4; addGestureRecognizer(press)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    deinit { NotificationCenter.default.removeObserver(self) }
    @objc private func illustrationLoaded() { setNeedsDisplay() }
    private func frame(_ page: ReadingPage) -> CTFrame {
        if let cachedFrame { return cachedFrame }
        let value = CTFramesetterCreateFrame(page.framesetter ?? CTFramesetterCreateWithAttributedString(page.text), page.range, CGPath(rect: page.textRect, transform: nil), nil)
        cachedFrame = value; return value
    }
    private func index(at point: CGPoint, page: ReadingPage) -> Int? {
        guard page.textRect.insetBy(dx: -8, dy: -8).contains(point) else { return nil }
        let frame = frame(page), lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = Array(repeating: CGPoint.zero, count: lines.count); CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
        let local = CGPoint(x: point.x-page.textRect.minX, y: bounds.height-point.y-page.textRect.minY)
        guard let nearest = lines.indices.min(by: { abs(origins[$0].y-local.y) < abs(origins[$1].y-local.y) }) else { return nil }
        let line = lines[nearest]
        let result = CTLineGetStringIndexForPosition(line, CGPoint(x: local.x-origins[nearest].x, y: 0))
        return result == kCFNotFound ? nil : min(page.range.location+page.range.length-1, max(page.range.location,result))
    }
    func note(at point:CGPoint)->(Int,String)? {
        guard let page,let index=index(at:point,page:page),index>=0,index<page.text.length,let href=page.text.attribute(BookTypesetter.noteKey,at:index,effectiveRange:nil) as? String else{return nil}
        return(page.chapter,href)
    }
    func action(at point:CGPoint)->(Int,Int)? {
        guard let page, let index=index(at:point,page:page), index>=0,index<page.text.length,
              let node=page.text.attribute(BookTypesetter.actionKey,at:index,effectiveRange:nil) as? Int else {return nil}
        return (page.chapter,node)
    }
    @objc private func selectText(_ gesture: UILongPressGestureRecognizer) {
        guard let page else { return }
        let text = page.text.string as NSString
        switch gesture.state {
        case .began:
            guard let at = index(at: gesture.location(in: self), page: page) else { return }
            if page.text.attribute(BookTypesetter.imageKey,at:at,effectiveRange:nil) != nil{return}
            firstIndex = at
            let range = text.rangeOfComposedCharacterSequence(at: at)
            selection = range
            // Select a word initially; dragging after holding extends to any range on this page.
            page.text.string.enumerateSubstrings(in: page.text.string.startIndex..<page.text.string.endIndex, options: .byWords) { _, range, _, stop in
                let word = NSRange(range, in: page.text.string)
                if NSLocationInRange(at,word) && word.location >= page.range.location && NSMaxRange(word) <= page.range.location+page.range.length { self.selection = word; stop = true }
            }
        case .changed:
            if let at = index(at: gesture.location(in: self), page: page) { selection = text.rangeOfComposedCharacterSequences(for: NSRange(location: min(firstIndex,at), length: abs(at-firstIndex)+1)) }
        case .ended:
            if let range = selection, range.length > 0 {
                let end = NSMaxRange(range)
                let startAnchor = page.content.anchor(chapter: page.chapter, at: range.location)
                let endAnchor = page.content.anchor(chapter: page.chapter, at: end)
                if startAnchor.block < endAnchor.block || startAnchor.block == endAnchor.block && startAnchor.charOffset < endAnchor.charOffset {
                    selected?(.init(start: startAnchor, end: endAnchor, text: text.substring(with: range)))
                }
            }
            selection = nil
        default: selection = nil
        }
        setNeedsDisplay()
    }
    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }
        if let image = backgroundImage, page?.surfaceHeight == nil {
            let scale = max(bounds.width / image.size.width, bounds.height / image.size.height)
            let size = CGSize(width: image.size.width * scale, height: image.size.height * scale)
            image.draw(in: CGRect(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2, width: size.width, height: size.height), blendMode: .normal, alpha: min(1, max(0, backgroundImageOpacity)))
        }
        guard let page else { return }
        context.textMatrix = .identity
        context.translateBy(x: 0, y: bounds.height); context.scaleBy(x: 1, y: -1)
        let frame = frame(page)
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = Array(repeating: CGPoint.zero, count: lines.count); CTFrameGetLineOrigins(frame, CFRange(location: 0,length: 0), &origins)
        let origin = CTFrameGetPath(frame).boundingBox.origin
        for (i,line) in lines.enumerated() {
            let lineRange = CTLineGetStringRange(line)
            let range = NSRange(location: lineRange.location, length: lineRange.length)
            page.text.enumerateAttribute(NSAttributedString.Key("ReaderHighlight"), in: range) { value, range, _ in
                if let color = value as? UIColor { self.paint(range, color: color, line: line, origin: CGPoint(x: origin.x+origins[i].x,y: origin.y+origins[i].y), context: context) }
            }
            if let highlight = speechHighlight, highlight.start.chapter == page.chapter {
                let begin = page.content.utf16(for: highlight.start), end = page.content.utf16(for: highlight.end)
                if end > begin {
                    let overlap = NSIntersectionRange(range, NSRange(location: begin, length: end - begin))
                    if overlap.length > 0 { paint(overlap, color: UIColor.systemBlue.withAlphaComponent(0.18), line: line, origin: CGPoint(x: origin.x + origins[i].x, y: origin.y + origins[i].y), context: context) }
                }
            }
            if let selection { let overlap = NSIntersectionRange(range, selection); if overlap.length > 0 { paint(overlap, color: UIColor.systemBlue.withAlphaComponent(0.25), line: line, origin: CGPoint(x:origin.x+origins[i].x,y:origin.y+origins[i].y), context: context) } }
        }
        CTFrameDraw(frame, context)
        BookTypesetter.drawImages(frame: frame, context: context)
    }
}

extension PaperSurface {
    fileprivate func paint(_ range: NSRange, color: UIColor, line: CTLine, origin: CGPoint, context: CGContext) {
        let x = CTLineGetOffsetForStringIndex(line, range.location, nil), end = CTLineGetOffsetForStringIndex(line, NSMaxRange(range), nil)
        var ascent: CGFloat = 0, descent: CGFloat = 0; _ = CTLineGetTypographicBounds(line, &ascent, &descent, nil)
        context.setFillColor(color.cgColor); context.fill(CGRect(x:origin.x+x,y:origin.y-descent,width:max(0,end-x),height:ascent+descent))
    }
}

private final class ScrollChapterLayout {
    let pages: [ReadingPage]
    var height: CGFloat { pages.reduce(0) { $0 + ($1.surfaceHeight ?? 0) } }
    private var maps: [Int: ReadingLineMap] = [:]
    init(_ pages: [ReadingPage]) { self.pages = pages }
    func map(_ row: Int) -> ReadingLineMap {
        if let value = maps[row] { return value }
        let value = ReadingLineMap(pages[row]); maps[row] = value; return value
    }
}

// Virtualizes paper rows without resetting UIScrollView's pan/deceleration state.
final class ReadingScrollViewport: UIScrollView {
    private var pages: [ReadingPage] = []
    private var frames: [CGRect] = []
    private var surfaces: [String: PaperSurface] = [:]
    private var prepare: ((Int, PaperSurface) -> Void)?
    private var displayLink: CADisplayLink?
    private lazy var displayTarget = ReadingScrollDisplayTarget(self)
    private(set) var trackingDisplayTicks = 0
    var papers: [PaperSurface] { Array(surfaces.values) }
    var updatingDuringDrag: Bool { displayLink != nil }
    func startDisplayUpdates() {
        guard displayLink == nil else { return }
        let link = CADisplayLink(target: displayTarget, selector: #selector(ReadingScrollDisplayTarget.tick))
        // UIKit tracks a held finger in UITrackingRunLoopMode. Drawing new rows
        // must continue there, including when a layout pass is deferred.
        link.add(to: .main, forMode: .common); displayLink = link
    }
    func stopDisplayUpdates() { displayLink?.invalidate(); displayLink = nil }
    func renderVisibleRows() { render(refresh: false) }
    fileprivate func displayTick() { trackingDisplayTicks += 1; renderVisibleRows() }
    override func didMoveToWindow() { super.didMoveToWindow(); if window == nil { stopDisplayUpdates() } }
    deinit { displayLink?.invalidate() }
    override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        let delta: CGFloat
        switch direction {
        case .down, .next: delta = bounds.height * 0.85
        case .up, .previous: delta = -bounds.height * 0.85
        default: return false
        }
        let minimum = -adjustedContentInset.top
        let maximum = max(minimum, contentSize.height - bounds.height + adjustedContentInset.bottom)
        let next = min(maximum, max(minimum, contentOffset.y + delta))
        guard abs(next - contentOffset.y) > 0.5 else { return false }
        setContentOffset(CGPoint(x: 0, y: next), animated: true)
        UIAccessibility.post(notification: .pageScrolled, argument: nil)
        return true
    }
    func rectForRow(at index: IndexPath) -> CGRect { frames[index.row] }
    func indexPathForRow(at point: CGPoint) -> IndexPath? {
        var low = 0, high = frames.count
        while low < high {
            let middle = (low + high) / 2
            if frames[middle].maxY <= point.y { low = middle + 1 } else { high = middle }
        }
        return low < frames.count && frames[low].minY <= point.y ? IndexPath(row: low, section: 0) : nil
    }
    func configure(pages: [ReadingPage], frames: [CGRect], prepare: @escaping (Int, PaperSurface) -> Void) {
        self.pages = pages; self.frames = frames; self.prepare = prepare
        render(refresh: true)
    }
    override func layoutSubviews() { super.layoutSubviews(); render(refresh: false) }
    private func render(refresh: Bool) {
        let visible = bounds.insetBy(dx: 0, dy: -bounds.height / 2)
        var retained = Set<String>()
        var first = 0, last = frames.count
        while first < last {
            let middle = (first + last) / 2
            if frames[middle].maxY < visible.minY { first = middle + 1 } else { last = middle }
        }
        for row in first..<frames.count {
            if frames[row].minY > visible.maxY { break }
            let page = pages[row], key = "\(page.chapter)/\(page.range.location)"
            retained.insert(key)
            let existing = surfaces[key]
            let surface = existing ?? PaperSurface()
            surface.frame = frames[row]
            if existing == nil || refresh {
                prepare?(row, surface)
                surface.isAccessibilityElement = true; surface.accessibilityTraits = .staticText
                surface.accessibilityIdentifier = "reading-scroll-page-\(page.chapter)-\(page.range.location)"
                surface.accessibilityLabel = BookTypesetter.accessibilityText(page.text.attributedSubstring(from:NSRange(location:page.range.location,length:page.range.length)))
                surface.accessibilityCustomActions = [
                    UIAccessibilityCustomAction(name: L("reader.scroll_down")) { [weak self] _ in self?.accessibilityScroll(.down) ?? false },
                    UIAccessibilityCustomAction(name: L("reader.scroll_up")) { [weak self] _ in self?.accessibilityScroll(.up) ?? false }
                ]
            }
            if existing == nil {
                surfaces[key] = surface; addSubview(surface)
                // A new paper must acquire its backing contents while the pan is
                // still held, rather than waiting for a deferred UIView draw pass.
                if displayLink != nil { surface.layer.displayIfNeeded() }
            }
        }
        for key in Array(surfaces.keys) where !retained.contains(key) { surfaces.removeValue(forKey: key)?.removeFromSuperview() }
    }
}

private final class ReadingScrollDisplayTarget: NSObject {
    weak var viewport: ReadingScrollViewport?
    init(_ viewport: ReadingScrollViewport) { self.viewport = viewport }
    @objc func tick() { viewport?.displayTick() }
}
