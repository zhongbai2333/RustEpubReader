import Foundation

struct ReaderSearchHit: Identifiable, Sendable {
    let chapter: Int
    let block: Int
    let offset: Int
    let title: String
    let before: String
    let match: String
    let after: String
    var id: String { "\(chapter)/\(block)/\(offset)" }
    var anchor: ReadingAnchor { .init(chapter: chapter, block: block, charOffset: offset) }
}
struct ReaderSearchResults: Sendable {
    var hits: [ReaderSearchHit] = []
    var hasMore = false
}

// Search the already-open native publication, keeping scalar coordinates used by
// CoreText anchors. Rendered fragments around inline images share a core block.
enum ReaderBookSearch {
    static func find(_ query: String, in book: PreviewBook, limit: Int = 500) -> ReaderSearchResults {
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty, limit > 0 else { return .init() }
        var result = ReaderSearchResults()
        for (chapterIndex, chapter) in book.chapters.enumerated() {
            if Task.isCancelled { return .init() }
            var groups: [Int: [(offset: Int, text: String)]] = [:]
            for (index, block) in chapter.blocks.enumerated() where block.type == "Paragraph" || block.type == "Heading" {
                groups[block.core_block ?? index, default: []].append((block.core_char_offset ?? 0, (block.spans ?? []).map(\.text).joined()))
            }
            for block in groups.keys.sorted() {
                if Task.isCancelled { return .init() }
                var scalars: [Unicode.Scalar] = [], coordinates: [Int] = []
                for fragment in groups[block]!.sorted(by: { $0.offset < $1.offset }) {
                    for (index, scalar) in fragment.text.unicodeScalars.enumerated() {
                        scalars.append(scalar); coordinates.append(fragment.offset + index)
                    }
                }
                let text = String(String.UnicodeScalarView(scalars))
                var start = text.startIndex
                while start < text.endIndex, let range = text.range(of: query, options: .caseInsensitive, range: start..<text.endIndex) {
                    if Task.isCancelled { return .init() }
                    if result.hits.count == limit { result.hasMore = true; return result }
                    let begin = text.unicodeScalars.distance(from: text.unicodeScalars.startIndex, to: range.lowerBound)
                    let end = text.unicodeScalars.distance(from: text.unicodeScalars.startIndex, to: range.upperBound)
                    let lo = max(0, begin - 24), hi = min(scalars.count, end + 24)
                    let before = (lo > 0 ? "…" : "") + String(String.UnicodeScalarView(scalars[lo..<begin]))
                    let match = String(String.UnicodeScalarView(scalars[begin..<end]))
                    let after = String(String.UnicodeScalarView(scalars[end..<hi])) + (hi < scalars.count ? "…" : "")
                    result.hits.append(.init(chapter: chapterIndex, block: block, offset: coordinates[begin], title: chapter.title, before: before, match: match, after: after))
                    start = range.upperBound
                }
            }
        }
        return result
    }
}

import SwiftUI

struct ReaderSearchView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    let book: PreviewBook
    let choose: (ReadingAnchor) -> Void
    @State private var query = ""
    @State private var results = ReaderSearchResults()
    @State private var searching = false
    private var term: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }
    var body: some View {
        Group {
            if term.isEmpty {
                ContentUnavailableView(L("reader.search_in_book"), systemImage: "magnifyingglass", description: Text(L("reader.search_hint")))
            } else if searching {
                ProgressView(L("common.searching")).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if results.hits.isEmpty {
                ContentUnavailableView(L("common.no_matches_found"), systemImage: "magnifyingglass", description: Text(L("common.try_other_keywords")))
            } else {
                List {
                    Section {
                        ForEach(results.hits) { hit in
                            Button { choose(hit.anchor) } label: {
                                VStack(alignment: .leading, spacing: 7) {
                                    Text(L("reader.search_result_location", hit.chapter + 1, hit.title)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                    (Text(hit.before) + Text(hit.match).bold() + Text(hit.after)).foregroundStyle(.primary).lineLimit(4)
                                }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 4).contentShape(Rectangle())
                            }.buttonStyle(.plain)
                        }
                    } header: { Text(results.hasMore ? L("reader.search_first_500_matches") : readerI18n.count("reader.search_match_count", results.hits.count)) }
                        footer: { if results.hasMore { Text(L("reader.search_results_limited")) } }
                }
            }
        }
        .navigationTitle(L("reader.search_book")).navigationBarTitleDisplayMode(.inline)
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: L("reader.search_in_book"))
        .task(id: term) {
            let term = self.term
            results = .init(); searching = !term.isEmpty
            guard !term.isEmpty else { return }
            do { try await Task.sleep(for: .milliseconds(300)) } catch { return }
            let task = Task.detached(priority: .userInitiated) { ReaderBookSearch.find(term, in: book) }
            let found = await withTaskCancellationHandler { await task.value } onCancel: { task.cancel() }
            guard !Task.isCancelled, self.term == term else { return }
            results = found; searching = false
        }
    }
}
