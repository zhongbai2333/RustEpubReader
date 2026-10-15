import Foundation

struct SpeechSourceBlock {
    let text: String
    let anchor: ReadingAnchor
}

struct SpeechTextUnit {
    let text: String
    let anchor: ReadingAnchor
    let end: ReadingAnchor
    let sourceIndex: Int
}

enum SpeechText {
    // Keep service requests bounded and never divide an extended grapheme.
    static func units(_ sources: [SpeechSourceBlock], from anchor: ReadingAnchor, limit: Int = 300) -> [SpeechTextUnit] {
        var result: [SpeechTextUnit] = []
        for (sourceIndex, source) in sources.enumerated() {
            guard source.anchor.chapter > anchor.chapter ||
                    (source.anchor.chapter == anchor.chapter && source.anchor.block >= anchor.block) else { continue }
            let offset = source.anchor.chapter == anchor.chapter && source.anchor.block == anchor.block
                ? max(0, anchor.charOffset - source.anchor.charOffset) : 0
            var scalarOffset = 0
            var start = source.text.startIndex
            while start < source.text.endIndex && scalarOffset < offset {
                scalarOffset += source.text[start].unicodeScalars.count
                start = source.text.index(after: start)
            }
            while start < source.text.endIndex {
                let initialOffset = scalarOffset
                var end = start
                var count = 0
                while end < source.text.endIndex {
                    let size = source.text[end].unicodeScalars.count
                    if count > 0 && count + size > max(1, limit) { break }
                    count += size
                    end = source.text.index(after: end)
                }
                let text = String(source.text[start..<end])
                scalarOffset += count
                if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    result.append(.init(text: text,
                        anchor: .init(chapter: source.anchor.chapter, block: source.anchor.block, charOffset: source.anchor.charOffset + initialOffset),
                        end: .init(chapter: source.anchor.chapter, block: source.anchor.block, charOffset: source.anchor.charOffset + scalarOffset),
                        sourceIndex: sourceIndex))
                }
                start = end
            }
        }
        return result
    }

    static func scalarOffset(in text: String, utf16Offset: Int) -> Int {
        var units = 0, count = 0
        for scalar in text.unicodeScalars {
            let next = units + (scalar.value > 0xffff ? 2 : 1)
            if next > max(0, utf16Offset) { break }
            units = next; count += 1
        }
        return count
    }

    static func xml(_ text: String) -> String {
        String(String.UnicodeScalarView(text.unicodeScalars.filter {
            $0.value >= 32 || $0 == "\n" || $0 == "\r" || $0 == "\t"
        })).replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\"", with: "&quot;")
            .replacingOccurrences(of: "'", with: "&apos;")
    }
}

struct SpeechHighlight: Equatable {
    let start: ReadingAnchor
    let end: ReadingAnchor
}
