import Foundation
import CoreText

private final class FittedReadingRun {
    let size: CGSize
    init(_ size: CGSize) { self.size = size }
}

// Shared Rust/cloud coordinate. Swift's grapheme count and UTF-16 count are not scalar offsets.
struct ReadingAnchor: Codable, Equatable {
    var chapter: Int
    var block: Int = 0
    var charOffset: Int = 0
    enum CodingKeys: String, CodingKey { case chapter, block; case charOffset = "char_offset" }
}

struct ReadingReturnPoint: Codable, Equatable {
    let anchor: ReadingAnchor
    let chapterTitle: String
    let pageLabel: String
}

struct ReadingJumpHistory: Codable, Equatable {
    private(set) var points: [ReadingReturnPoint] = []
    var previous: ReadingReturnPoint? { points.last }
    mutating func record(_ origin: ReadingReturnPoint, target: ReadingAnchor) {
        guard origin.anchor != target, points.last?.anchor != origin.anchor else { return }
        points.append(origin)
        if points.count > 16 { points.removeFirst(points.count - 16) }
    }
    mutating func pop() -> ReadingReturnPoint? { points.popLast() }
}
struct BlockCoordinate {
    let block: Int
    let start: Int
    let source: String
    var base: Int = 0
    var renderLength: Int? = nil
    var length: Int { renderLength ?? source.utf16.count }
    func scalar(atUTF16 offset: Int) -> Int {
        if let rendered=renderLength {return offset>=rendered ? source.unicodeScalars.count : 0}
        var units = 0, scalars = 0
        for scalar in source.unicodeScalars {
            let next = units + (scalar.value > 0xffff ? 2 : 1)
            if next > max(0, offset) { break }
            units = next; scalars += 1
        }
        return scalars
    }
    func utf16(atScalar offset: Int) -> Int { if let rendered=renderLength {return offset<=0 ? 0:rendered};return source.unicodeScalars.prefix(max(0, offset)).reduce(0) { $0 + ($1.value > 0xffff ? 2 : 1) } }
}
struct LaidOutChapter {
    let text: NSAttributedString
    let coordinates: [BlockCoordinate]
    func anchor(chapter: Int, at offset: Int) -> ReadingAnchor {
        guard !coordinates.isEmpty else { return .init(chapter: chapter) }
        var low = 0, high = coordinates.count
        while low < high {
            let mid = (low + high) / 2
            if coordinates[mid].start <= offset { low = mid + 1 } else { high = mid }
        }
        let item = coordinates[max(0, low - 1)]
        return .init(chapter: chapter, block: item.block, charOffset: item.base + item.scalar(atUTF16: min(item.length, max(0, offset - item.start))))
    }
    func utf16(for anchor: ReadingAnchor) -> Int {
        let matches = coordinates.filter { $0.block == anchor.block }
        guard let item = (anchor.charOffset == 0 ? matches.first : matches.last(where: { !$0.source.isEmpty && $0.base <= anchor.charOffset })) ?? matches.first else { return 0 }
        return item.start + item.utf16(atScalar: max(0, anchor.charOffset - item.base))
    }
}
struct ReadingPage {
    let chapter: Int
    let content: LaidOutChapter
    let range: CFRange
    let textRect: CGRect
    var surfaceHeight: CGFloat? = nil
    var framesetter: CTFramesetter? = nil
    var text: NSAttributedString { content.text }
    var anchor: ReadingAnchor { content.anchor(chapter: chapter, at: range.location) }
}
enum ReadingLayout {
    static func paginate(_ chapters: [LaidOutChapter], in rect: CGRect, chapterOffset: Int = 0) -> [ReadingPage] {
        guard rect.width > 0, rect.height > 0 else { return [] }
        var pages: [ReadingPage] = []
        for (chapter, content) in chapters.enumerated() {
            var content = content
            var setter = CTFramesetterCreateWithAttributedString(content.text)
            var offset = 0
            while offset < content.text.length {
                var frame = CTFramesetterCreateFrame(setter, CFRange(location: offset, length: 0), CGPath(rect: rect, transform: nil), nil)
                var visible = CTFrameGetVisibleStringRange(frame)
                if visible.length == 0 {
                    // An authored line/margin larger than the viewport must not
                    // silently discard the rest of the chapter. Repair only the
                    // unplaceable paragraph, preserving all source coordinates.
                    let repaired = NSMutableAttributedString(attributedString: content.text)
                    let range = (content.text.string as NSString).paragraphRange(for: NSRange(location: offset, length: 1))
                    var zero: CGFloat = 0
                    var lineHeight = min(32, rect.height * 0.5)
                    let paragraph = withUnsafePointer(to: &zero) { zeroPointer in
                        withUnsafePointer(to: &lineHeight) { heightPointer in
                            var settings = [
                                CTParagraphStyleSetting(spec: .paragraphSpacingBefore, valueSize: MemoryLayout<CGFloat>.size, value: zeroPointer),
                                CTParagraphStyleSetting(spec: .paragraphSpacing, valueSize: MemoryLayout<CGFloat>.size, value: zeroPointer),
                                CTParagraphStyleSetting(spec: .minimumLineHeight, valueSize: MemoryLayout<CGFloat>.size, value: heightPointer),
                                CTParagraphStyleSetting(spec: .maximumLineHeight, valueSize: MemoryLayout<CGFloat>.size, value: heightPointer)
                            ]
                            return CTParagraphStyleCreate(&settings, settings.count)
                        }
                    }
                    repaired.addAttributes([
                        NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica" as CFString, min(18, rect.height * 0.25), nil),
                        NSAttributedString.Key(kCTParagraphStyleAttributeName as String): paragraph
                    ], range: range)
                    repaired.enumerateAttribute(NSAttributedString.Key(kCTRunDelegateAttributeName as String), in: range) { value, run, _ in
                        guard value != nil else { return }
                        let line = CTTypesetterCreateLine(CTFramesetterGetTypesetter(setter), CFRange(location: run.location, length: run.length))
                        var ascent: CGFloat = 0, descent: CGFloat = 0
                        let width = CGFloat(CTLineGetTypographicBounds(line, &ascent, &descent, nil))
                        let height = max(1, ascent + descent)
                        let scale = min(1, min(rect.width / max(1, width), rect.height * 0.75 / height))
                        let metrics = Unmanaged.passRetained(FittedReadingRun(CGSize(width: max(1, width * scale), height: max(1, height * scale)))).toOpaque()
                        var callbacks = CTRunDelegateCallbacks(version: kCTRunDelegateVersion1,
                            dealloc: { Unmanaged<FittedReadingRun>.fromOpaque($0).release() },
                            getAscent: { Unmanaged<FittedReadingRun>.fromOpaque($0).takeUnretainedValue().size.height },
                            getDescent: { _ in 0 },
                            getWidth: { Unmanaged<FittedReadingRun>.fromOpaque($0).takeUnretainedValue().size.width })
                        if let delegate = CTRunDelegateCreate(&callbacks, metrics) {
                            repaired.addAttribute(NSAttributedString.Key(kCTRunDelegateAttributeName as String), value: delegate, range: run)
                        } else { Unmanaged<FittedReadingRun>.fromOpaque(metrics).release() }
                    }
                    content = LaidOutChapter(text: repaired, coordinates: content.coordinates)
                    setter = CTFramesetterCreateWithAttributedString(content.text)
                    frame = CTFramesetterCreateFrame(setter, CFRange(location: offset, length: 0), CGPath(rect: rect, transform: nil), nil)
                    visible = CTFrameGetVisibleStringRange(frame)
                }
                if visible.length == 0 {
                    // Keep the unread suffix in a continuous surface as the final
                    // fallback. Its full height is exposed by the page controller.
                    let remainder = CFRange(location: offset, length: content.text.length-offset)
                    let fitted = CTFramesetterSuggestFrameSizeWithConstraints(setter, remainder, nil,
                        CGSize(width: rect.width, height: .greatestFiniteMagnitude), nil)
                    let expanded = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: max(rect.height, ceil(fitted.height)+2))
                    pages.append(.init(chapter: chapter + chapterOffset, content: content, range: remainder, textRect: expanded,
                        surfaceHeight: expanded.height, framesetter: setter))
                    offset = content.text.length
                    continue
                }
                pages.append(.init(chapter: chapter + chapterOffset, content: content, range: visible, textRect: rect, framesetter: setter))
                offset += visible.length
            }
        }
        return pages
    }
    static func continuousPage(_ page: ReadingPage, topInset: CGFloat = 24, bottomInset: CGFloat = 24) -> ReadingPage {
        let setter = page.framesetter ?? CTFramesetterCreateWithAttributedString(page.text)
        let fit = CTFramesetterSuggestFrameSizeWithConstraints(setter, page.range, nil,
            CGSize(width: page.textRect.width, height: .greatestFiniteMagnitude), nil)
        return .init(chapter: page.chapter, content: page.content, range: page.range,
                     textRect: CGRect(x: page.textRect.minX, y: bottomInset, width: page.textRect.width, height: ceil(fit.height) + 2),
                     surfaceHeight: ceil(fit.height) + 2 + topInset + bottomInset, framesetter: setter)
    }
    private static func lines(_ page: ReadingPage) -> [(range: CFRange, top: Double, bottom: Double)] {
        let frame = CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(page.text), page.range,
            CGPath(rect: page.textRect, transform: nil), nil)
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = Array(repeating: CGPoint.zero, count: lines.count)
        CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
        return lines.enumerated().map { index, line in
            var ascent: CGFloat = 0, descent: CGFloat = 0
            CTLineGetTypographicBounds(line, &ascent, &descent, nil)
            let height = page.surfaceHeight ?? (page.textRect.height + 48)
            let baseline = height - page.textRect.minY - origins[index].y
            return (CTLineGetStringRange(line), baseline - ascent, baseline + descent)
        }
    }
    static func verticalOffset(for anchor: ReadingAnchor, in page: ReadingPage) -> Double {
        let offset = page.content.utf16(for: anchor)
        let lines = lines(page)
        return (lines.last(where: { $0.range.location <= offset }) ?? lines.first)?.top ?? 24
    }
    static func anchor(atVerticalOffset offset: Double, in page: ReadingPage) -> ReadingAnchor {
        let lines = lines(page)
        guard let line = lines.last(where: { $0.top <= offset + 0.5 }) ?? lines.first else { return page.anchor }
        return page.content.anchor(chapter: page.chapter, at: line.range.location)
    }
    static func pageIndex(for anchor: ReadingAnchor, in pages: [ReadingPage]) -> Int {
        let candidates = pages.indices.filter { pages[$0].chapter == anchor.chapter }
        guard let first = candidates.first else { return 0 }
        let offset = pages[first].content.utf16(for: anchor)
        return candidates.last(where: { pages[$0].range.location <= offset }) ?? first
    }
    static func completedAnchor(completed: Bool, visible: [Int], pages: [ReadingPage]) -> ReadingAnchor? {
        guard completed, let first = visible.filter({ pages.indices.contains($0) }).min() else { return nil }
        return pages[first].anchor
    }
}

