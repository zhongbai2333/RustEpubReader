import SwiftUI
import UIKit

struct ReaderOptions: Equatable {
    var fontSize: Double
    var lineHeight: Double
    var fontName: String
    var theme: String
    var sliding: Bool
    var backgroundHex = "F7F2E8"
    var textHex = "252525"
    var paragraphSpacing = 0.65
    var pageMargin = 24.0
    var firstLineIndent = 2.0
    var justified = true
    var cjkFontName: String? = nil
    var nativeRevision = 0
    var readingMode: ReadingMode = .curl
    var backgroundImageName = ""
    var backgroundImageEnabled = true
    var language = ReaderI18n.shared.resolvedCode
    var backgroundImageOpacity = 0.22
    var paper: UIColor {
        switch theme {
        case "dark": return UIColor(hex: "1A1A1C")
        case "light": return .white
        case "warm": return UIColor(hex: "EEDDC4")
        case "gray": return UIColor(hex: "E4E4E2")
        case "green": return UIColor(hex: "E2EADC")
        case "custom": return UIColor(hex: backgroundHex)
        default: return UIColor(red: 0.97, green: 0.95, blue: 0.91, alpha: 1)
        }
    }
    var ink: UIColor { theme == "custom" ? UIColor(hex: textHex) : theme == "dark" ? UIColor(white: 0.86, alpha: 1) : UIColor(hex: "252525") }
    var darkAppearance: Bool { var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0; paper.getRed(&r, green: &g, blue: &b, alpha: &a); return r * 0.299 + g * 0.587 + b * 0.114 < 0.45 }
}

extension UIColor {
    convenience init(hex: String) {
        let rgb = UInt32(hex, radix: 16) ?? 0xF7F2E8
        self.init(red: CGFloat((rgb >> 16) & 255) / 255, green: CGFloat((rgb >> 8) & 255) / 255, blue: CGFloat(rgb & 255) / 255, alpha: 1)
    }
    var readerHex: String {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        getRed(&r, green: &g, blue: &b, alpha: &a)
        return String(format: "%02X%02X%02X", Int((r * 255).rounded()), Int((g * 255).rounded()), Int((b * 255).rounded()))
    }
}

