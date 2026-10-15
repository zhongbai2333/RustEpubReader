import SwiftUI
import UIKit

// Keep the visible shelf stable while live reading metadata changes underneath it.
final class StableShelfOrder {
    private var ids=[String]()
    private var byTitle=false
    private var revision = 0
    func reset() {ids=[]}
    func resolve(_ ranked:[String],byTitle title:Bool,revision nextRevision:Int = 0)->[String] {
        guard !ranked.isEmpty else {return []}
        if revision != nextRevision { reset(); revision = nextRevision }
        let current=Set(ranked),previous=Set(ids)
        if ids.isEmpty || title || byTitle != title {ids=ranked}
        else {ids=ids.filter {current.contains($0)} + ranked.filter {!previous.contains($0)}}
        byTitle=title
        return ids
    }
}

// Global coordinates include navigation bars, safe areas and the shelf scroll position.
struct BookCoverFrames: PreferenceKey {
    static var defaultValue: [String: CGRect] = [:]
    static func reduce(value: inout [String: CGRect], nextValue: () -> [String: CGRect]) { value.merge(nextValue(), uniquingKeysWith: { _, new in new }) }
}
extension View {
    func bookCoverFrame(_ key: String) -> some View {
        background { GeometryReader { proxy in Color.clear.preference(key: BookCoverFrames.self, value: [key: proxy.frame(in: .global)]) } }
    }
}
struct BookPaperTransition: View {
    let source: CGRect?
    let expanded: Bool
    let progress: CGFloat
    let closing: Bool
    let opacity: Double
    let paper: UIColor
    let waiting: Bool
    let cancel: () -> Void
    let settled: (CGFloat,Double)->Void
    @State private var showProgress = false
    @State private var viewport = ReaderWindowViewport()
    var body: some View {
        GeometryReader { proxy in
            let bounds = CGRect(origin: .zero, size: proxy.size)
            let global = proxy.frame(in: .global)
            let top=viewport.bounds.isEmpty ? proxy.safeAreaInsets.top : max(0,viewport.safe.top-global.minY)
            let leading=viewport.bounds.isEmpty ? proxy.safeAreaInsets.leading : max(0,viewport.safe.left-global.minX)
            let local = source.map { $0.offsetBy(dx: -global.minX, dy: -global.minY) }
            let valid = local.flatMap { bounds.contains($0) && $0.width > 0 ? $0 : nil }
            let origin=valid ?? bounds
            let frame=CGRect(x:origin.minX*(1-progress),y:origin.minY*(1-progress),
                width:origin.width+(bounds.width-origin.width)*progress,
                height:origin.height+(bounds.height-origin.height)*progress)
            ZStack(alignment: .topLeading) {
                Color.clear.contentShape(Rectangle()).onTapGesture { }
                PaperMorphSurface(frame: frame, paper: paper,
                    opacity: opacity * (valid == nil && !expanded ? 0 : 1),
                    radius: 6*(1-progress),progress:progress,closing:closing,viewportChanged:{viewport=$0},settled:settled)
                    .frame(width:proxy.size.width,height:proxy.size.height)
                if showProgress && waiting {
                    ProgressView().tint(Color(uiColor: paper).luminanceIsDark ? .white : .gray)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .accessibilityLabel(L("manage.opening"))
                    ReaderChromeButton(symbol:"xmark",label:L("nav.back_to_library"),tint:Color(uiColor:paper).luminanceIsDark ? .white : .label,action:cancel)
                        .frame(width:44,height:44).background(.regularMaterial,in:Circle())
                        .padding(.leading,leading+16).padding(.top,top+8)
                }
            }.frame(width: proxy.size.width, height: proxy.size.height)
        }.task { try? await Task.sleep(for: .milliseconds(600)); if !Task.isCancelled { showProgress = true } }
    }
}
// Core Animation owns the rectangle transform so native first-page layout does
// not freeze its movement on the main thread. The reader itself never scales.
private struct PaperMorphSurface: UIViewRepresentable {
    let frame: CGRect
    let paper: UIColor
    let opacity: Double
    let radius: CGFloat
    let progress: CGFloat
    let closing: Bool
    let viewportChanged: (ReaderWindowViewport)->Void
    let settled: (CGFloat,Double)->Void
    final class Surface: UIView {
        let paper=UIView()
        var initialized=false
        var progress: CGFloat = 0
        var previousOpacity: Double = 0
        var animationGeneration=0
        var viewportChanged: ((ReaderWindowViewport)->Void)?
        private var viewport=ReaderWindowViewport()
        override func didMoveToWindow() {super.didMoveToWindow(); reportViewport()}
        override func layoutSubviews() {super.layoutSubviews(); reportViewport()}
        private func reportViewport() {
            guard let window else {return}
            let next=ReaderWindowViewport(bounds:window.bounds,safe:window.safeAreaInsets)
            guard next != viewport else {return};viewport=next
            DispatchQueue.main.async { [weak self] in self?.viewportChanged?(next) }
        }
        override init(frame:CGRect) {super.init(frame:frame);isUserInteractionEnabled=false;addSubview(paper);paper.layer.cornerCurve = .continuous}
        required init?(coder:NSCoder) {fatalError("init(coder:) has not been implemented")}
    }
    func makeUIView(context:Context)->Surface {Surface(frame:.zero)}
    func updateUIView(_ view:Surface,context:Context) {
        view.viewportChanged=viewportChanged
        let moving=view.progress != progress
        // Layout/viewport callbacks may update SwiftUI without changing the
        // paper. Do not restart a running geometry animation in that case.
        if view.initialized, !moving, view.previousOpacity == opacity,
           view.paper.frame == frame, view.paper.backgroundColor == paper,
           view.paper.layer.cornerRadius == radius {return}
        let duration:Double = !view.initialized ? 0 : moving ? (closing ? 0.30 : progress == 1 ? 0.08 : view.progress == 0 ? 0.30 : 0.16) : (expandedOpacityReveal(view) ? 0.16 : closing && progress == 1 ? 0.14 : 0.10)
        view.progress=progress;view.previousOpacity=opacity;view.initialized=true
        view.animationGeneration += 1
        let generation=view.animationGeneration
        UIView.animate(withDuration:duration,delay:0,options:[.beginFromCurrentState,.curveEaseInOut,.allowUserInteraction]) {
            view.paper.frame=frame;view.paper.backgroundColor=paper;view.paper.alpha=opacity;view.paper.layer.cornerRadius=radius
        } completion: { [weak view] finished in
            guard finished, let view, view.animationGeneration == generation else {return}
            // Completion is delivered after the paper really reaches its target.
            // A replaced/cancelled transition cannot release a newer reader.
            if !closing && progress == 1 {
                DispatchQueue.main.async {settled(progress,opacity)}
            }
        }
    }