// Computed once for a visible paper surface, reused for scroll events and jumps.
final class ReadingLineMap {
    let frame: CTFrame
    private let page: ReadingPage
    private let entries: [(top: CGFloat, offset: Int, anchor: ReadingAnchor)]
    init(_ page: ReadingPage) {
        self.page = page
        frame = CTFramesetterCreateFrame(page.framesetter ?? CTFramesetterCreateWithAttributedString(page.text), page.range, CGPath(rect: page.textRect, transform: nil), nil)
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = Array(repeating: CGPoint.zero, count: lines.count)
        CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
        entries = lines.enumerated().map { index, line in
            var ascent: CGFloat = 0
            CTLineGetTypographicBounds(line, &ascent, nil, nil)
            let offset = CTLineGetStringRange(line).location
            let top = (page.surfaceHeight ?? (page.textRect.height + 48)) - page.textRect.minY - origins[index].y - ascent
            return (top, offset, page.content.anchor(chapter: page.chapter, at: offset))
        }
    }
    func anchor(at offset: CGFloat) -> ReadingAnchor {
        (entries.last(where: { $0.top <= offset + 0.5 }) ?? entries.first)?.anchor ?? page.anchor
    }
    func verticalOffset(for anchor: ReadingAnchor) -> CGFloat {
        let offset = page.content.utf16(for: anchor)
        return (entries.last(where: { $0.offset <= offset }) ?? entries.first)?.top ?? 24
    }
}
