import SwiftUI
import UIKit
import CoreText

struct NativeReader: UIViewControllerRepresentable {
    let book: PreviewBook
    let initial: ReadingAnchor
    let options: ReaderOptions
    let annotations: ReaderAnnotations
    let annotationRevision: Int
    let selected: (SelectedPassage) -> Void
    let jump: ReadingAnchor?
    let jumpID: Int
    let toggleTools: () -> Void
    let turning: () -> Void
    let visible: (ReadingAnchor, String) -> Void
    let committed: (ReadingAnchor) -> Void
    func makeUIViewController(context: Context) -> NativeReadingController {
        NativeReadingController(book: book, anchor: initial, options: options)
    }
    func updateUIViewController(_ controller: NativeReadingController, context: Context) {
        controller.toggleTools = toggleTools; controller.turning = turning
        controller.visible = visible; controller.committed = committed
        controller.selected = selected
        controller.update(options: options, jump: jump, jumpID: jumpID, annotations: annotations, revision: annotationRevision)
    }
}

final class NativeReadingController: UIViewController, UIPageViewControllerDataSource, UIPageViewControllerDelegate, UIGestureRecognizerDelegate {
    let book: PreviewBook
    var anchor: ReadingAnchor
    var options: ReaderOptions
    var pages: [ReadingPage] = []
    var annotations = ReaderAnnotations.empty
    var annotationRevision = -1
    var selected: ((SelectedPassage) -> Void)?
    var pager: UIPageViewController?
    var dual = false
    var currentIndex = 0
    var lastSize: CGSize = .zero
    var layoutOptions: ReaderOptions?
    var lastJumpID = 0
    var verticalInset: CGFloat = 80
    var toggleTools: (() -> Void)?
    var turning: (() -> Void)?
    var visible: ((ReadingAnchor, String) -> Void)?
    var committed: ((ReadingAnchor) -> Void)?
    init(book: PreviewBook, anchor: ReadingAnchor, options: ReaderOptions) {
        self.book = book; self.anchor = anchor; self.options = options
        super.init(nibName: nil, bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = options.paper
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        rebuildIfNeeded()
        pager?.view.frame = view.bounds
    }
    func update(options: ReaderOptions, jump: ReadingAnchor?, jumpID: Int, annotations: ReaderAnnotations, revision: Int) {
        if revision != annotationRevision { self.annotations = annotations; annotationRevision = revision; layoutOptions = nil }
        self.options = options
        if jumpID != lastJumpID, let jump {
            lastJumpID = jumpID
            anchor = jump
            layoutOptions = nil
        }
        if isViewLoaded { rebuildIfNeeded() }
    }
    private func chapters(in rect: CGRect) -> [LaidOutChapter] {
        BookTypesetter.chapters(book.chapters, options: options, rect: rect).enumerated().map { chapter, content in
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
    }
    private func rebuildIfNeeded() {
        guard view.window != nil else { return }
        let size = view.bounds.size
        guard size.width > 100 && size.height > 100 else { return }
        guard size != lastSize || layoutOptions != options else { return }
        let sizeChanged = size != lastSize
        lastSize = size; layoutOptions = options
        // Freeze a symmetric safe-area budget for this size. Hiding system or app
        // chrome must never repaginate; only actual size/options changes do so.
        let screenSafe = view.window?.safeAreaInsets ?? view.safeAreaInsets
        if sizeChanged { verticalInset = max(24, max(screenSafe.top, screenSafe.bottom)) + 24 }
        dual = UIDevice.current.userInterfaceIdiom == .pad && size.width >= 1000 && options.fontSize <= 24 && !options.sliding
        let leafWidth = dual ? size.width / 2 : size.width
        let textWidth = min(620, leafWidth - 48)
        let textRect = CGRect(x: (leafWidth-textWidth)/2, y: verticalInset, width: textWidth, height: max(60, size.height-verticalInset*2))
        pages = ReadingLayout.paginate(chapters(in: textRect), in: textRect)
        guard !pages.isEmpty else { return }
        currentIndex = ReadingLayout.pageIndex(for: anchor, in: pages)
        if dual { currentIndex = currentIndex / 2 * 2 }
        if let old = pager {
            old.dataSource = nil; old.delegate = nil
            old.willMove(toParent: nil); old.view.removeFromSuperview(); old.removeFromParent()
        }
        let controller = UIPageViewController(transitionStyle: options.sliding ? .scroll : .pageCurl, navigationOrientation: .horizontal, options: options.sliding ? nil : [.spineLocation: NSNumber(value: dual ? UIPageViewController.SpineLocation.mid.rawValue : UIPageViewController.SpineLocation.min.rawValue)])
        controller.isDoubleSided = dual
        controller.dataSource = self; controller.delegate = self
        pager = controller
        addChild(controller); controller.view.frame = view.bounds
        controller.view.backgroundColor = options.paper
        view.addSubview(controller.view); controller.didMove(toParent: self)
        controller.setViewControllers(dual ? [leaf(currentIndex), leaf(currentIndex+1)] : [leaf(currentIndex)], direction: .forward, animated: false)
        let tap = UITapGestureRecognizer(target: self, action: #selector(centerTapped))
        tap.delegate = self; tap.cancelsTouchesInView = false
        controller.view.addGestureRecognizer(tap)
        reportVisible()
    }
    private func leaf(_ index: Int) -> PaperController {
        let page = pages.indices.contains(index) ? pages[index] : nil
        return PaperController(index: index, page: page, paper: options.paper, selected: { [weak self] in self?.selected?($0) })
    }
    private var leafCount: Int { dual ? pages.count + pages.count % 2 : pages.count }
    func pageViewController(_ pageViewController: UIPageViewController, viewControllerBefore viewController: UIViewController) -> UIViewController? {
        guard let leaf = viewController as? PaperController, leaf.index > 0 else { return nil }
        return self.leaf(leaf.index-1)
    }
    func pageViewController(_ pageViewController: UIPageViewController, viewControllerAfter viewController: UIViewController) -> UIViewController? {
        guard let leaf = viewController as? PaperController, leaf.index+1 < leafCount else { return nil }
        return self.leaf(leaf.index+1)
    }
    func pageViewController(_ pageViewController: UIPageViewController, willTransitionTo pendingViewControllers: [UIViewController]) { turning?() }
    func pageViewController(_ pageViewController: UIPageViewController, didFinishAnimating finished: Bool, previousViewControllers: [UIViewController], transitionCompleted completed: Bool) {
        guard pageViewController === pager else { return }
        let indices = pageViewController.viewControllers?.compactMap { ($0 as? PaperController)?.index } ?? []
        guard let next = ReadingLayout.completedAnchor(completed: completed, visible: indices, pages: pages) else { return }
        anchor = next; currentIndex = indices.min() ?? currentIndex
        committed?(next); reportVisible()
    }
    private func reportVisible() {
        guard pages.indices.contains(currentIndex) else { return }
        let current = pages[currentIndex].anchor
        let end = min(pages.count, currentIndex+(dual ? 2 : 1))
        let label = dual ? "\(currentIndex+1)–\(end) / \(pages.count)" : "\(currentIndex+1) / \(pages.count)"
        let callback = visible
        DispatchQueue.main.async { callback?(current, label) }
    }
    @objc private func centerTapped() { toggleTools?() }
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        let x = touch.location(in: view).x / view.bounds.width
        return x >= 0.32 && x <= 0.68
    }
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool { true }
}

final class PaperController: UIViewController {
    let index: Int
    let page: ReadingPage?
    let paper: UIColor
    let selected: (SelectedPassage) -> Void
    init(index: Int, page: ReadingPage?, paper: UIColor, selected: @escaping (SelectedPassage) -> Void) {
        self.index = index; self.page = page; self.paper = paper; self.selected = selected
        super.init(nibName: nil, bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func loadView() {
        let surface = PaperSurface()
        surface.page = page; surface.backgroundColor = paper; surface.selected = selected
        surface.isAccessibilityElement = page != nil
        surface.accessibilityIdentifier = "reading-page-\(index)"
        if let page { surface.accessibilityLabel = page.text.attributedSubstring(from: NSRange(location: page.range.location, length: page.range.length)).string }
        view = surface
    }
}
final class PaperSurface: UIView {
    var page: ReadingPage?
    var selected: ((SelectedPassage) -> Void)?
    private var selection: NSRange?
    private var firstIndex = 0
    override init(frame: CGRect) {
        super.init(frame: frame)
        let press = UILongPressGestureRecognizer(target: self, action: #selector(selectText(_:))); press.minimumPressDuration = 0.4; addGestureRecognizer(press)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    private func frame(_ page: ReadingPage) -> CTFrame { CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(page.text), page.range, CGPath(rect: page.textRect, transform: nil), nil) }
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
    @objc private func selectText(_ gesture: UILongPressGestureRecognizer) {
        guard let page else { return }
        let text = page.text.string as NSString
        switch gesture.state {
        case .began:
            guard let at = index(at: gesture.location(in: self), page: page) else { return }
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
        guard let page, let context = UIGraphicsGetCurrentContext() else { return }
        context.textMatrix = .identity
        context.translateBy(x: 0, y: bounds.height); context.scaleBy(x: 1, y: -1)
        let setter = CTFramesetterCreateWithAttributedString(page.text)
        let frame = CTFramesetterCreateFrame(setter, page.range, CGPath(rect: page.textRect, transform: nil), nil)
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = Array(repeating: CGPoint.zero, count: lines.count); CTFrameGetLineOrigins(frame, CFRange(location: 0,length: 0), &origins)
        let origin = CTFrameGetPath(frame).boundingBox.origin
        for (i,line) in lines.enumerated() {
            let lineRange = CTLineGetStringRange(line)
            let range = NSRange(location: lineRange.location, length: lineRange.length)
            page.text.enumerateAttribute(NSAttributedString.Key("ReaderHighlight"), in: range) { value, range, _ in
                if let color = value as? UIColor { self.paint(range, color: color, line: line, origin: CGPoint(x: origin.x+origins[i].x,y: origin.y+origins[i].y), context: context) }
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
