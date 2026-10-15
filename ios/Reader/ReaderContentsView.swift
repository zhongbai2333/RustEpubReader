import SwiftUI

struct ReaderContentsView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    let book: PreviewBook
    let jump: (ReadingAnchor) -> Void
    @State private var query = ""
    @State private var openedChapter: Int
    init(book: PreviewBook, current: ReadingAnchor, jump: @escaping (ReadingAnchor) -> Void) {
        self.book = book; self.jump = jump
        _openedChapter = State(initialValue: current.chapter)
    }
    private var rows: [ReaderContentsRow] {
        ReaderContentsModel.rows(toc: book.toc.map { ($0.title, $0.chapter_index) }, chapters: book.chapterTitles)
    }
    private var content: some View {
        ReaderContentsList(rows: rows, current: rows.first { $0.chapter == openedChapter }?.id, query: $query, jump: jump)
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: L("reader.search_chapter_titles"))
            .navigationTitle(L("toc.title")).navigationBarTitleDisplayMode(.inline)
    }
    var body: some View {
        if #available(iOS 17.1, *) { content.searchPresentationToolbarBehavior(.avoidHidingContent) }
        else { content }
    }
}

private struct ReaderContentsList: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    let rows: [ReaderContentsRow]
    let current: Int?
    @Binding var query: String
    let jump: (ReadingAnchor) -> Void
    @Environment(\.dismissSearch) private var dismissSearch
    private var matches: [ReaderContentsRow] { ReaderContentsModel.filter(rows, query: query) }
    var body: some View {
        ScrollViewReader { proxy in
            List(matches) { row in
                let selected = row.id == current
                Button { jump(.init(chapter: row.chapter)) } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text(row.title).font(.body).fontWeight(selected ? .semibold : .regular)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 0)
                        if selected { Text(L("common.current")).font(.caption.weight(.medium)).foregroundStyle(.secondary).fixedSize() }
                    }.foregroundStyle(.primary).frame(maxWidth: .infinity, minHeight: 24, alignment: .leading)
                        .padding(.vertical, 10).padding(.horizontal, 20).contentShape(Rectangle())
                }.buttonStyle(.plain)
                    .accessibilityLabel(selected ? L("reader.current_chapter_label", row.title) : row.title)
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(selected ? Color(uiColor: .secondarySystemFill) : Color.clear)
                    .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] + 20 }
                    .id(row.id)
            }
            .listStyle(.plain).scrollContentBackground(.hidden)
            .overlay {
                if rows.isEmpty { ContentUnavailableView(L("reader.no_chapters_available"), systemImage: "list.bullet") }
                else if matches.isEmpty {
                    ContentUnavailableView(L("reader.no_matching_chapters"), systemImage: "magnifyingglass", description: Text(L("common.try_other_keywords")))
                }
            }
            .onAppear { if let current { proxy.scrollTo(current, anchor: .center) } }
            .onChange(of: query) { _, value in
                if value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, let current {
                    Task { @MainActor in await Task.yield(); proxy.scrollTo(current, anchor: .center) }
                }
            }
            .toolbar {
                if let current {
                    ToolbarItem(placement: .topBarLeading) {
                        Button {
                            query = ""; dismissSearch()
                            Task { @MainActor in await Task.yield(); proxy.scrollTo(current, anchor: .center) }
                        } label: { Image(systemName: "location") }
                            .accessibilityLabel(L("reader.locate_current_chapter"))
                    }
                }
            }
        }
    }
}
