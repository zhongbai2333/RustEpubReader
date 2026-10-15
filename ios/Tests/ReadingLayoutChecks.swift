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
        for page in narrow {
            let continuous = ReadingLayout.continuousPage(page)
            let cached = ReadingLineMap(continuous)
            for y in stride(from: CGFloat(0), through: continuous.surfaceHeight!, by: 7) {
                precondition(cached.anchor(at: y) == ReadingLayout.anchor(atVerticalOffset: y, in: continuous), "Cached scroll position changed line coordinates")
            }
            let frame = CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(continuous.text), continuous.range, CGPath(rect: continuous.textRect, transform: nil), nil)
            precondition(CTFrameGetVisibleStringRange(frame).length == page.range.length, "Continuous row clipped text")
            let top = ReadingLayout.verticalOffset(for: page.anchor, in: continuous)
            precondition(ReadingLayout.anchor(atVerticalOffset: top, in: continuous) == page.anchor, "Scroll restoration changed logical position")
            let middle = ReadingLayout.anchor(atVerticalOffset: continuous.textRect.height / 2, in: continuous)
            precondition(middle.chapter == page.chapter && middle.block >= page.anchor.block)
            let restored = ReadingLayout.verticalOffset(for: middle, in: continuous)
            let roundTrip = ReadingLayout.anchor(atVerticalOffset: restored, in: continuous)
            if roundTrip != middle { print("MISMATCH", page.range, middle, restored, roundTrip); fflush(stdout) }
            precondition(roundTrip == middle, "Within-page scroll restoration failed")
        }
        let inner = ReadingLayout.continuousPage(narrow[1], topInset: 0, bottomInset: 0)
        precondition(inner.surfaceHeight == inner.textRect.height && inner.textRect.minY == 0, "Within-chapter rows retained page gutters")
        let innerMiddle = ReadingLayout.anchor(atVerticalOffset: inner.textRect.height / 2, in: inner)
        precondition(ReadingLayout.anchor(atVerticalOffset: ReadingLayout.verticalOffset(for: innerMiddle, in: inner), in: inner) == innerMiddle)
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
        let table=LaidOutChapter(text:NSAttributedString(string:"\u{fffc}\nNext\n"),coordinates:[.init(block:1,start:0,source:"A long original table row",renderLength:1),.init(block:2,start:2,source:"Next")])
        precondition(table.utf16(for:.init(chapter:0,block:1,charOffset:20))==1,"Table source offset escaped its attachment")
        precondition(table.anchor(chapter:0,at:0).charOffset==0)
        precondition(table.anchor(chapter:0,at:1).charOffset==25)
        let shared = ReadingAnchor(chapter: 2, block: 1, charOffset: 4)
        precondition(mapped.anchor(chapter: 2, at: mapped.utf16(for: shared)) == shared)
        precondition(mapped.anchor(chapter: 2, at: 1).charOffset == 1, "Synthetic newline must not count in shared position")
        precondition(ReadingLayout.completedAnchor(completed: false, visible: [2], pages: narrow) == nil)
        precondition(ReadingLayout.completedAnchor(completed: true, visible: [3,2], pages: narrow) == narrow[2].anchor)
        precondition(ReadingLayout.completedAnchor(completed: true, visible: [narrow.count], pages: narrow) == nil)
        // Publisher spacing/font metrics larger than the viewport must retain the tail.
        var authoredSpacing: CGFloat = 4000
        let hostileParagraph = withUnsafePointer(to: &authoredSpacing) { pointer in
            var settings = [CTParagraphStyleSetting(spec: .paragraphSpacingBefore, valueSize: MemoryLayout<CGFloat>.size, value: pointer)]
            return CTParagraphStyleCreate(&settings, settings.count)
        }
        let hostileSource = "这一段不能因为排不下而丢失。😀\n后续正文也必须完整保留。"
        let hostile = LaidOutChapter(text: NSAttributedString(string: hostileSource, attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("PingFangSC-Regular" as CFString, 800, nil),
            NSAttributedString.Key(kCTParagraphStyleAttributeName as String): hostileParagraph
        ]), coordinates: [.init(block:0,start:0,source:hostileSource)])
        let repaired = ReadingLayout.paginate([hostile],in:CGRect(x:0,y:0,width:320,height:200))
        var consumed = 0
        for page in repaired {precondition(page.range.location == consumed); consumed += page.range.length}
        precondition(consumed == hostile.text.length, "Unplaceable paragraph discarded a chapter tail")
        precondition(repaired.last!.text.string == hostileSource, "Fallback changed canonical text")
        print("PASS: continuous row coverage and within-page anchor restoration, contiguous Unicode pagination, block/scalar UTF-16 round trips, reflow anchor, canceled transition, dual visible anchor, blank sentinel")
    }
}
