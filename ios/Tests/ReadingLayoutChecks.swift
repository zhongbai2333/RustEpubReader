import Foundation
import CoreText

@main struct ReadingLayoutChecks {
    static func main() {
        let sample = "Reading 😀 e\u{301} 中文。A paragraph with stable positions.\n"
        let attrs: [NSAttributedString.Key: Any] = [NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("PingFangSC-Regular" as CFString, 18, nil)]
        let chapters = (0..<3).map { index -> LaidOutChapter in
            let text = NSMutableAttributedString(string: "")
            var coordinates: [BlockCoordinate] = []
            for block in 0..<70 {
                coordinates.append(.init(block: block, start: text.length, source: sample.trimmingCharacters(in: .newlines)))
                text.append(NSAttributedString(string: sample, attributes: attrs))
            }
            return .init(text: text, coordinates: coordinates)
        }
        let narrow = ReadingLayout.paginate(chapters, in: CGRect(x: 24, y: 80, width: 340, height: 560))
        let wide = ReadingLayout.paginate(chapters, in: CGRect(x: 32, y: 56, width: 500, height: 760))
        precondition(narrow.count > wide.count)
        for chapter in chapters.indices {
            let slices = narrow.filter { $0.chapter == chapter }
            var next = 0
            for page in slices {
                precondition(page.range.location == next && page.range.length > 0)
                next += page.range.length
            }
            precondition(next == chapters[chapter].text.length, "Pagination dropped text")
            let rebuilt = slices.map { chapters[chapter].text.attributedSubstring(from: NSRange(location: $0.range.location, length: $0.range.length)).string }.joined()
            precondition(rebuilt == chapters[chapter].text.string, "Pagination duplicated or changed Unicode text")
        }
        let anchor = narrow[narrow.count / 2].anchor
        let index = ReadingLayout.pageIndex(for: anchor, in: wide)
        let offset = wide[index].content.utf16(for: anchor)
        precondition(wide[index].chapter == anchor.chapter && wide[index].range.location <= offset)
        precondition(offset < wide[index].range.location + wide[index].range.length)
        let unicode = BlockCoordinate(block: 7, start: 3, source: "中😀e\u{301}文")
        precondition(unicode.scalar(atUTF16: 3) == 2, "Emoji occupies two UTF-16 units but one scalar")
        precondition(unicode.utf16(atScalar: 4) == 5, "Combining mark is its own scalar")
        precondition(unicode.scalar(atUTF16: 2) == 1, "Do not split a surrogate pair")
        for scalar in 0...5 { precondition(unicode.scalar(atUTF16: unicode.utf16(atScalar: scalar)) == scalar) }
        let mapped = LaidOutChapter(text: NSAttributedString(string: "X\n中😀e\u{301}文\n"), coordinates: [.init(block: 0, start: 0, source: "X"), .init(block: 1, start: 2, source: "中😀e\u{301}文")])
        let decorated = LaidOutChapter(text: NSAttributedString(string: "\u{fffc}\n中😀e\u{301}文\n"), coordinates: [.init(block: 1, start: 0, source: ""), .init(block: 1, start: 2, source: "中😀e\u{301}文")])
        precondition(decorated.utf16(for: .init(chapter: 0, block: 1, charOffset: 4)) == 7, "Render-only media must not consume shared characters")
        precondition(decorated.anchor(chapter: 0, at: 0) == .init(chapter: 0, block: 1))
        let shared = ReadingAnchor(chapter: 2, block: 1, charOffset: 4)
        precondition(mapped.anchor(chapter: 2, at: mapped.utf16(for: shared)) == shared)
        precondition(mapped.anchor(chapter: 2, at: 1).charOffset == 1, "Synthetic newline must not count in shared position")
        precondition(ReadingLayout.completedAnchor(completed: false, visible: [2], pages: narrow) == nil)
        precondition(ReadingLayout.completedAnchor(completed: true, visible: [3,2], pages: narrow) == narrow[2].anchor)
        precondition(ReadingLayout.completedAnchor(completed: true, visible: [narrow.count], pages: narrow) == nil)
        print("PASS: contiguous Unicode pagination, block/scalar UTF-16 round trips, reflow anchor, canceled transition, dual visible anchor, blank sentinel")
    }
}
