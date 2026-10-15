import Foundation

struct ReaderContentsRow: Identifiable, Equatable {
    let id: Int
    let chapter: Int
    let title: String
}

enum ReaderContentsModel {
    static func rows(toc: [(title: String, chapter: Int)], chapters: [String]) -> [ReaderContentsRow] {
        func fallback(_ chapter: Int) -> String {
            let title = chapters[chapter].trimmingCharacters(in: .whitespacesAndNewlines)
            return title.isEmpty ? L("annotations.chapter", chapter + 1) : title
        }
        let valid = toc.enumerated().compactMap { index, item -> ReaderContentsRow? in
            guard chapters.indices.contains(item.chapter) else { return nil }
            let title = item.title.trimmingCharacters(in: .whitespacesAndNewlines)
            return .init(id: index, chapter: item.chapter, title: title.isEmpty ? fallback(item.chapter) : title)
        }
        if !valid.isEmpty { return valid }
        return chapters.indices.map { .init(id: $0, chapter: $0, title: fallback($0)) }
    }
    static func filter(_ rows: [ReaderContentsRow], query: String) -> [ReaderContentsRow] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return term.isEmpty ? rows : rows.filter { $0.title.localizedStandardContains(term) }
    }
}
