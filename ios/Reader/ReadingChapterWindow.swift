import Foundation

// Heavy text layouts stay bounded even though the parsed publication is loaded.
final class ReadingChapterCache<Source: Equatable, Value> {
    private var entries: [Int: (Source, Value)] = [:]
    private(set) var builds = 0
    var count: Int { entries.count }
    var chapters: [Int] { entries.keys.sorted() }
    static func range(around chapter: Int, count: Int, radius: Int) -> Range<Int> {
        guard count > 0 else { return 0..<0 }
        let center = min(count - 1, max(0, chapter))
        return max(0, center - radius)..<min(count, center + radius + 1)
    }
    func retain(_ range: Range<Int>) { entries = entries.filter { range.contains($0.key) } }
    func clear() { entries.removeAll() }
    func value(chapter: Int, source: Source, make: () -> Value) -> Value {
        if let entry = entries[chapter], entry.0 == source { return entry.1 }
        let value = make(); builds += 1; entries[chapter] = (source, value); return value
    }
}
