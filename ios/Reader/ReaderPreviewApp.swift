import SwiftUI
import UniformTypeIdentifiers

@main struct ReaderPreviewApp: App {
    init() {
        ReaderFonts.shared.restoreSystemFonts()
        #if READER_PERFORMANCE_CHECK
        if CommandLine.arguments.contains("--measure-release-book") {ReadingReleasePerformance.run()}
        #endif
        #if DEBUG
        if CommandLine.arguments.contains("--verify-appearance") { ReadingAppearanceRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-book-share") { BookShareRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-i18n") {ReaderI18n.verifyBundled()}
        if CommandLine.arguments.contains("--verify-native") { NativePresentationRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-keychain") { KeychainRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-illustrations") { IllustrationRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-chapter-loading") || CommandLine.arguments.contains("--verify-reading-modes") || CommandLine.arguments.contains("--verify-page-footers") { ReadingModesRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-model") { ModelRuntimeCheck.run() }
        #endif
    }
    var body: some Scene { WindowGroup { AppHome().task {
        #if DEBUG
        if CommandLine.arguments.contains("--verify-fonts") { await FontsRuntimeCheck.run() }
        if CommandLine.arguments.contains("--verify-speech") { await SpeechRuntimeCheck.run() }
        #endif
    } } }
}

struct AppHome: View {
    @StateObject private var appearanceLink: AppearanceCoordinator = {
        #if DEBUG
        if CommandLine.arguments.contains("--verify-ambient") {
            return AppearanceCoordinator(reader: AmbientAppearanceRuntimeCheck.sensor)
        }
        #endif
        return AppearanceCoordinator()
    }()
    @ObservedObject private var readerI18n = ReaderI18n.shared
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
    @State private var libraryQuery = ""
    @State private var librarySearching = false
    @AppStorage("library.sort-title") private var shelfSortTitle = false
    @AppStorage("library.list") private var shelfListMode = false
    @State private var sharedShelfOrder = StableShelfOrder()
    @State private var selectedBook: PreviewBook?
    @State private var openingBook: LibraryEntry?
    @State private var coverFrames: [String: CGRect] = [:]
    @State private var sourceCover: CGRect?
    @State private var sourceKey = ""
    @State private var expanded = false
    @State private var paperProgress: CGFloat = 0
    @State private var readerMounted = false
    @State private var paperOpacity = 1.0
    @State private var openingExpanded = false
    @State private var readerReady = false
    @State private var closing = false
    @State private var shelfRevision = 0
    @State private var transitionToken = UUID()
    #if DEBUG
    @State private var verifyingBookPresentation = false
    #endif
    @State private var paper = ReaderOptions.savedPaper
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @StateObject private var releases = ReleaseNotice()
    @Environment(\.horizontalSizeClass) private var sizeClass
    var body: some View {
        ZStack {
        Group {
            if sizeClass == .regular {
                NavigationSplitView {
                    List(selection: $destination) {
                        Label(L("toolbar.library"), systemImage: "books.vertical").tag(0)
                        Label(L("sync.multiplatform"), systemImage: "arrow.triangle.2.circlepath").tag(1)
                    }.navigationTitle("EPUB Reader")
                } detail: {
                    NavigationStack {
                        if destination == 1 { SyncDesignView(account: account, nearby: nearby) }
                        else { LibraryDesignView(library: library, account: account, releases: releases, shelfRevision: shelfRevision, search: $libraryQuery, sortByTitle: $shelfSortTitle, listMode: $shelfListMode, shelfOrder: sharedShelfOrder, bottomSearch: false, open: openBook, openSync: { destination = 1 }) }
                    }
                }
            } else if #available(iOS 26.0, *) {
                modernHome
            } else {
                ZStack {
                    NavigationStack {
                        LibraryDesignView(library: library, account: account, releases: releases,
                            shelfRevision: shelfRevision, search: $libraryQuery, sortByTitle: $shelfSortTitle, listMode: $shelfListMode, shelfOrder: sharedShelfOrder, bottomSearch: true,
                            open: openBook, openSync: { destination = 1 })
                    }.opacity(destination == 1 ? 0 : 1)
                     .allowsHitTesting(destination != 1).accessibilityHidden(destination == 1)
                    NavigationStack { SyncDesignView(account: account, nearby: nearby) }
                        .opacity(destination == 1 ? 1 : 0)
                        .allowsHitTesting(destination == 1).accessibilityHidden(destination != 1)
                }.safeAreaInset(edge: .bottom, spacing: 0) {
                    LibraryBottomBar(selection: Binding(get: { destination ?? 0 }, set: { destination = $0 }),
                        query: $libraryQuery, searching: $librarySearching,
                        readerActive: openingBook != nil || selectedBook != nil || closing)
                }

            }
        }
        .allowsHitTesting(openingBook == nil && selectedBook == nil && !closing)
        .accessibilityHidden(selectedBook != nil || openingBook != nil || closing)
        if let book = selectedBook, readerMounted {
            ReadingWorkspace(book: book, library: library,
                openSync: { closeBook(after: { destination = 1 }) },
                close: { closeBook() }, ready: { [token=transitionToken] in
                    if transitionToken == token && !closing {readerDidPrepare()}
                },
                paperChanged: { paper = $0 }, contentVisible: openingExpanded || (openingBook == nil && !closing))
                .opacity(openingExpanded || (openingBook == nil && !closing) ? 1 : 0)
                .allowsHitTesting(openingBook == nil && !closing)
                .accessibilityHidden(openingBook != nil || closing)
        }
        if openingBook != nil || closing {
            BookPaperTransition(source: reduceMotion ? nil : sourceCover,
                expanded: expanded, progress: paperProgress, closing: closing, opacity: paperOpacity, paper: paper,
                waiting: !readerReady && !closing,
                cancel: { closeBook() }, settled: { [token=transitionToken] progress,opacity in
                    if transitionToken == token {paperDidSettle(progress,opacity)}
                })
                .ignoresSafeArea().zIndex(2)
        }
        }
        .onPreferenceChange(BookCoverFrames.self) { coverFrames = $0 }
        .environmentObject(appearanceLink)
        .environment(\.colorScheme, appearanceLink.appDark ? .dark : .light)
        .preferredColorScheme(appearanceLink.appDark ? .dark : .light)
        .task(id: scenePhase) {
            appearanceLink.setForeground(scenePhase == .active)
            guard scenePhase == .active else { return }
            while !Task.isCancelled {
                appearanceLink.refreshSystemAppearance()
                do { try await Task.sleep(for: .seconds(1)) } catch { break }
            }
        }
        .environment(\.locale, readerI18n.locale)
        .tint(Color(red: 0.18, green: 0.39, blue: 0.75))
        .onChange(of: library.books.map(\.id)) { _, ids in
            if let book=selectedBook, !ids.contains(book.id) { closeBook() }
        }
        .onOpenURL { url in
            if url.scheme == "rustepubreader" {
                library.receivingSharedBooks = true; library.bookSharing.receive(url.absoluteString)
            } else { Task { await library.importBook(url) } }
        }
        .sheet(isPresented: $library.receivingSharedBooks, onDismiss: { library.bookSharing.cancel() }) { ReceiveBooksView(sharing: library.bookSharing, account: account) }
        .task {
            readerI18n.refresh(); account.setForeground(scenePhase == .active)
            #if DEBUG
            if CommandLine.arguments.contains(where: { $0.hasPrefix("--verify-") }) {
                UserDefaults.standard.set(true, forKey: ReadingIntroduction.storageKey)
            }
            if CommandLine.arguments.contains("--verify-book-presentation") { Task { await verifyBookPresentation() } }
            if CommandLine.arguments.contains("--reader-introduction-preview"), Bundle.main.bundleIdentifier == "local.reader.firstuse" {
                UserDefaults.standard.set(false, forKey: ReadingIntroduction.storageKey)
            }
            if CommandLine.arguments.contains("--appearance-reader") {
                let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("appearance.epub")
                await library.importBook(url)
                if let entry = library.recentBooks.first { openBook(entry, "appearance-runtime") }
            }
            if CommandLine.arguments.contains("--verify-linkage") { await AppearanceLinkageRuntimeCheck.run(appearanceLink,close:{closeBook()}) }
            if CommandLine.arguments.contains("--verify-ambient") || CommandLine.arguments.contains("--verify-ambient-device") {
                await AmbientAppearanceRuntimeCheck.run(appearanceLink, physical: CommandLine.arguments.contains("--verify-ambient-device"))
            }
            #endif
            await releases.check()
        }
        .onChange(of: scenePhase) { _, phase in account.setForeground(phase == .active); if phase != .active { nearby.stop() }; if phase == .background { library.bookSharing.cancel() }; if phase == .active { readerI18n.refresh(); Task { await releases.check() } } }
        .alert(LM(library.errorTitle), isPresented: Binding(get: { library.error != nil && selectedBook == nil }, set: { if !$0 { library.error = nil } })) {
            Button(L("manage.done"), role: .cancel) { library.error = nil }
        } message: { Text(LM(library.error ?? "")) }
    }
    #if DEBUG
    // Exercise the actual presentation host without stealing another simulator's input.
    @MainActor private func verifyBookPresentation() async {
        guard !verifyingBookPresentation else { return }
        verifyingBookPresentation = true
        var checks: [[String: Any]] = []
        for _ in 0..<100 {
            if !library.busy && !library.recentBooks.isEmpty && !coverFrames.isEmpty { break }
            try? await Task.sleep(for: .milliseconds(50))
        }
        // Cancel a live expansion before reopening: stale animation/layout
        // callbacks must not release the next book's visibility gate.
        var cancellationPassed=false
        if let entry=library.recentBooks.first {
            openBook(entry,"shelf:"+entry.id)
            try? await Task.sleep(for:.milliseconds(180))
            closeBook()
            for _ in 0..<100 {
                if !closing {break}
                try? await Task.sleep(for:.milliseconds(16))
            }
            cancellationPassed=selectedBook == nil && openingBook == nil && !closing
        }
        for index in 0..<4 {
            let visible=coverFrames.filter {$0.key.hasPrefix("shelf:") && $0.value.width>0 && $0.value.minY>=0 && $0.value.maxY<1000}
                .sorted {abs($0.value.minY-$1.value.minY)<1 ? $0.value.minX<$1.value.minX : $0.value.minY<$1.value.minY}
            // Open the second visible cover: opening only the first misses the
            // recent-sort regression where a right cover moved into the left slot.
            let target=(index == 2 ? visible.dropFirst(3).first : visible.dropFirst().first) ?? visible.dropFirst().first ?? visible.first
            let targetID=target.map {String($0.key.dropFirst("shelf:".count))}
            guard let entry=library.books.first(where:{$0.id==targetID}) ?? library.recentBooks.first else { break }
            let started = Date()
            let key=index == 3 ? "verification:offscreen" : "shelf:"+entry.id
            let source = coverFrames[key]
            openBook(entry,key)
            var hiddenSamples=0, prematureSamples=0
            for _ in 0..<625 {
                if selectedBook != nil && openingBook == nil { break }
                if let alpha=preparedReaderAlpha(), !openingExpanded {
                    if alpha > 0.001 {prematureSamples += 1} else {hiddenSamples += 1}
                }
                try? await Task.sleep(for: .milliseconds(16))
            }
            let shown = selectedBook != nil && readerReady && openingBook == nil
            let seconds = Date().timeIntervalSince(started)
            try? await Task.sleep(for: .milliseconds(400))
            let hitTests=controlHitTests()
            closeBook()
            for _ in 0..<100 {
                if !closing { break }
                try? await Task.sleep(for: .milliseconds(50))
            }
            let returnedToSource = source == sourceCover
            try? await Task.sleep(for: .milliseconds(500))
            let settledShelf=coverFrames.filter {$0.key.hasPrefix("shelf:") && $0.value.width>0}
                .sorted {abs($0.value.minY-$1.value.minY)<1 ? $0.value.minX<$1.value.minX : $0.value.minY<$1.value.minY}
            let promoted = settledShelf.first?.key == "shelf:"+entry.id
            checks.append(["promoted_after_close":promoted, "cycle": index, "source_visible": source != nil, "reader_ready": shown,
                "closed": selectedBook == nil && openingBook == nil && !closing,
                "returned_to_opened_cover":returnedToSource,
                "source_x":source?.minX ?? -1,"return_x":sourceCover?.minX ?? -1,
                "source_y":source?.minY ?? -1,"return_y":sourceCover?.minY ?? -1,
                "open_seconds": seconds, "cancel_before_reopen_passed":cancellationPassed, "hidden_preparation_samples":hiddenSamples, "premature_native_samples":prematureSamples, "control_hit_tests":hitTests, "controls_hittable":hitTests.count==3 && hitTests.values.allSatisfy {$0.contains("ReaderChromeControl") || $0.contains("ReaderMenuButton")}])
            try? await Task.sleep(for: .milliseconds(500))
        }
        let file = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("book-presentation-verification.json")
        try? JSONSerialization.data(withJSONObject: checks, options: .prettyPrinted).write(to: file, options: .atomic)
    }
    @MainActor private func preparedReaderAlpha()->CGFloat? {
        func find(_ view:UIView)->CGFloat? {
            if view.accessibilityIdentifier == "book-reading-content" {
                var alpha:CGFloat=1;var item:UIView?=view
                while let current=item {alpha *= current.isHidden ? 0 : current.alpha;item=current.superview}
                return alpha
            }
            for child in view.subviews {if let alpha=find(child) {return alpha}}
            return nil
        }
        return UIApplication.shared.connectedScenes.compactMap({$0 as? UIWindowScene}).flatMap({$0.windows}).compactMap(find).first
    }
    @MainActor private func controlHitTests()->[String:String] {
        guard let window=UIApplication.shared.connectedScenes.compactMap({$0 as? UIWindowScene}).flatMap({$0.windows}).first(where:{$0.isKeyWindow}) else {return [:]}
        let safe=window.safeAreaInsets,bounds=window.bounds
        let points=["close":CGPoint(x:safe.left+38,y:safe.top+30),"bookmark":CGPoint(x:bounds.maxX-safe.right-38,y:safe.top+30),"menu":CGPoint(x:bounds.maxX-safe.right-38,y:bounds.maxY-safe.bottom-30)]
        return points.mapValues { point in
            var hit=window.hitTest(point,with:nil);var classes=[String]()
            while let view=hit {classes.append(NSStringFromClass(type(of:view)));hit=view.superview}
            return classes.joined(separator:" > ")
        }
    }
    #endif
    @available(iOS 26.0, *)
    private var modernHome: some View {
        TabView(selection: $destination) {
            Tab(L("toolbar.library"), systemImage: "books.vertical", value: Optional(0)) {
                nativeLibrary(visible: destination == 0)
            }
            Tab(L("sync.multiplatform"), systemImage: "arrow.triangle.2.circlepath", value: Optional(1)) {
                NavigationStack { SyncDesignView(account: account, nearby: nearby) }
            }
            Tab(L("toolbar.search"), systemImage: "magnifyingglass", value: Optional(2), role: .search) {
                nativeLibrary(visible: destination == 2)
                    .searchable(text: $libraryQuery, isPresented: $librarySearching, prompt: L("ui.search_books"))
            }
        }
         .tabViewSearchActivation(.searchTabSelection)
         .onChange(of: librarySearching) { was, active in
             if was && !active && openingBook == nil && selectedBook == nil && !closing { libraryQuery = "" }
         }
    }
    private func nativeLibrary(visible: Bool) -> some View {
        NavigationStack {
            LibraryDesignView(library: library, account: account, releases: releases,
                shelfRevision: shelfRevision, search: $libraryQuery, sortByTitle: $shelfSortTitle, listMode: $shelfListMode,
                shelfOrder: sharedShelfOrder, bottomSearch: true,
                open: openBook, openSync: { destination = 1 })
        }.transformPreference(BookCoverFrames.self) { frames in
            // Only the visible native tab contributes cover coordinates. Both
            // presentations share books, filtering, sort and stable shelf order.
            if !visible { frames.removeAll() }
        }
    }
    private func openBook(_ entry: LibraryEntry, _ key: String) {
        UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
        guard openingBook == nil && selectedBook == nil && !closing && !library.busy else { return }
        sourceKey = key; sourceCover = coverFrames[key]
        paper = UIColor(hex: ReadingAppearance.decode(UserDefaults.standard.string(forKey: ReadingAppearance.storageKey) ?? "").style[dark: appearanceLink.readingDark].paper)
        openingExpanded = false; readerMounted = false; readerReady = false; expanded = false; paperProgress = 0; paperOpacity = sourceCover == nil || reduceMotion ? 1 : 0
        openingBook = entry
        let token = UUID(); transitionToken = token
        Task {
            // First commit the cover-sized surface, then animate its bounds.
            try? await Task.sleep(for: .milliseconds(20))
            guard transitionToken == token else { return }
            if sourceCover != nil && !reduceMotion {
                withAnimation(.easeOut(duration: 0.10)) { paperOpacity = 1 }
                try? await Task.sleep(for: .milliseconds(100))
                guard transitionToken == token else { return }
            }
            expanded = true
            paperProgress = reduceMotion || sourceCover == nil ? 1 : 0.94
            // Commit the native layer animation before native text layout begins.
            try? await Task.sleep(for: .milliseconds(20))
            guard transitionToken == token else { return }
            readerMounted = true
            if !reduceMotion && sourceCover != nil { try? await Task.sleep(for: .milliseconds(280)) }
            var lastAdvance = Date.distantPast
            while !readerReady {
                guard transitionToken == token else { return }
                if !reduceMotion && sourceCover != nil && Date().timeIntervalSince(lastAdvance) > 0.16 {
                    paperProgress += (0.99-paperProgress)*0.45; lastAdvance=Date()
                }
                try? await Task.sleep(for: .milliseconds(16))
            }
            guard transitionToken == token else { return }
            paperProgress = 1
            // The native paper completion callback releases visibility, not a timer.
        }
        Task {
            let book = await library.open(entry)
            guard transitionToken == token else { if selectedBook == nil { library.closed() }; return }
            if let book { selectedBook = book } else { closeBook() }
        }
    }
    private func readerDidPrepare() { readerReady = true; revealReader() }
    private func revealReader() {
        guard openingExpanded && readerReady && openingBook != nil && !closing else { return }
        paperOpacity = 0
    }
    private func paperDidSettle(_ progress: CGFloat, _ opacity: Double) {
        guard openingBook != nil, !closing, progress == 1 else { return }
        if opacity == 1 {
            // Reader layout is already warm, but remains invisible until the
            // paper's actual viewport animation has completed.
            openingExpanded = true
            revealReader()
        } else if opacity == 0 && openingExpanded && readerReady {
            openingBook = nil
        }
    }

    private func closeBook(after: @escaping () -> Void = {}) {
        guard !closing else { return }
        closing = true; transitionToken = UUID()
        withAnimation(.easeOut(duration: 0.14)) { paperOpacity = 1 }
        Task {
            try? await Task.sleep(for: .milliseconds(140))
            selectedBook = nil
            library.closed()
            // The shelf remained mounted; use its current, visible destination.
            await Task.yield()
            sourceCover = coverFrames[sourceKey] ?? sourceCover
            withAnimation(.easeInOut(duration: reduceMotion || sourceCover == nil ? 0.16 : 0.30)) {
                expanded = false; paperProgress = 0
                if reduceMotion || sourceCover == nil { paperOpacity = 0 }
            }
            try? await Task.sleep(for: .milliseconds(reduceMotion || sourceCover == nil ? 160 : 300))
            if sourceCover != nil && !reduceMotion {
                withAnimation(.easeOut(duration: 0.10)) { paperOpacity = 0 }
                try? await Task.sleep(for: .milliseconds(100))
            }
            openingBook = nil; closing = false; readerReady = false
            // Begin shelf movement only after the paper-to-cover dissolve.
            withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.38)) { shelfRevision += 1 }
            after()
        }
    }
}

