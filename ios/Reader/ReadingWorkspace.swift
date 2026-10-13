import SwiftUI
import UIKit

struct ReaderOptions: Equatable {
    var fontSize: Double
    var lineHeight: Double
    var fontName: String
    var theme: String
    var sliding: Bool
    var paper: UIColor { theme == "dark" ? UIColor(red: 0.10, green: 0.10, blue: 0.11, alpha: 1) : theme == "light" ? .systemBackground : UIColor(red: 0.97, green: 0.95, blue: 0.91, alpha: 1) }
    var ink: UIColor { theme == "dark" ? UIColor(white: 0.86, alpha: 1) : .label }
}

struct ReadingWorkspace: View {
    let book: PreviewBook
    @ObservedObject var library: PreviewLibrary
    @StateObject private var annotations: ReadingAnnotations
    @StateObject private var model: ChineseCorrection
    @State private var passage: SelectedPassage?
    @State private var prepared = false
    init(book: PreviewBook, library: PreviewLibrary) {
        self.book = book; self.library = library
        _annotations = StateObject(wrappedValue: ReadingAnnotations(library: library, book: book))
        _model = StateObject(wrappedValue: ChineseCorrection(root: library.client.root))
    }
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage("preview.fontSize") private var fontSize = 18.0
    @AppStorage("preview.lineHeight") private var lineHeight = 1.65
    @AppStorage("preview.fontName") private var fontName = "PingFangSC-Regular"
    @AppStorage("preview.theme") private var theme = "paper"
    @AppStorage("preview.sliding") private var sliding = false
    @State private var tools = true
    @State private var toolsMenuOpen = false
    @State private var panel: String?
    @State private var current = ReadingAnchor(chapter: 0)
    @State private var position = ""
    @State private var jump: ReadingAnchor?
    @State private var jumpingID = 0
    @State private var hideTask: Task<Void, Never>?
    private var options: ReaderOptions { .init(fontSize: fontSize, lineHeight: lineHeight, fontName: fontName, theme: theme, sliding: sliding || reduceMotion) }
    var body: some View {
        ZStack {
            Color(uiColor: options.paper).ignoresSafeArea()
            NativeReader(book: book, initial: library.anchors[book.id] ?? .init(chapter: 0), options: options, annotations: annotations.value, annotationRevision: annotations.revision, selected: { passage = $0; hideTask?.cancel() }, jump: jump, jumpID: jumpingID,
                         toggleTools: { withAnimation(.easeOut(duration: reduceMotion ? 0 : 0.18)) { tools.toggle() }; scheduleHide() },
                         turning: { hideTask?.cancel(); withAnimation(.easeOut(duration: reduceMotion ? 0 : 0.12)) { tools = false } },
                         visible: { anchor, label in current = anchor; position = label; prepared = true },
                         committed: { library.save($0, for: book) })
                .ignoresSafeArea()
            VStack {
                if tools {
                    HStack(spacing: 16) {
                        Button { dismiss() } label: { Image(systemName: "chevron.left").frame(width: 44, height: 44) }.accessibilityLabel("书库")
                        VStack(alignment: .leading, spacing: 2) {
                            Text(book.title).font(.subheadline).fontWeight(.semibold).lineLimit(1)
                            Text("第 \(current.chapter+1) 章").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 8)
                        Button { panel = "toc"; hideTask?.cancel() } label: { Image(systemName: "list.bullet").frame(width: 44, height: 44) }.accessibilityLabel("目录")
                        ReadingToolsMenu(visibility: { opened in
                            toolsMenuOpen = opened
                            if opened { hideTask?.cancel() } else { scheduleHide() }
                        }, annotations: { panel = "annotations"; hideTask?.cancel() },
                        correction: { model.suggestions = []; panel = "correction"; hideTask?.cancel() })
                            .frame(width: 36, height: 44)
                        Button { panel = "settings"; hideTask?.cancel() } label: { Image(systemName: "slider.horizontal.3").frame(width: 44, height: 44) }.accessibilityLabel("阅读设置")
                    }.padding(.horizontal, 8).background(.regularMaterial).transition(.opacity)
                }
                Spacer()
                Text(position).font(.caption).monospacedDigit().foregroundStyle(Color(uiColor: options.ink).opacity(0.5)).padding(.bottom, 8).allowsHitTesting(false)
            }
        }
        .overlay { if !prepared { BookOpeningView(book: book.entry) } }
        .sheet(item: $passage, onDismiss: { scheduleHide() }) { SelectedTextView(passage: $0, annotations: annotations) }
        .task { await annotations.perform() }
        .preferredColorScheme(theme == "dark" ? .dark : .light)
        .statusBarHidden(!tools)
        .persistentSystemOverlays(tools ? .visible : .hidden)
        .sheet(item: Binding(get: { panel.map(ReaderPanel.init) }, set: { panel = $0?.id })) { selected in
            NavigationStack {
                Group {
                if selected.id == "toc" {
                    List(Array(book.toc.enumerated()), id: \.offset) { _, item in
                        let chapter = item.chapter_index
                        Button {
                            jump = .init(chapter: chapter); jumpingID += 1; library.save(jump!, for: book); panel = nil
                        } label: {
                            HStack {
                                Text(item.title).foregroundStyle(.primary)
                                Spacer()
                                if chapter == current.chapter { Image(systemName: "checkmark").foregroundStyle(.tint) }
                            }.frame(minHeight: 28)
                        }.buttonStyle(.plain)
                    }.navigationTitle("目录").navigationBarTitleDisplayMode(.inline)
                } else if selected.id == "annotations" {
                    AnnotationsView(annotations: annotations, current: current, jump: { jump = $0; jumpingID += 1; library.save($0, for: book); panel = nil })
                } else if selected.id == "correction" {
                    CorrectionView(model: model, annotations: annotations, chapter: current.chapter)
                } else {
                    Form {
                        Section("排版") {
                            LabeledContent("字号", value: "\(Int(fontSize))")
                            Slider(value: $fontSize, in: 14...28, step: 1).accessibilityLabel("字号")
                            LabeledContent("行距", value: String(format: "%.2f", lineHeight))
                            Slider(value: $lineHeight, in: 1.3...2.0, step: 0.05).accessibilityLabel("行距")
                            Picker("字体", selection: $fontName) {
                                Text("苹方简体").tag("PingFangSC-Regular")
                                if UIFont(name: "SongtiSC-Regular", size: 18) != nil { Text("宋体").tag("SongtiSC-Regular") }
                            }
                        }
                        Section("阅读背景") {
                            Picker("阅读背景", selection: $theme) { Text("纸色").tag("paper"); Text("浅色").tag("light"); Text("深色").tag("dark") }.pickerStyle(.segmented)
                        }
                        Section("翻页方式") {
                            Picker("翻页方式", selection: $sliding) { Text("拟真").tag(false); Text("滑动").tag(true) }.pickerStyle(.segmented)
                        }
                    }.navigationTitle("阅读设置").navigationBarTitleDisplayMode(.inline)
                }
                }
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { panel = nil } } }
            }
            .presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
        }
        .onAppear { scheduleHide() }
        .onChange(of: panel) { _, new in if new == nil { scheduleHide() } }
        .onDisappear { hideTask?.cancel(); model.cancel() }
        .alert("检测到新的阅读位置", isPresented: Binding(get: { library.offer != nil }, set: { if !$0 { library.offer = nil } })) {
            Button("跳转") {
                if let offer = library.offer { jump = offer.anchor; jumpingID += 1; library.save(offer.anchor, for: book) }
                library.offer = nil
            }
            Button("稍后", role: .cancel) { library.offer = nil }
        } message: { Text(library.offer?.chapter_title ?? "第 \(1 + (library.offer?.chapter ?? 0)) 章") }
        .alert(library.errorTitle, isPresented: Binding(get: { library.error != nil }, set: { if !$0 { library.error = nil } })) {
            Button("完成", role: .cancel) { library.error = nil }
        } message: { Text(library.error ?? "") }
    }
    private func scheduleHide() {
        hideTask?.cancel()
        guard tools && !toolsMenuOpen && panel == nil && passage == nil else { return }
        hideTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled && !toolsMenuOpen && panel == nil && passage == nil else { return }
            withAnimation(.easeOut(duration: reduceMotion ? 0 : 0.18)) { tools = false }
        }
    }
}
struct ReaderPanel: Identifiable { var id: String }
