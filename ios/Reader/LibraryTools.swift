import SwiftUI
import UIKit

struct BookOpeningView: View {
    let book: LibraryEntry
    @Environment(\.accessibilityReduceMotion) private var reduced
    @State private var unfolded = false
    var body: some View {
        VStack(spacing: 24) {
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 6).fill(Color(uiColor: .secondarySystemBackground)).frame(width: 112, height: 168)
                    .overlay(alignment: .trailing) { VStack(spacing: 10) { ForEach(0..<8) { _ in Rectangle().fill(.secondary.opacity(0.15)).frame(height: 1) } }.padding(14) }
                BookCover(book: book).frame(width: 112, height: 168)
                    .rotation3DEffect(.degrees(reduced ? 0 : (unfolded ? -55 : -8)), axis: (0, 1, 0), anchor: .leading, perspective: 0.5)
            }.frame(width: 150, height: 180)
            Text(book.title).font(.headline).lineLimit(2)
            HStack(spacing: 10) { ProgressView(); Text("正在打开书籍…").font(.subheadline).foregroundStyle(.secondary) }
        }.padding(32).frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(.regularMaterial).accessibilityElement(children: .combine)
            .onAppear { guard !reduced else { return }; withAnimation(.easeInOut(duration: 1.2).repeatForever(autoreverses: true)) { unfolded = true } }
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
            message = newest.map { "发现新版本 \($0)" } ?? "当前已是最新版本"
        } catch { if manual { message = "暂时无法检查，请稍后重试。" } }
    }
}
struct AboutView: View {
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
                    LabeledContent("开发者", value: "zhongbai233")
                    Link("项目主页", destination: URL(string: "https://github.com/zhongbai2333/RustEpubReader")!)
                }
                Section {
                    Button { Task { await releases.check(manual: true) } } label: { HStack { Text("检查新版本"); Spacer(); if releases.checking { ProgressView() } } }.disabled(releases.checking)
                    if let message = releases.message { Text(message).foregroundStyle(.secondary) }
                    if releases.newest != nil { Link("查看发行说明", destination: URL(string: "https://github.com/zhongbai2333/RustEpubReader/releases/latest")!) }
                    Toggle("自动提示新版本", isOn: $autoNotice)
                } footer: { Text("仅在前台检查新版本并提示，不会自动下载或安装。") }
                Section {
                    Button("导出诊断信息") {
                        let report: [String: Any] = ["version": releases.version, "system_version": UIDevice.current.systemVersion, "book_count": count]
                        let url = FileManager.default.temporaryDirectory.appendingPathComponent("ReaderDiagnostics.json")
                        do { try JSONSerialization.data(withJSONObject: report, options: .prettyPrinted).write(to: url, options: .atomic); diagnostics = [url]; sharing = true } catch {}
                    }
                } footer: { Text("诊断信息仅包含版本、系统版本和书库数量，不包含账号凭据或书籍内容。") }
                Section("开源组件") {
                    Link("rbook · Apache-2.0", destination: URL(string: "https://crates.io/crates/rbook")!)
                    Link("ONNX Runtime · MIT", destination: URL(string: "https://github.com/microsoft/onnxruntime/blob/main/LICENSE")!)
                    Link("serde · MIT / Apache-2.0", destination: URL(string: "https://serde.rs")!)
                }
            }.navigationTitle("关于").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { dismiss() } } }
                .sheet(isPresented: $sharing) { ShareFiles(urls: diagnostics) }
        }
    }
}
