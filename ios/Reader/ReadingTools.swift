import SwiftUI
import UIKit
struct ReadingToolsMenu: UIViewRepresentable {
    var visibility: (Bool) -> Void
    var annotations: () -> Void
    var correction: () -> Void
    final class Coordinator {
        var parent: ReadingToolsMenu
        init(_ parent: ReadingToolsMenu) { self.parent = parent }
    }
    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeUIView(context: Context) -> ReaderMenuButton {
        let button = ReaderMenuButton(type: .system)
        button.setImage(UIImage(systemName: "ellipsis"), for: .normal)
        button.setPreferredSymbolConfiguration(.init(pointSize: 20), forImageIn: .normal)
        button.accessibilityLabel = "阅读工具"
        button.showsMenuAsPrimaryAction = true
        let coordinator = context.coordinator
        button.visibility = { coordinator.parent.visibility($0) }
        button.menu = UIMenu(children: [
            UIAction(title: "批注", image: UIImage(systemName: "highlighter")) { _ in coordinator.parent.annotations() },
            UIAction(title: "中文纠错", image: UIImage(systemName: "text.badge.checkmark")) { _ in coordinator.parent.correction() }
        ])
        return button
    }
    func updateUIView(_ button: ReaderMenuButton, context: Context) { context.coordinator.parent = self }
}
final class ReaderMenuButton: UIButton {
    var visibility: ((Bool) -> Void)?
    override func contextMenuInteraction(_ interaction: UIContextMenuInteraction, willDisplayMenuFor configuration: UIContextMenuConfiguration, animator: (any UIContextMenuInteractionAnimating)?) {
        super.contextMenuInteraction(interaction, willDisplayMenuFor: configuration, animator: animator)
        visibility?(true)
    }
    override func contextMenuInteraction(_ interaction: UIContextMenuInteraction, willEndFor configuration: UIContextMenuConfiguration, animator: (any UIContextMenuInteractionAnimating)?) {
        super.contextMenuInteraction(interaction, willEndFor: configuration, animator: animator)
        if let animator { animator.addCompletion { [weak self] in self?.visibility?(false) } }
        else { visibility?(false) }
    }
}
struct ReaderHighlight: Decodable, Identifiable {
    let id: String; let chapter: Int; let start_block: Int; let start_offset: Int; let end_block: Int; let end_offset: Int; let color: String
    var start: ReadingAnchor { .init(chapter: chapter, block: start_block, charOffset: start_offset) }
    var end: ReadingAnchor { .init(chapter: chapter, block: end_block, charOffset: end_offset) }
    var tint: UIColor { switch color { case "Green": return .systemGreen; case "Blue": return .systemBlue; case "Pink": return .systemPink; default: return .systemYellow } }
}
struct ReaderNote: Decodable { let highlight_id: String; let content: String }
struct ReaderBookmark: Decodable, Identifiable { let chapter: Int; let block: Int; var id: String { "\(chapter)-\(block)" } }
struct ReaderCorrection: Decodable { let chapter: Int; let block_idx: Int; let char_offset: Int; let original: String; let corrected: String; let status: String }
struct ReaderAnnotations: Decodable {
    var highlights: [ReaderHighlight]; var notes: [ReaderNote]; var bookmarks: [ReaderBookmark]; var corrections: [ReaderCorrection]?
    static var empty: Self { .init(highlights: [], notes: [], bookmarks: [], corrections: []) }
}
struct SelectedPassage: Identifiable { let id = UUID(); let start: ReadingAnchor; let end: ReadingAnchor; let text: String }
struct NativeDictionary: UIViewControllerRepresentable {
    let term: String
    func makeUIViewController(context: Context) -> UIReferenceLibraryViewController { UIReferenceLibraryViewController(term: term) }
    func updateUIViewController(_ controller: UIReferenceLibraryViewController, context: Context) {}
}
@MainActor final class ReadingAnnotations: ObservableObject {
    @Published var value = ReaderAnnotations.empty
    @Published var revision = 0
    @Published var busy = false
    let library: PreviewLibrary
    let book: PreviewBook
    init(library: PreviewLibrary, book: PreviewBook) { self.library = library; self.book = book }
    @discardableResult func perform(_ fields: [String: Any] = [:]) async -> Bool {
        guard !busy else { return false }; busy = true; defer { busy = false }
        do { value = try await library.store.annotations(id: book.id, fields: fields); revision += 1; return true }
        catch { library.fail(error, title: "无法完成操作"); return false }
    }
    @discardableResult func highlight(_ selected: SelectedPassage, color: String, note: String = "") async -> Bool {
        return await perform(["action": "highlight", "chapter": selected.start.chapter, "start_block": selected.start.block, "start_offset": selected.start.charOffset, "end_block": selected.end.block, "end_offset": selected.end.charOffset, "color": color, "note": note])
    }
    func excerpt(_ highlight: ReaderHighlight) -> String {
        guard book.chapters.indices.contains(highlight.chapter) else { return "" }
        return book.chapters[highlight.chapter].blocks.filter { $0.type == "Paragraph" || $0.type == "Heading" }.filter { ($0.core_block ?? 0) >= highlight.start_block && ($0.core_block ?? 0) <= highlight.end_block }.map { block in
            let text = (block.spans ?? []).map(\.text).joined(); let scalars = Array(text.unicodeScalars)
            let begin = (block.core_block ?? 0) == highlight.start_block ? highlight.start_offset : 0
            let end = (block.core_block ?? 0) == highlight.end_block ? highlight.end_offset : scalars.count
            return String(String.UnicodeScalarView(scalars.dropFirst(max(0,begin)).prefix(max(0,end-begin))))
        }.joined(separator: "\n")
    }
}
struct AnnotationsView: View {
    @ObservedObject var annotations: ReadingAnnotations
    let current: ReadingAnchor
    let jump: (ReadingAnchor) -> Void
    var body: some View {
        List {
            Section("书签") {
                Button("添加书签", systemImage: "bookmark") { Task { await annotations.perform(["action":"bookmark", "chapter":current.chapter, "block":current.block]) } }.disabled(annotations.busy)
                ForEach(annotations.value.bookmarks) { bookmark in
                    Button("第 \(bookmark.chapter+1) 章") { jump(.init(chapter: bookmark.chapter, block: bookmark.block)) }
                        .swipeActions { Button("删除", role: .destructive) { Task { await annotations.perform(["action":"bookmark", "chapter":bookmark.chapter,"block":bookmark.block]) } } }
                }
            }
            Section("高亮与笔记") {
                if annotations.value.highlights.isEmpty { Text("长按正文选择文字，可查词、高亮或添加笔记。").foregroundStyle(.secondary) }
                ForEach(annotations.value.highlights) { item in
                    Button { jump(item.start) } label: {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(annotations.excerpt(item)).lineLimit(3).foregroundStyle(.primary)
                            if let note = annotations.value.notes.first(where: { $0.highlight_id == item.id }) { Text(note.content).font(.subheadline).foregroundStyle(.secondary) }
                            Text("第 \(item.chapter+1) 章").font(.caption).foregroundStyle(.secondary)
                        }.padding(.leading, 8).overlay(alignment: .leading) { Rectangle().fill(Color(uiColor: item.tint)).frame(width: 3) }
                    }.swipeActions { Button("删除高亮", role: .destructive) { Task { await annotations.perform(["action":"delete", "highlight_id":item.id]) } } }
                }
            }
        }.navigationTitle("批注").navigationBarTitleDisplayMode(.inline)
    }
}
struct SelectedTextView: View {
    let passage: SelectedPassage
    @ObservedObject var annotations: ReadingAnnotations
    @Environment(\.dismiss) private var dismiss
    @State private var note = ""
    @State private var dictionary = false
    var body: some View {
        NavigationStack {
            Form {
                Section { Text(passage.text).textSelection(.enabled) }
                Section {
                    Button("复制", systemImage: "doc.on.doc") { UIPasteboard.general.string = passage.text; dismiss() }
                    Button("查词", systemImage: "character.book.closed") { dictionary = true }
                }
                Section("高亮") {
                    HStack {
                        ForEach([("Yellow","黄色"),("Green","绿色"),("Blue","蓝色"),("Pink","粉色")], id: \.0) { item in
                            Button { Task { if await annotations.highlight(passage, color: item.0) { dismiss() } } } label: {
                                VStack(spacing: 6) {
                                    Circle().fill(highlightColor(item.0)).frame(width: 22, height: 22)
                                    Text(item.1).font(.caption).foregroundStyle(.primary)
                                }.frame(maxWidth: .infinity, minHeight: 48)
                            }.buttonStyle(.plain)
                        }
                    }.disabled(annotations.busy)
                }
                Section("笔记") {
                    TextEditor(text: $note).frame(minHeight: 100)
                    Button("保存笔记") { Task { if await annotations.highlight(passage, color: "Yellow", note: note) { dismiss() } } }.disabled(note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || annotations.busy)
                }
            }.navigationTitle("选中文字").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("取消选择") { dismiss() } } }
                .sheet(isPresented: $dictionary) { NativeDictionary(term: passage.text) }
        }
    }
    private func highlightColor(_ color: String) -> Color {
        switch color { case "Green": return .green; case "Blue": return .blue; case "Pink": return .pink; default: return .yellow }
    }
}
struct CorrectionView: View {
    @ObservedObject var model: ChineseCorrection
    @ObservedObject var annotations: ReadingAnnotations
    let chapter: Int
    var body: some View {
        Form {
            Section {
                LabeledContent("MacBERT", value: model.ready ? "模型已就绪" : model.available ? "未加载" : "未下载")
                if model.busy { HStack { ProgressView(); Text("正在处理…") }; Button("取消") { model.cancel() } }
                else if !model.ready { Button(model.available ? "加载模型" : "下载模型") { model.prepare(download: !model.available) } }
                else { Button("检查当前章节") { model.check(chapter: annotations.book.chapters[chapter]) } }
                if model.available && !model.busy { Button("删除模型", role: .destructive) { model.remove() } }
                Picker("纠错强度", selection: $model.threshold) { Text("保守").tag(0.97); Text("标准").tag(0.90); Text("积极").tag(0.80) }.disabled(model.busy)
                if let error = model.error { Text(error).foregroundStyle(.red) }
            } footer: { Text("首次使用需下载模型和词表（约 100 MB），下载后可离线纠错。纠错建议可能有误，原始 EPUB 不会被改写。") }
            Section("纠错建议") {
                ForEach(model.suggestions) { suggestion in
                    VStack(alignment: .leading, spacing: 8) {
                        Text("\(suggestion.original) → \(suggestion.corrected)").font(.headline)
                        Text(String(format: "%.0f%%", suggestion.confidence*100)).font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button("接受") { Task { if await annotations.perform(["action":"correction","chapter":chapter,"block":suggestion.block,"char_offset":suggestion.offset,"original":suggestion.original,"corrected":suggestion.corrected]) { model.suggestions.removeAll { $0.id == suggestion.id } } } }.disabled(annotations.busy)
                            Button("忽略") { model.suggestions.removeAll { $0.id == suggestion.id } }
                        }
                    }
                }
            }
        }.navigationTitle("中文纠错").navigationBarTitleDisplayMode(.inline)
    }
}
