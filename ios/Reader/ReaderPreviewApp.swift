import SwiftUI
import UniformTypeIdentifiers

@main struct ReaderPreviewApp: App {
    init() {
        #if DEBUG
        if CommandLine.arguments.contains("--verify-keychain") { KeychainRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-model") { ModelRuntimeCheck.run() }
        #endif
    }
    var body: some Scene { WindowGroup { AppHome() } }
}

struct AppHome: View {
    @StateObject private var library: PreviewLibrary
    @StateObject private var account: GitHubAccount
    @StateObject private var nearby: NearbySync
    @Environment(\.scenePhase) private var scenePhase
    init() {
        let library = PreviewLibrary()
        _library = StateObject(wrappedValue: library)
        _account = StateObject(wrappedValue: GitHubAccount(library: library))
        _nearby = StateObject(wrappedValue: NearbySync(library: library))
    }
    @State private var destination: Int? = 0
    @State private var selectedBook: PreviewBook?
    @State private var openingBook: LibraryEntry?
    @StateObject private var releases = ReleaseNotice()
    @Environment(\.horizontalSizeClass) private var sizeClass
    var body: some View {
        Group {
            if sizeClass == .regular {
                NavigationSplitView {
                    List(selection: $destination) {
                        Label("书库", systemImage: "books.vertical").tag(0)
                        Label("多平台同步", systemImage: "arrow.triangle.2.circlepath").tag(1)
                    }.navigationTitle("EPUB Reader")
                } detail: {
                    NavigationStack {
                        if destination == 1 { SyncDesignView(account: account, nearby: nearby) }
                        else { LibraryDesignView(library: library, account: account, releases: releases, open: openBook) }
                    }
                }
            } else {
                TabView {
                    NavigationStack { LibraryDesignView(library: library, account: account, releases: releases, open: openBook) }
                        .tabItem { Label("书库", systemImage: "books.vertical") }
                    NavigationStack { SyncDesignView(account: account, nearby: nearby) }
                        .tabItem { Label("多平台同步", systemImage: "arrow.triangle.2.circlepath") }
                }
            }
        }
        .overlay { if let openingBook { BookOpeningView(book: openingBook) } }
        .tint(Color(red: 0.18, green: 0.39, blue: 0.75))
        .fullScreenCover(item: $selectedBook, onDismiss: { library.closed() }) { book in ReadingWorkspace(book: book, library: library) }
        .onOpenURL { url in Task { await library.importBook(url) } }
        .task { account.setForeground(scenePhase == .active); await releases.check() }
        .onChange(of: scenePhase) { _, phase in account.setForeground(phase == .active); if phase != .active { nearby.stop() }; if phase == .active { Task { await releases.check() } } }
        .alert(library.errorTitle, isPresented: Binding(get: { library.error != nil && selectedBook == nil }, set: { if !$0 { library.error = nil } })) {
            Button("完成", role: .cancel) { library.error = nil }
        } message: { Text(library.error ?? "") }
    }
    private func openBook(_ entry: LibraryEntry) {
        guard openingBook == nil && !library.busy else { return }
        openingBook = entry
        Task { selectedBook = await library.open(entry); openingBook = nil }
    }
}

struct BookCover: View {
    let book: LibraryEntry
    var compact = false
    var body: some View {
        GeometryReader { geometry in
        ZStack(alignment: .topLeading) {
            if let path = book.coverPath, let image = UIImage(contentsOfFile: path) {
                Image(uiImage: image).resizable().scaledToFill().frame(width: geometry.size.width, height: geometry.size.height).clipped()
            } else {
            book.color
            Rectangle().fill(.white.opacity(0.12)).frame(width: 6)
            VStack(alignment: .leading, spacing: 12) {
                Rectangle().fill(.white.opacity(0.55)).frame(height: 1)
                Text(book.title).font(compact ? .system(size: 11, weight: .medium) : .system(.title3, design: .serif)).fontWeight(.medium)
                Spacer(minLength: 0)
                if !compact { Text("EPUB").font(.caption2).foregroundStyle(.white.opacity(0.7)) }
            }.padding(compact ? 8 : 16).foregroundStyle(.white)
            }
        }.frame(width: geometry.size.width, height: geometry.size.height).clipShape(RoundedRectangle(cornerRadius: 6))
        }
    }
}

struct LibraryDesignView: View {
    @ObservedObject var library: PreviewLibrary
    @ObservedObject var account: GitHubAccount
    @ObservedObject var releases: ReleaseNotice
    let open: (LibraryEntry) -> Void
    @State private var search = ""
    @State private var titleCollapsed = false
    @State private var sortByTitle = false
    @State private var about = false
    @State private var importing = false
    @State private var managing = false
    @State private var selected = Set<String>()
    @State private var confirmingDelete = false
    @State private var exporting = false
    private var books: [LibraryEntry] {
        let filtered = library.recentBooks.filter { search.isEmpty || $0.title.localizedCaseInsensitiveContains(search) }
        return sortByTitle ? filtered.sorted { $0.title.localizedCompare($1.title) == .orderedAscending } : filtered
    }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                if !managing && search.isEmpty, let book = library.books.first(where: { $0.id == library.recent }) {
                    Button { open(book) } label: {
                        HStack(spacing: 16) {
                            BookCover(book: book, compact: true).frame(width: 62, height: 88)
                            VStack(alignment: .leading, spacing: 6) {
                                Text("继续阅读").font(.caption).foregroundStyle(.secondary)
                                Text(book.title).font(.headline).foregroundStyle(.primary)
                                Text("第 \((library.anchors[book.id]?.chapter ?? 0)+1) 章").font(.subheadline).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Image(systemName: "chevron.right").font(.subheadline).foregroundStyle(.secondary)
                        }.padding(16).background(.background, in: RoundedRectangle(cornerRadius: 12))
                    }.buttonStyle(.plain)
                }
                VStack(alignment: .leading, spacing: 16) {
                    if managing {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("已选 \(selected.count) 本").font(.headline)
                            HStack {
                                Button("全选当前列表") { selected.formUnion(books.map(\.id)) }
                                Button("清除选择") { selected.removeAll() }
                            }.font(.subheadline)
                            HStack {
                                Button("导出所选", systemImage: "square.and.arrow.up") { exporting = true }
                                Button("删除所选", systemImage: "trash", role: .destructive) { confirmingDelete = true }
                            }.disabled(selected.isEmpty || library.busy).buttonStyle(.bordered)
                        }
                    } else { Text("全部书籍").font(.headline) }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 145, maximum: 220), spacing: 20)], alignment: .leading, spacing: 24) {
                        ForEach(books) { book in
                            Button { if managing { if !selected.insert(book.id).inserted { selected.remove(book.id) } } else { open(book) } } label: {
                                VStack(alignment: .leading, spacing: 8) {
                                    BookCover(book: book).aspectRatio(2/3, contentMode: .fit)
                                        .overlay(alignment: .topTrailing) { if managing { Image(systemName: selected.contains(book.id) ? "checkmark.circle.fill" : "circle").font(.title2).symbolRenderingMode(.palette).foregroundStyle(.white, .blue).padding(8).background(.regularMaterial, in: Circle()).padding(6) } }
                                    Text(book.title).font(.subheadline).fontWeight(.medium).foregroundStyle(.primary).lineLimit(2)
                                    Text("第 \((library.anchors[book.id]?.chapter ?? 0)+1) 章").font(.caption).foregroundStyle(.secondary)
                                }
                            }.buttonStyle(.plain)
                        }
                    }
                }
                if library.books.isEmpty && !library.busy {
                    ContentUnavailableView {
                        Label("还没有书籍", systemImage: "books.vertical")
                    } description: { Text("导入 EPUB，开始阅读。") } actions: {
                        Button("导入 EPUB") { importing = true }.buttonStyle(.borderedProminent)
                    }
                }
                if library.busy { ProgressView().frame(maxWidth: .infinity).accessibilityLabel("正在处理书籍") }
            }.padding(20)
                .background {
                    GeometryReader { geometry in
                        Color.clear.preference(key: LibraryTitleCollapsedPreference.self,
                            value: geometry.frame(in: .named("library-scroll")).minY < -UIFont.preferredFont(forTextStyle: .largeTitle).lineHeight)
                    }
                }
        }
        .coordinateSpace(name: "library-scroll")
        .onPreferenceChange(LibraryTitleCollapsedPreference.self) { titleCollapsed = $0 }
        .background(Color(uiColor: .systemGroupedBackground))
        .contentMargins(.bottom, 80, for: .scrollContent)
        .navigationTitle("书库")
        .safeAreaInset(edge: .bottom) { if !managing, let version = releases.newest { Link("发现新版本 \(version) · 查看发行说明", destination: URL(string: "https://github.com/zhongbai2333/RustEpubReader/releases/latest")!).font(.footnote).padding(12).frame(maxWidth: .infinity).background(.regularMaterial) } }
        .searchable(text: $search, prompt: "搜索书名")
        .toolbar {
            if #available(iOS 26.0, *) {
                ToolbarItem(placement: .largeTitle) {
                    HStack(spacing: 12) {
                        Text("书库").font(.largeTitle.bold())
                        if account.syncing { syncIndicator }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }
                if titleCollapsed {
                    ToolbarItem(placement: .title) {
                        HStack(spacing: 8) {
                            Text("书库").font(.headline)
                            if account.syncing { syncIndicator }
                        }
                    }
                }
            } else if account.syncing {
                ToolbarItem(placement: .principal) {
                    HStack(spacing: 8) {
                        Text("书库").font(.headline)
                        syncIndicator
                    }
                }
            }
            ToolbarItem(placement: .topBarLeading) {
                Button(managing ? "完成" : "管理书籍") { managing.toggle(); if !managing { selected.removeAll() } }.disabled(library.books.isEmpty || library.busy)
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("导入 EPUB", systemImage: "plus") { importing = true }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.circle)
                    .disabled(library.busy)
            }
            if #available(iOS 26.0, *) {
                ToolbarSpacer(.fixed, placement: .topBarTrailing)
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Picker("排序", selection: $sortByTitle) {
                        Text("最近阅读").tag(false)
                        Text("书名").tag(true)
                    }
                    Button("关于", systemImage: "info.circle") { about = true }
                } label: { Image(systemName: "ellipsis.circle") }.accessibilityLabel("更多操作")
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [UTType(filenameExtension: "epub") ?? .data]) { result in
            switch result {
            case .success(let url): Task { await library.importBook(url) }
            case .failure(let error): library.fail(error, title: "无法导入书籍")
            }
        }
        .sheet(isPresented: $about) { AboutView(releases: releases, count: library.books.count) }
        .sheet(isPresented: $exporting) { ShareFiles(urls: library.books.filter { selected.contains($0.id) }.map { URL(fileURLWithPath: $0.path) }) }
        .confirmationDialog("删除所选书籍？", isPresented: $confirmingDelete, titleVisibility: .visible) {
            Button("删除所选", role: .destructive) { let ids = selected; Task { if await library.remove(ids) { selected.removeAll() } } }
            Button("取消", role: .cancel) {}
        } message: { Text("将删除所选书籍及其本地阅读进度和批注，不会删除已同步到 GitHub 仓库的副本。") }
        .onChange(of: library.books.map(\.id)) { _, ids in selected.formIntersection(ids) }
    }
    private var syncIndicator: some View {
        HStack(spacing: 6) {
            ProgressView().controlSize(.small).tint(.secondary)
            Text("同步中…").font(.caption).foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("正在同步")
    }
}


private struct LibraryTitleCollapsedPreference: PreferenceKey {
    static var defaultValue = false
    static func reduce(value: inout Bool, nextValue: () -> Bool) { value = nextValue() }
}
