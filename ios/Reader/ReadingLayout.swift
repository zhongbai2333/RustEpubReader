import Foundation
import CoreText

// Shared Rust/cloud coordinate. Swift's grapheme count and UTF-16 count are not scalar offsets.
struct ReadingAnchor: Codable, Equatable {
    var chapter: Int
    var block: Int = 0
    var charOffset: Int = 0
    enum CodingKeys: String, CodingKey { case chapter, block; case charOffset = "char_offset" }
}
struct BlockCoordinate {
    let block: Int
    let start: Int
    let source: String
    var base: Int = 0
    var length: Int { source.utf16.count }
    func scalar(atUTF16 offset: Int) -> Int {
        var units = 0, scalars = 0
        for scalar in source.unicodeScalars {
            let next = units + (scalar.value > 0xffff ? 2 : 1)
            if next > max(0, offset) { break }
            units = next; scalars += 1
        }
        return scalars
    }
    func utf16(atScalar offset: Int) -> Int { source.unicodeScalars.prefix(max(0, offset)).reduce(0) { $0 + ($1.value > 0xffff ? 2 : 1) } }
}
struct LaidOutChapter {
    let text: NSAttributedString
    let coordinates: [BlockCoordinate]
    func anchor(chapter: Int, at offset: Int) -> ReadingAnchor {
        guard let item = coordinates.last(where: { $0.start <= offset }) ?? coordinates.first else { return .init(chapter: chapter) }
        return .init(chapter: chapter, block: item.block, charOffset: item.base + item.scalar(atUTF16: min(item.length, max(0, offset - item.start))))
    }
    func utf16(for anchor: ReadingAnchor) -> Int {
        let matches = coordinates.filter { $0.block == anchor.block }
        guard let item = (anchor.charOffset == 0 ? matches.first : matches.first(where: { !$0.source.isEmpty })) ?? matches.first else { return 0 }
        return item.start + item.utf16(atScalar: max(0, anchor.charOffset - item.base))
    }
}
struct ReadingPage {
    let chapter: Int
    let content: LaidOutChapter
    let range: CFRange
    let textRect: CGRect
    var text: NSAttributedString { content.text }
    var anchor: ReadingAnchor { content.anchor(chapter: chapter, at: range.location) }
}
enum ReadingLayout {
    static func paginate(_ chapters: [LaidOutChapter], in rect: CGRect) -> [ReadingPage] {
        guard rect.width > 0, rect.height > 0 else { return [] }
        var pages: [ReadingPage] = []
        for (chapter, content) in chapters.enumerated() {
            let setter = CTFramesetterCreateWithAttributedString(content.text)
            var offset = 0
            while offset < content.text.length {
                let frame = CTFramesetterCreateFrame(setter, CFRange(location: offset, length: 0), CGPath(rect: rect, transform: nil), nil)
                let visible = CTFrameGetVisibleStringRange(frame)
                guard visible.length > 0 else { break }
                pages.append(.init(chapter: chapter, content: content, range: visible, textRect: rect))
                offset += visible.length
            }
        }
        return pages
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