    private func expandedOpacityReveal(_ view:Surface)->Bool {progress==1 && opacity<view.previousOpacity && !closing}
}

private extension Color {
    var luminanceIsDark: Bool { var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0; UIColor(self).getRed(&r, green: &g, blue: &b, alpha: &a); return r*0.299+g*0.587+b*0.114 < 0.45 }
}
extension ReaderOptions {
    static var savedPaper: UIColor {
        let defaults = UserDefaults.standard
        ReadingAppearance.migrate(defaults)
        let value = ReadingAppearance.decode(defaults.string(forKey: ReadingAppearance.storageKey) ?? "")
        let dark = value.mode.isDark(systemDark: UITraitCollection.current.userInterfaceStyle == .dark)
        return UIColor(hex: value.style[dark: dark].paper)
    }
}
struct ShareFiles: UIViewControllerRepresentable {
    let urls: [URL]
    func makeUIViewController(context: Context) -> UIActivityViewController { UIActivityViewController(activityItems: urls, applicationActivities: nil) }
    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
struct ReleaseInfo: Decodable { let tag_name: String; let draft: Bool; let prerelease: Bool }
@MainActor final class ReleaseNotice: ObservableObject {
    @Published var newest: String?
    @Published var checking = false
    @Published var message: String?
    var version: String { Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0.8" }
    static func components(_ version: String) -> [Int]? {
        let raw = version.hasPrefix("v") ? String(version.dropFirst()) : version
        let parts = raw.split(separator: ".", omittingEmptySubsequences: false)
        guard (2...4).contains(parts.count), parts.allSatisfy({ !$0.isEmpty && $0.allSatisfy(\.isNumber) && Int($0) != nil }) else { return nil }
        return parts.map { Int($0)! } + Array(repeating: 0, count: 4-parts.count)
    }
    func check(manual: Bool = false) async {
        guard !checking else { return }
        if !manual {
            guard UserDefaults.standard.object(forKey: "reader.releaseNotice") as? Bool ?? true else { return }
            let last = UserDefaults.standard.double(forKey: "reader.releaseCheck")
            guard Date().timeIntervalSince1970-last >= 86400 else { return }
        }
        checking = true; defer { checking = false }
        UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: "reader.releaseCheck")
        do {
            var request = URLRequest(url: URL(string: "https://api.github.com/repos/zhongbai2333/RustEpubReader/releases/latest")!); request.timeoutInterval = 15
            request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
            let (data, response) = try await URLSession.shared.data(for: request)
            guard (response as? HTTPURLResponse)?.statusCode == 200, data.count <= 1024*1024 else { throw URLError(.badServerResponse) }
            let release = try JSONDecoder().decode(ReleaseInfo.self, from: data)
            guard !release.draft && !release.prerelease, let remote = Self.components(release.tag_name), let local = Self.components(version) else { throw URLError(.cannotParseResponse) }
            newest = local.lexicographicallyPrecedes(remote) ? release.tag_name : nil
            message = newest.map { L("common.new_version_found", $0) } ?? L("common.up_to_date")
        } catch { if manual { message = L("common.error_check_update_failed") } }
    }
}
struct AboutView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    private static var speechLicense: String {
        ["EdgeSpeech-NOTICE", "edge-tts-LICENSE"].compactMap { name in
            Bundle.main.url(forResource: name, withExtension: "txt").flatMap { try? String(contentsOf: $0, encoding: .utf8) }
        }.joined(separator: "\n\n")
    }
    @ObservedObject var releases: ReleaseNotice
    let count: Int
    @Environment(\.dismiss) private var dismiss
    @AppStorage("reader.releaseNotice") private var autoNotice = true
    @State private var diagnostics: [URL] = []
    @State private var sharing = false
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    HStack(spacing: 16) {
                        Image("ReaderIcon").resizable().frame(width: 64, height: 64).clipShape(RoundedRectangle(cornerRadius: 12))
                        VStack(alignment: .leading, spacing: 4) { Text("EPUB Reader").font(.headline); Text("\(releases.version) (\(Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "1"))").foregroundStyle(.secondary) }
                    }.padding(.vertical, 8)
                    LabeledContent(L("common.developer"), value: "zhongbai233")
                    Link(L("common.project_homepage"), destination: URL(string: "https://github.com/zhongbai2333/RustEpubReader")!)
                }
                Section {
                    Button { Task { await releases.check(manual: true) } } label: { HStack { Text(L("common.check_for_updates")); Spacer(); if releases.checking { ProgressView() } } }.disabled(releases.checking)
                    if let message = releases.message { Text(LM(message)).foregroundStyle(.secondary) }
                    if releases.newest != nil { Link(L("common.view_release_notes"), destination: URL(string: "https://github.com/zhongbai2333/RustEpubReader/releases/latest")!) }
                    Toggle(L("settings.auto_notify_new_version"), isOn: $autoNotice)
                } footer: { Text(L("settings.auto_notify_new_version_desc")) }
                Section {
                    Button(L("common.export_diagnostics")) {
                        let report: [String: Any] = ["version": releases.version, "system_version": UIDevice.current.systemVersion, "book_count": count]
                        let url = FileManager.default.temporaryDirectory.appendingPathComponent("ReaderDiagnostics.json")
                        do { try JSONSerialization.data(withJSONObject: report, options: .prettyPrinted).write(to: url, options: .atomic); diagnostics = [url]; sharing = true } catch {}
                    }
                } footer: { Text(L("common.diagnostics_desc")) }
                Section(L("common.open_source_components")) {
                    Link("rbook · Apache-2.0", destination: URL(string: "https://crates.io/crates/rbook")!)
                    Link("ONNX Runtime · MIT", destination: URL(string: "https://github.com/microsoft/onnxruntime/blob/main/LICENSE")!)
                    NavigationLink("edge-tts · LGPL-3.0") {
                        ScrollView {
                            Text(Self.speechLicense).font(.caption).textSelection(.enabled).padding()
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }.navigationTitle("edge-tts").navigationBarTitleDisplayMode(.inline)
                    }
                    Link("serde · MIT / Apache-2.0", destination: URL(string: "https://serde.rs")!)
                }
            }.navigationTitle(L("about.title")).navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button(L("manage.done")) { dismiss() } } }
                .sheet(isPresented: $sharing) { ShareFiles(urls: diagnostics) }
        }
    }
}

