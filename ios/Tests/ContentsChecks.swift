import Foundation
@main struct ContentsChecks {
    static func main() {
        let chapters = ["Start", "Café", "江月何年初照人", ""]
        let rows = ReaderContentsModel.rows(toc: [("bad", -1), ("First", 0), ("", 1), ("江月", 2), ("bad", 99)], chapters: chapters)
        precondition(rows.map(\.id) == [1, 2, 3])
        precondition(rows.map(\.chapter) == [0, 1, 2])
        let found = ReaderContentsModel.filter(rows, query: "  江月  ")
        precondition(found.count == 1 && found[0].id == 3 && found[0].chapter == 2)
        precondition(ReaderContentsModel.filter(rows, query: "CAFE").first?.chapter == 1)
        precondition(ReaderContentsModel.filter(rows, query: "not present").isEmpty)
        precondition(ReaderContentsModel.filter(rows, query: " \n ") == rows)
        let fallback = ReaderContentsModel.rows(toc: [("invalid", 999)], chapters: chapters)
        precondition(fallback.map(\.chapter) == [0, 1, 2, 3] && fallback.last?.title == "第 4 章")
        precondition(ReaderContentsModel.rows(toc: [], chapters: []).isEmpty)
        print("PASS: original chapter IDs preserved through filtering, Unicode title search, invalid indices rejected, real-chapter fallback, empty data")
    }
}