struct ReadingWorkspace: View {
    @Namespace private var chromeGlass
    @EnvironmentObject private var appearanceLink: AppearanceCoordinator
    @ObservedObject private var readerI18n = ReaderI18n.shared
    let book: PreviewBook
    var contentVisible: Bool
    var openSync: () -> Void = {}
    var close: () -> Void = {}
    var ready: () -> Void = {}
    var paperChanged: (UIColor) -> Void = { _ in }
    @ObservedObject var library: PreviewLibrary
    @StateObject private var annotations: ReadingAnnotations
    @StateObject private var model: ChineseCorrection
    @StateObject private var speech: ReadingSpeech
    @StateObject private var activity: ReadingActivity
    @State private var passage: SelectedPassage?
    @State private var prepared = false
    @State private var annotationsReady = false
    @State private var viewport = ReaderWindowViewport()
    @AppStorage("preview.readingStyle") private var readingMode = "paging"
    @AppStorage("preview.pageAnimation") private var pageAnimation = "curl"
    @AppStorage("preview.cssPolicy") private var cssPolicy = "mixed"
    @AppStorage("preview.scriptsEnabled") private var scriptsEnabled = true
    @AppStorage("preview.showImmersiveStatus") private var showImmersiveStatus = true
    @State private var statusInfo: ReadingStatusInfo?
    @AppStorage(ReadingIntroduction.storageKey) private var introductionSeen = false
    @State private var introductionPresented = false
    @State private var nativeStyles: [Int:[NativeStyle]] = [:]
    @State private var nativeEvents: [Int:[Int]] = [:]
    @State private var nativeRevision = 0
    @State private var publicationNote: NativeNote?
    init(book: PreviewBook, library: PreviewLibrary, openSync: @escaping () -> Void = {}, close: @escaping () -> Void = {}, ready: @escaping () -> Void = {}, paperChanged: @escaping (UIColor) -> Void = { _ in }, contentVisible: Bool = true) {
        ReadingIntroduction.prepare()
        ReaderFontPicker.migrateLegacySettings()
        ReadingMode.migrate()
        ReadingAppearance.migrate()
        _current = State(initialValue: library.anchors[book.id] ?? book.entry.anchor)
        self.contentVisible = contentVisible
        self.book = book; self.library = library; self.openSync = openSync; self.close = close; self.ready = ready; self.paperChanged = paperChanged
        _annotations = StateObject(wrappedValue: ReadingAnnotations(library: library, book: book))
        _model = StateObject(wrappedValue: ChineseCorrection(root: library.client.root))
        _speech = StateObject(wrappedValue: ReadingSpeech(book: book))
        _activity = StateObject(wrappedValue: ReadingActivity(library: library, id: book.id))
    }
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage("preview.fontSize") private var fontSize = 18.0
    @AppStorage("preview.lineHeight") private var lineHeight = 1.65
    // Preserve the old key for the Chinese font during the split.
    @AppStorage("preview.fontName") private var fontName = "PingFangSC-Regular"
    @AppStorage("preview.latinFontName") private var latinFontName = "Helvetica"
    @AppStorage("preview.theme") private var theme = "paper"
    @AppStorage(ReadingAppearance.storageKey) private var appearanceJSON = ""
    private var appearance: ReadingAppearance { ReadingAppearance.decode(appearanceJSON) }
    private var appearanceDark: Bool { appearanceLink.readingDark }
    private var chromeInk: UIColor { appearanceLink.appDark ? UIColor(white: 0.9, alpha: 1) : UIColor(hex: "252525") }
    private var appearanceBinding: Binding<ReadingAppearance> { Binding(get: { appearance }, set: { appearanceJSON = $0.encoded }) }
    @State private var tools = true
    @State private var panel: String?
    @State private var quickMenuOpen = false
    @State private var panelDetent: PresentationDetent = .medium
    @State private var current = ReadingAnchor(chapter: 0)
    @State private var position = ""
    @State private var jump: ReadingAnchor?
    @State private var jumpingID = 0
    @AppStorage("preview.backgroundHex") private var backgroundHex = "F7F2E8"
    @AppStorage("preview.textHex") private var textHex = "252525"
    @AppStorage("preview.paragraphSpacing") private var paragraphSpacing = 0.65
    @AppStorage("preview.pageMargin") private var pageMargin = 24.0
    @AppStorage("preview.firstLineIndent") private var firstLineIndent = 2.0
    @AppStorage("preview.justified") private var justified = true
    @AppStorage("preview.backgroundImageName") private var backgroundImageName = ""
    @AppStorage("preview.backgroundImageEnabled") private var backgroundImageEnabled = true
    @AppStorage("preview.backgroundImageOpacity") private var backgroundImageOpacity = 0.22
    private var options: ReaderOptions {
        let paint = appearance.style[dark: appearanceDark]
        return .init(fontSize: fontSize, lineHeight: lineHeight, fontName: latinFontName, theme: "custom", sliding: pageAnimation != "curl" || reduceMotion,
        backgroundHex: paint.paper, textHex: paint.ink, paragraphSpacing: paragraphSpacing, pageMargin: pageMargin, firstLineIndent: firstLineIndent, justified: justified, cjkFontName: fontName, nativeRevision: nativeRevision, readingMode: ReadingMode.resolve(style: readingMode, animation: pageAnimation), backgroundImageName: paint.image, backgroundImageEnabled: !paint.image.isEmpty, backgroundImageOpacity: paint.strength) }
    private var bookmarked: Bool { annotations.value.bookmarks.contains { $0.chapter == current.chapter && $0.block == current.block } }
    private var showTools: Bool { tools && panel == nil && passage == nil }
    private var showTopTools: Bool { tools && passage == nil && (panel == nil || panelDetent != .large) }
    private var readingSurface: some View {
        ZStack {
            Color(uiColor: options.paper).ignoresSafeArea()
            if prepared || (annotationsReady && nativeStyles[current.chapter] != nil) {
            NativeReader(book: book, initial: library.anchors[book.id] ?? book.entry.anchor, options: options, annotations: annotations.value, annotationRevision: annotations.revision,
                         speechHighlight: speech.highlight, followSpeech: speech.following, selected: { activity.touch(); passage = $0 }, jump: jump, jumpID: jumpingID,
                         toggleTools: { activity.touch(); withAnimation(.easeInOut(duration: reduceMotion ? 0 : 0.35)) { tools.toggle() } },
                         turning: { activity.touch(); speech.detach(); withAnimation(.easeInOut(duration: reduceMotion ? 0 : 0.25)) { tools = false } },
                         visible: { anchor, label in current = anchor; position = label; if !prepared { prepared = true; ready() } },
                         committed: { library.save($0, for: book) },
                         publicationNote: { chapter,href in
                             let client=library.client;let id=book.id
                             Task {publicationNote=try? await Task.detached {try client.call("native_note",["id":id,"chapter":chapter,"href":href],as:NativeNote.self)}.value}
                         }, publicationEvent: { chapter,node in
                             guard scriptsEnabled,(nativeEvents[chapter]?.count ?? 0)<128 else {return}
                             nativeEvents[chapter,default:[]].append(node)
                         }, nativeStyles: nativeStyles, viewportChanged: { viewport = $0 }, contentVisible: contentVisible, immersiveStatus: showImmersiveStatus ? statusInfo : nil)
                .ignoresSafeArea()
            }
            GeometryReader { geometry in
                let frame=geometry.frame(in:.global)
                let top=viewport.bounds.isEmpty ? geometry.safeAreaInsets.top : max(0,viewport.safe.top-frame.minY)
                let bottom=viewport.bounds.isEmpty ? geometry.safeAreaInsets.bottom : max(0,frame.maxY-(viewport.bounds.maxY-viewport.safe.bottom))
                let left=viewport.bounds.isEmpty ? geometry.safeAreaInsets.leading : max(0,viewport.safe.left-frame.minX)
                let right=viewport.bounds.isEmpty ? geometry.safeAreaInsets.trailing : max(0,frame.maxX-(viewport.bounds.maxX-viewport.safe.right))
                ReaderChromeGlassContainer {
                VStack(spacing: 0) {
                        HStack(spacing: 8) {
                          if showTopTools {
                            ReaderChromeButton(symbol:"xmark",label:L("nav.back_to_library"),tint:chromeInk,nativeGlass:false,action:close)
                                .frame(width:44,height:44)
                                .readerChromeGlass(in: Circle(), id: "close", namespace: chromeGlass)
                            VStack(spacing: 2) {
                                Text(book.title).font(.system(size: 13, weight: .medium)).lineLimit(1)
                                Text("\(current.chapter+1)/\(book.chapters.count) · \(book.chapters.indices.contains(current.chapter) ? book.chapters[current.chapter].title : "")")
                                    .font(.system(size: 11)).foregroundStyle(Color(uiColor: chromeInk).opacity(0.65)).lineLimit(1)
                            }.padding(.horizontal, 12).padding(.vertical, 7)
                                .frame(maxWidth: .infinity)
                                .readerChromeTitleGlass(namespace: chromeGlass)
                                .allowsHitTesting(false).accessibilityElement(children: .combine)
                            ReaderChromeButton(symbol:bookmarked ? "bookmark.fill" : "bookmark",
                                label:bookmarked ? L("reader.remove_bookmark") : L("context.bookmark"),
                                tint:chromeInk,enabled:!annotations.busy,nativeGlass:false) {
                                    Task {await annotations.perform(["action":"bookmark","chapter":current.chapter,"block":current.block])}
                                }.frame(width:44,height:44)
                                .readerChromeGlass(in: Circle(), id: "bookmark", namespace: chromeGlass)
                          }
                        }
                        .frame(height:44)
                        .padding(.leading,left+16).padding(.trailing,right+16)
                    Spacer(minLength: 0)
                    ZStack(alignment: .trailing) {
                        if speech.hasSession && panel == nil && passage == nil {
                            ReadingSpeechCapsule(speech: speech, current: current, open: { panel = "speech" })
                                .padding(.trailing, showTools ? 56 : 0)
                        } else if options.readingMode == .scroll {
                            Text(position).font(.caption).monospacedDigit().foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, alignment: .trailing)
                                .padding(.trailing, showTools ? 56 : 0).allowsHitTesting(false)
                        }
                        if showTools {
                            ReadingToolsMenu(tint: chromeInk,
                                contents: { panelDetent = .large; panel = "toc" }, settings: { panel = "settings" },
                                speech: { panel = "speech" },
                                annotations: { panel = "annotations" },
                                correction: { model.suggestions = []; panel = "correction" },
                                search: { panelDetent = .large; panel = "search" },
                                statistics: { activity.flush(); panelDetent = .large; panel = "stats" },
                                night: appearanceDark,
                                dayNight: {
                                    appearanceLink.toggleReading()
                                }, visibility: {quickMenuOpen=$0;activity.touch()})
                                .frame(width: 44, height: 44)
                                .readerChromeGlass(in: Circle(), id: "menu", namespace: chromeGlass)
                        }
                    }.frame(maxWidth: .infinity, alignment: .trailing).frame(height: 44).padding(.leading,left+16).padding(.trailing,right+16)
                        .padding(.bottom, options.readingMode.paginated ? 24 : 0)
                        .overlay(alignment: .bottomLeading) {
                            if options.readingMode == .scroll, showImmersiveStatus, let statusInfo {
                                ReadingImmersiveStatus(info: statusInfo, ink: options.ink).padding(.leading,left+16)
                            }
                        }
                }
                .padding(.top,top+8).padding(.bottom,bottom+8)
                .foregroundStyle(Color(uiColor: chromeInk))
                .buttonStyle(.plain).dynamicTypeSize(...DynamicTypeSize.xxxLarge)
                }
                .animation(reduceMotion ? nil : .easeInOut(duration: 0.35), value: panel)
                .animation(reduceMotion ? nil : .easeInOut(duration: 0.35), value: panelDetent)
            }
        }
    }
    private var readingPanels: some View {
        readingSurface
        .sheet(item: $publicationNote) { note in
            NavigationStack {ScrollView {Text(note.text).textSelection(.enabled).frame(maxWidth:.infinity,alignment:.leading).padding(24)}.navigationTitle(L("reader.publication_note")).navigationBarTitleDisplayMode(.inline).toolbar {ToolbarItem(placement:.confirmationAction){Button(L("settings.close")){publicationNote=nil}}}}.readerPanelSurface(appearance: appearanceLink).presentationDetents([.medium,.large])
        }
        .sheet(item: $passage) { SelectedTextView(passage: $0, annotations: annotations).readerPanelSurface(appearance: appearanceLink) }
        .task { _ = await annotations.perform(); annotationsReady = true }
        .statusBarHidden(!tools)
        .persistentSystemOverlays(tools ? .visible : .hidden)
        .sheet(item: Binding(get: { panel.map(ReaderPanel.init) }, set: { panel = $0?.id })) { selected in
            NavigationStack {
                Group {
                if selected.id == "toc" {
                    ReaderContentsView(book: book, current: current) { anchor in
                        speech.detach(); jump = anchor; jumpingID += 1; library.save(anchor, for: book); panel = nil
                    }
                } else if selected.id == "search" {
                    ReaderSearchView(book: book) { anchor in
                        speech.detach(); jump = anchor; jumpingID += 1; library.save(anchor, for: book); panel = nil
                    }
                } else if selected.id == "annotations" {
                    AnnotationsView(annotations: annotations, current: current, jump: { speech.detach(); jump = $0; jumpingID += 1; library.save($0, for: book); panel = nil })
                } else if selected.id == "correction" {
                    CorrectionView(model: model, annotations: annotations, chapter: current.chapter)
                } else if selected.id == "stats" {
                    ReadingStatsView(library: library, currentID: book.id, syncSettings: {activity.stop();openSync()})
                } else if selected.id == "speech" {
                    ReadingSpeechPanel(speech: speech, current: current)
                } else {
                    ReaderAppearanceSettings(fontSize: $fontSize, lineHeight: $lineHeight, fontName: $fontName,
                        theme: $theme, readingMode: $readingMode, backgroundHex: $backgroundHex, textHex: $textHex,
                        paragraphSpacing: $paragraphSpacing, pageMargin: $pageMargin, firstLineIndent: $firstLineIndent, justified: $justified, pageAnimation: $pageAnimation, latinFontName: $latinFontName, cssPolicy: $cssPolicy, scriptsEnabled: $scriptsEnabled, showImmersiveStatus: $showImmersiveStatus, backgroundImageName: $backgroundImageName, backgroundImageEnabled: $backgroundImageEnabled, backgroundImageOpacity: $backgroundImageOpacity, appearance: appearanceBinding, appearanceDark: appearanceDark)
                }
                }
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button(selected.id == "speech" ? L("common.collapse") : selected.id == "stats" ? L("settings.close") : L("manage.done")) { panel = nil } }
                    if selected.id == "settings" || selected.id == "speech" {
                        ToolbarItem(placement: .topBarLeading) {
                            Button { panelDetent = panelDetent == .large ? .medium : .large } label: {
                                Image(systemName: panelDetent == .large ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                            }.accessibilityLabel(panelDetent == .large ? (selected.id == "speech" ? L("reader.collapse_tts_panel") : L("reader.collapse_settings")) : (selected.id == "speech" ? L("reader.expand_tts_panel") : L("reader.expand_settings")))
                        }
                    }
                }
                .toolbarBackground(Color(uiColor: .systemGroupedBackground), for: .navigationBar)
                .toolbarBackground(.visible, for: .navigationBar)
            }
            .environment(\.colorScheme, appearanceLink.appDark ? .dark : .light)
            .tint(Color(uiColor: .systemBlue))
            .readerPanelSurface(appearance: appearanceLink)
            .presentationDetents([.medium, .large], selection: $panelDetent).presentationDragIndicator(.visible)
            .presentationBackgroundInteraction(panel == "speech" ? .enabled(upThrough: .medium) : .disabled)
        }
    }
    private var introductionReady: Bool {
        prepared && contentVisible && scenePhase == .active && !introductionSeen && panel == nil && passage == nil && publicationNote == nil && !quickMenuOpen
    }
    var body: some View {
        readingPanels
        .task(id: introductionReady) {
            if introductionReady { tools = true; introductionPresented = true }
        }
        .sheet(isPresented: $introductionPresented, onDismiss: { tools = true }) {
            ReadingIntroductionView { introductionPresented = false; tools = true }
                .onAppear { introductionSeen = true }
        }
        .onAppear {
            paperChanged(options.paper)
            #if DEBUG
            if CommandLine.arguments.contains("--appearance-settings") { panelDetent = .large; panel = "settings" }
            #endif
        }
        .task(id: "\(showImmersiveStatus)/\(scenePhase)") {
            guard showImmersiveStatus, scenePhase == .active else { statusInfo = nil; return }
            ReadingStatusInfo.beginMonitoring()
            defer { ReadingStatusInfo.endMonitoring() }
            while !Task.isCancelled {
                statusInfo = ReadingStatusInfo.current()
                do { try await Task.sleep(for: .seconds(15)) } catch { break }
            }
        }
        .onChange(of: options.paper) { _, value in paperChanged(value) }
        .onChange(of: annotations.revision) { _, _ in speech.refreshText(corrections: annotations.value.corrections ?? []) }
        .task {
            while !Task.isCancelled {
                activity.sample(reading: prepared && !introductionPresented && scenePhase == .active && !quickMenuOpen && panel == nil && passage == nil && publicationNote == nil, listening: speech.audible)
                try? await Task.sleep(for: .seconds(1))
            }
        }
        .onChange(of: current) { _, _ in if !speech.following || speech.state == .idle {activity.touch()} }
        .onChange(of: quickMenuOpen) { _, open in if open {activity.sample(reading:false,listening:speech.audible)} }
        .onChange(of: panel) { _, value in activity.touch(); if value != nil { activity.sample(reading: false, listening: speech.audible) } }
        .onChange(of: scenePhase) { _, phase in if phase != .active { speech.pause(); activity.stop() } else {activity.touch()} }
        .onDisappear { activity.stop() }
        .task(id: "\(cssPolicy)/\(scriptsEnabled)/\(current.chapter)/\(nativeEvents)") {
            let client=library.client, id=book.id, mode=cssPolicy, enabled=scriptsEnabled, events=nativeEvents
            let indices=Array(max(0,current.chapter-2)..<min(book.chapters.count,current.chapter+3))
            let chapter=current.chapter
            let first=await Task.detached { () -> NativePresentation in
                (try? client.call("native_presentation",["id":id,"chapter":chapter,"policy":mode,"scripts":enabled,"events":(events[chapter] ?? []).map {["node":$0]}],as:NativePresentation.self)) ?? NativePresentation(styles:[],errors:[])
            }.value
            guard !Task.isCancelled else {return}
            nativeStyles[chapter]=first.styles; nativeRevision += 1
            // Optional neighbour styles must not trigger a second first-screen layout.
            while !prepared && !Task.isCancelled { try? await Task.sleep(for:.milliseconds(16)) }
            guard !Task.isCancelled else {return}
            let result=await Task.detached {
                indices.filter {$0 != chapter}.reduce(into:[Int:NativePresentation]()) { result,index in
                    result[index]=try? client.call("native_presentation",["id":id,"chapter":index,"policy":mode,"scripts":enabled,"events":(events[index] ?? []).map {["node":$0]}],as:NativePresentation.self)
                }
            }.value
            guard !Task.isCancelled else {return}
            var styles = nativeStyles.filter { indices.contains($0.key) }
            for (index, value) in result { styles[index] = value.styles }
            if styles != nativeStyles { nativeStyles = styles; nativeRevision += 1 }
        }
        .onChange(of: cssPolicy) { _,_ in nativeStyles=[:] }
        .onChange(of: scriptsEnabled) { _,_ in nativeStyles=[:];nativeEvents=[:] }
        .onDisappear { speech.stop(); model.cancel() }
        .alert(L("reader.new_position_detected"), isPresented: Binding(get: { library.offer != nil }, set: { if !$0 { library.offer = nil } })) {
            Button(L("common.jump")) {
                if let offer = library.offer { speech.detach(); jump = offer.anchor; jumpingID += 1; library.save(offer.anchor, for: book) }
                library.offer = nil
            }
            Button(L("common.later"), role: .cancel) { library.offer = nil }
        } message: { Text(library.offer?.chapter_title ?? L("annotations.chapter", 1 + (library.offer?.chapter ?? 0))) }
        .alert(LM(library.errorTitle), isPresented: Binding(get: { library.error != nil }, set: { if !$0 { library.error = nil } })) {
            Button(L("manage.done"), role: .cancel) { library.error = nil }
        } message: { Text(LM(library.error ?? "")) }
    }

}
struct ReaderPanel: Identifiable { var id: String }

private extension Array { subscript(safe index:Int)->Element? {indices.contains(index) ? self[index] : nil} }