struct BookCover: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    let book: LibraryEntry
    let covers: ShelfCoverStore
    @State private var image: UIImage?
    var compact = false
    var iconPlaceholder = false
    var body: some View {
        GeometryReader { geometry in
        ZStack(alignment: .topLeading) {
            if let image {
                Image(uiImage: image).resizable().scaledToFill().frame(width: geometry.size.width, height: geometry.size.height).clipped()
            } else if iconPlaceholder {
                book.color
                Image(systemName: "book.closed").font(.system(size: 22, weight: .regular)).foregroundStyle(.white.opacity(0.85))
                    .frame(width: geometry.size.width, height: geometry.size.height)
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
        }.task(id: book.path) {
            let loaded = await covers.image(id: book.id, path: book.path)
            if !Task.isCancelled { image = loaded }
        }
    }
}

struct LibraryDesignView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var library: PreviewLibrary
    @ObservedObject var account: GitHubAccount
    @ObservedObject var releases: ReleaseNotice
    var shelfRevision: Int = 0
    @Environment(\.accessibilityReduceMotion) private var reduceShelfMotion
    @Namespace private var shelfNamespace
    @Binding var search: String
    @Binding var sortByTitle: Bool
    @Binding var listMode: Bool
    var shelfOrder: StableShelfOrder
    var bottomSearch: Bool = false
    let open: (LibraryEntry, String) -> Void
    var openSync: () -> Void = {}
    private enum ToolSheet: String, Identifiable {
        case sortDisplay, archive, statistics, settings
        var id: String { rawValue }
    }
    @State private var toolSheet: ToolSheet?
    @State private var importing = false
    @State private var managing = false
    @State private var selected = Set<String>()
    @State private var confirmingArchive = false
    @State private var bookShareSelection: BookShareSelection?
    @State private var renaming: LibraryEntry?
    @State private var renameTitle = ""
    private var books: [LibraryEntry] {
        let searchTerm = search.trimmingCharacters(in: .whitespacesAndNewlines)
        let ranked=sortByTitle ? library.recentBooks.sorted {$0.title.localizedCompare($1.title) == .orderedAscending} : library.recentBooks
        let ranks=Dictionary(uniqueKeysWithValues:shelfOrder.resolve(ranked.map(\.id),byTitle:sortByTitle,revision:shelfRevision).enumerated().map {($1,$0)})
        return ranked.sorted {(ranks[$0.id] ?? Int.max) < (ranks[$1.id] ?? Int.max)}
            .filter {searchTerm.isEmpty || $0.title.localizedCaseInsensitiveContains(searchTerm)}
    }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                if account.syncing { SyncProgressContent(account: account) }
                if !managing && search.isEmpty, let book = library.books.first(where: { $0.id == library.recent }) {
                    Button { open(book, "recent:" + book.id) } label: {
                        HStack(spacing: 16) {
                            BookCover(book: book, covers: library.covers, compact: true).frame(width: 62, height: 88).bookCoverFrame("recent:" + book.id)
                            VStack(alignment: .leading, spacing: 6) {
                                Text(L("library.continue_reading")).font(.caption).foregroundStyle(.secondary)
                                Text(book.title).font(.headline).foregroundStyle(.primary)
                                Text(L("annotations.chapter", (library.anchors[book.id]?.chapter ?? 0)+1)).font(.subheadline).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Image(systemName: "chevron.right").font(.subheadline).foregroundStyle(.secondary)
                        }.padding(16).background(.background, in: RoundedRectangle(cornerRadius: 12))
                    }.buttonStyle(.plain)
                }
                VStack(alignment: .leading, spacing: 16) {
                    if managing {
                        VStack(alignment: .leading, spacing: 10) {
                            Text(L("manage.count", selected.count)).font(.headline)
                            HStack {
                                Button(L("manage.all")) { selected.formUnion(books.map(\.id)) }
                                Button(L("manage.clear")) { selected.removeAll() }
                            }.font(.subheadline)
                            HStack {
                                Button(L("manage.share"), systemImage: "square.and.arrow.up") { bookShareSelection = BookShareSelection(ids: books.filter { selected.contains($0.id) }.map(\.id)) }
                                Button(L("archive.action"), systemImage: "archivebox") { confirmingArchive = true }
                            }.disabled(selected.isEmpty || library.busy).buttonStyle(.bordered)
                        }
                    } else { Text(L("ui.all_books")).font(.headline) }
                    LazyVGrid(columns: listMode ? [GridItem(.flexible())] : [GridItem(.adaptive(minimum: 145, maximum: 220), spacing: 20)], alignment: .leading, spacing: listMode ? 8 : 24) {
                        ForEach(books) { book in
                            Button { if managing { if !selected.insert(book.id).inserted { selected.remove(book.id) } } else { open(book, "shelf:" + book.id) } } label: {
                                if listMode {
                                    HStack(spacing: 12) {
                                        BookCover(book: book, covers: library.covers, compact: true, iconPlaceholder: true).frame(width: 52, height: 78).bookCoverFrame("shelf:" + book.id)
                                        VStack(alignment: .leading, spacing: 6) {
                                            Text(book.title).font(.body.weight(.medium)).foregroundStyle(.primary).lineLimit(2)
                                            Text(L("annotations.chapter", (library.anchors[book.id]?.chapter ?? 0)+1)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                        }.frame(maxWidth: .infinity, alignment: .leading)
                                        if managing { Image(systemName: selected.contains(book.id) ? "checkmark.circle.fill" : "circle").font(.title2).foregroundStyle(selected.contains(book.id) ? Color.accentColor : Color.secondary).frame(width: 44, height: 44) }
                                    }.padding(.vertical, 8).padding(.trailing, managing ? 0 : 44).frame(maxWidth: .infinity).contentShape(Rectangle())
                                } else {
                                VStack(alignment: .leading, spacing: 8) {
                                    BookCover(book: book, covers: library.covers).aspectRatio(2/3, contentMode: .fit).bookCoverFrame("shelf:" + book.id)
                                        .overlay(alignment: .topTrailing) { if managing { Image(systemName: selected.contains(book.id) ? "checkmark.circle.fill" : "circle").font(.title2).symbolRenderingMode(.palette).foregroundStyle(.white, .blue).padding(8).background(.regularMaterial, in: Circle()).padding(6) } }
                                    Text(book.title).font(.subheadline).fontWeight(.medium).foregroundStyle(.primary).lineLimit(2).padding(.trailing, managing ? 0 : 44)
                                    Text(L("annotations.chapter", (library.anchors[book.id]?.chapter ?? 0)+1)).font(.caption).foregroundStyle(.secondary).padding(.trailing, managing ? 0 : 44)
                                }
                                }
                            }.buttonStyle(.plain)
                            .overlay(alignment: .bottom) { if listMode { Divider().padding(.leading, 64) } }
                            .matchedGeometryEffect(id: book.id, in: shelfNamespace)
                            .zIndex(book.id == library.recent ? 1 : 0)
                            .overlay(alignment: listMode ? .trailing : .bottomTrailing) {
                                if !managing {
                                    Menu {
                                        Button(L("book.share"), systemImage: "square.and.arrow.up") { bookShareSelection = BookShareSelection(ids: [book.id]) }
                                        Button(L("book.rename"), systemImage: "pencil") { renameTitle = book.title; renaming = book }
                                        Divider()
                                        Button(L("archive.action"), systemImage: "archivebox") { selected = [book.id]; confirmingArchive = true }
                                    } label: { Image(systemName: "ellipsis").frame(width: 44, height: 44).contentShape(Rectangle()) }
                                        .accessibilityLabel(L("reader.more_actions"))
                                }
                            }
                        }
                    }.animation(reduceShelfMotion ? nil : .easeInOut(duration: 0.38), value: shelfRevision)
                }
                if library.books.isEmpty && search.isEmpty && !library.busy {
                    ContentUnavailableView {
                        Label(L("library.empty_state_title"), systemImage: "books.vertical")
                    } description: { Text(L("library.empty_state_hint")) } actions: {
                        Button(L("library.import_epub")) { importing = true }.buttonStyle(.borderedProminent)
                    }
                }
                if !search.isEmpty && books.isEmpty && !library.busy {
                    ContentUnavailableView { Label(L("ui.no_results"), systemImage: "magnifyingglass") }
                    Button { search = "" } label: { Text(L("ui.clear_search")).frame(minHeight: 44) }.frame(maxWidth: .infinity)
                }
                if library.busy { ProgressView().frame(maxWidth: .infinity).accessibilityLabel(L("common.processing_book")) }
            }.padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Color(uiColor: .systemGroupedBackground))
        .contentMargins(.bottom, 12, for: .scrollContent)
        .navigationTitle("")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        .safeAreaInset(edge: .bottom) { if !managing, let version = releases.newest { Link(L("common.new_version_release_notes", version), destination: URL(string: "https://github.com/zhongbai2333/RustEpubReader/releases/latest")!).font(.footnote).padding(12).frame(maxWidth: .infinity).background(.regularMaterial) } }
        .modifier(LibrarySearchPlacement(query: $search, bottom: bottomSearch))
        .toolbar {
            if #available(iOS 26.0, *) {
                ToolbarItem(placement: .topBarLeading) { libraryHeading }
                    .sharedBackgroundVisibility(.hidden)
            } else {
                ToolbarItem(placement: .topBarLeading) { libraryHeading }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button(L("ui.import"), systemImage: "doc.badge.plus") { importing = true }
                    Button(L("transfer.receive"), systemImage: "qrcode.viewfinder") { library.bookSharing.resetReceiver(); library.receivingSharedBooks = true }
                } label: { Label(L("library.add_books"), systemImage: "plus") }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.circle)
                    .disabled(library.busy)
            }
            if #available(iOS 26.0, *) { ToolbarSpacer(.fixed, placement: .topBarTrailing) }
            ToolbarItem(placement: .topBarTrailing) {
                Button(managing ? L("manage.done") : L("manage.books")) { managing.toggle(); if !managing { selected.removeAll() } }
                    .buttonBorderShape(.capsule)
                    .disabled(library.books.isEmpty || library.busy)
            }
            if #available(iOS 26.0, *) { ToolbarSpacer(.fixed, placement: .topBarTrailing) }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button(L("library.sort_display"), systemImage: "slider.horizontal.3") { toolSheet = .sortDisplay }
                    Button(L("archive.title") + " (\(library.archived.count))", systemImage: "archivebox") { toolSheet = .archive }
                    Button(L("stats.title"), systemImage: "chart.bar") { toolSheet = .statistics }
                    Button(L("settings.title"), systemImage: "gearshape") { toolSheet = .settings }
                } label: { Image(systemName: "ellipsis.circle") }.accessibilityLabel(L("reader.more_actions"))
            }
        }
        .sheet(item: $toolSheet) { sheet in
            switch sheet {
            case .sortDisplay: LibrarySortDisplayView(sortByTitle: $sortByTitle, listMode: $listMode)
            case .archive: ArchiveLibraryView(library: library)
            case .settings: LibrarySettingsView(releases: releases, count: library.books.count)
            case .statistics:
                NavigationStack {
                    ReadingStatsView(library: library, syncSettings: { toolSheet = nil; openSync() })
                        .toolbar { ToolbarItem(placement: .confirmationAction) { Button(L("settings.close")) { toolSheet = nil } } }
                }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [UTType(filenameExtension: "epub") ?? .data]) { result in
            switch result {
            case .success(let url): Task { await library.importBook(url) }
            case .failure(let error): library.fail(error, title: L("library.error_import_failed"))
            }
        }
        .sheet(item: $bookShareSelection) { selection in BookShareView(library: library, account: account, ids: selection.ids) }
        .alert(L("book.rename"), isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField(L("book.rename_hint"), text: $renameTitle)
            Button(L("error.ok")) { if let entry = renaming { let title = renameTitle; Task { _ = await library.rename(entry, title: title) } }; renaming = nil }
                .disabled(renameTitle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || library.busy)
            Button(L("settings.cancel"), role: .cancel) { renaming = nil }
        } message: { Text(L("book.rename_description")) }
        // A shelf-wide confirmation has no stable popover anchor after the book menu closes.
        .alert(L("archive.confirm"), isPresented: $confirmingArchive) {
            Button(L("archive.action")) { let ids = selected; Task { if await library.remove(ids) { selected.removeAll() } } }
            Button(L("share.cancel"), role: .cancel) {}
        } message: { Text(L("archive.note")) }
        .onDisappear {if !bottomSearch {shelfOrder.reset()}}
        .onChange(of: library.books.map(\.id)) { _, ids in selected.formIntersection(ids) }
    }
    private var libraryHeading: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Text(L("toolbar.library")).font(.system(size: 24, weight: .bold)).fixedSize()
                if account.syncing { syncIndicator.fixedSize(horizontal:true,vertical:false) }
            }
            Text(readerI18n.count("ui.library_count", library.books.count)).font(.system(size: 11)).foregroundStyle(.secondary).fixedSize()
        }.fixedSize(horizontal:true,vertical:false).accessibilityAddTraits(.isHeader)
    }
    private var syncIndicator: some View {
        HStack(spacing: 6) {
            ProgressView().controlSize(.small).tint(.secondary)
            Text(L("cloud.syncing_indicator")).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: true, vertical: false)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(L("cloud.syncing"))
    }
}