struct BookShareSelection: Identifiable { let id = UUID(); let ids: [String] }
private struct BookShareFiles: Identifiable { let id = UUID(); let urls: [URL] }
struct BookShareView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var library: PreviewLibrary
    @ObservedObject var account: GitHubAccount
    let ids: [String]
    @Environment(\.dismiss) private var dismiss
    @State private var preparing = false
    @State private var systemShare: BookShareFiles?
    @State private var error: String?
    @State private var stage = 0
    private var books: [LibraryEntry] { ids.compactMap { id in library.books.first { $0.id == id } } }
    private func back() { if stage == 1 || library.bookSharing.busy { library.bookSharing.cancel() }; stage = 0 }
    private func export() {
        preparing = true; error = nil
        Task {
            do {
                let directory = FileManager.default.temporaryDirectory.appendingPathComponent("BookExports", isDirectory: true).appendingPathComponent(UUID().uuidString, isDirectory: true)
                let files = try await library.exportBooks(books.map(\.id), directory: directory)
                if !files.isEmpty { systemShare = BookShareFiles(urls: files) }
            } catch { self.error = L("book.export_failed", error.localizedDescription) }
            preparing = false
        }
    }
    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 18) {
                ShareBookSummary(books: books).padding(.horizontal, 24).padding(.top, 10)
                ScrollView {
                    BookShareSender(sharing: library.bookSharing, account: account, ids: books.map(\.id), stage: $stage, exporting: preparing, export: export).padding(.horizontal, 24).padding(.bottom, 24)
                    if let error { Text(error).font(.footnote).foregroundStyle(.red).padding(.horizontal, 24) }
                }.scrollBounceBehavior(.basedOnSize)
            }.background(Color(.systemGroupedBackground))
                .navigationTitle(L(stage == 1 ? "share_ui.nearby" : stage == 2 ? "transfer.github" : "book.share_title")).navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { if stage != 0 { Button(action: back) { Image(systemName: "chevron.left") }.accessibilityLabel(L("settings.back")) } }
                    ToolbarItem(placement: .confirmationAction) { Button(L("settings.close")) { dismiss() }.disabled(preparing) }
                }
                .sheet(item: $systemShare) { item in ShareFiles(urls: item.urls) }
                .sheet(isPresented: $account.loginPresented, onDismiss: { account.cancelLogin() }) { GitHubLoginView(account: account) }
        }.interactiveDismissDisabled(preparing)
            .presentationDetents(stage == 1 || (stage == 2 && library.bookSharing.phase == "published") ? [.large] : [.height(490), .large])
            .presentationDragIndicator(.visible)
    }
}
