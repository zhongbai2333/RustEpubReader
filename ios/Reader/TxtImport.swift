import SwiftUI
import UniformTypeIdentifiers

struct TxtImportRequest: Identifiable {
    let id = UUID()
    let url: URL
}
struct TxtChapterPreview: Decodable, Identifiable, Sendable {
    let title: String
    let line_start: Int
    let char_count: Int
    let is_heading: Bool
    let excerpt: String
    var id: Int { line_start }
}
struct TxtAnalysis: Decodable, Sendable {
    let encoding: String
    let char_count: Int
    let chapter_count: Int
    let heading_count: Int
    let chapters: [TxtChapterPreview]
}
struct TxtImportResult: Decodable {
    let entry: LibraryEntry
    let chapter_count: Int
}

// Own only our staging directory, never the security-scoped original. Actor calls
// retain this lease until synchronous Rust work has finished, even after dismissal.
final class PreparedTXT: @unchecked Sendable {
    // Initialized once, off the main actor, before any import lease exists.
    // Only UUID-named directories in our private staging namespace are ours.
    private static let stagingRoot: Result<URL, Error> = Result {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ReaderTXTImports", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        for old in try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey]) where UUID(uuidString: old.lastPathComponent) != nil {
            let values = try old.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            if values.isDirectory == true && values.isSymbolicLink != true { try FileManager.default.removeItem(at: old) }
        }
        return root
    }
    let directory: URL
    let url: URL
    init(directory: URL, url: URL) { self.directory = directory; self.url = url }
    deinit { try? FileManager.default.removeItem(at: directory) }
    static func stage(_ source: URL) throws -> PreparedTXT {
        let access = source.startAccessingSecurityScopedResource()
        defer { if access { source.stopAccessingSecurityScopedResource() } }
        let directory = try stagingRoot.get().appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        let staged = directory.appendingPathComponent(source.lastPathComponent)
        var coordinationError: NSError?
        var result: Result<Void, Error>?
        NSFileCoordinator().coordinate(readingItemAt: source, options: .withoutChanges, error: &coordinationError) { coordinated in
            result = Result {
                let values = try coordinated.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
                guard values.isRegularFile == true else { throw CoreFailure(message: "TXT_NOT_REGULAR_FILE") }
                guard (values.fileSize ?? 0) <= 64 * 1024 * 1024 else { throw CoreFailure(message: "TXT_TOO_LARGE") }
                guard FileManager.default.createFile(atPath: staged.path, contents: nil, attributes: [.posixPermissions: 0o600]) else { throw CocoaError(.fileWriteUnknown) }
                let input = try FileHandle(forReadingFrom: coordinated)
                defer { try? input.close() }
                let output = try FileHandle(forWritingTo: staged)
                defer { try? output.close() }
                var size = 0
                while let chunk = try input.read(upToCount: 256 * 1024), !chunk.isEmpty {
                    try Task.checkCancellation()
                    size += chunk.count
                    guard size <= 64 * 1024 * 1024 else { throw CoreFailure(message: "TXT_TOO_LARGE") }
                    try output.write(contentsOf: chunk)
                }
            }
        }
        do {
            if let coordinationError { throw coordinationError }
            guard let result else { throw CocoaError(.fileReadUnknown) }
            try result.get()
            return PreparedTXT(directory: directory, url: staged)
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
    }
}
extension CoreStore {
    func analyzeTXT(_ file: PreparedTXT, heuristic: Bool, regex: String, excluded: Set<Int>, included: Set<Int>) throws -> TxtAnalysis {
        try Task.checkCancellation()
        defer { withExtendedLifetime(file) {} }
        return try client.call("txt_analyze", ["path": file.url.path, "use_heuristic": heuristic, "custom_regex": regex, "excluded_lines": Array(excluded), "included_lines": Array(included)])
    }
    func txtLines(_ file: PreparedTXT, query: String) throws -> [TxtChapterPreview] {
        try Task.checkCancellation()
        defer { withExtendedLifetime(file) {} }
        return try client.call("txt_lines", ["path": file.url.path, "query": query])
    }
    func convertTXT(_ file: PreparedTXT, title: String, author: String, heuristic: Bool, regex: String, excluded: Set<Int>, included: Set<Int>) throws -> TxtImportResult {
        defer { withExtendedLifetime(file) {} }
        return try client.call("txt_convert", ["path": file.url.path, "title": title, "author": author, "use_heuristic": heuristic, "custom_regex": regex, "excluded_lines": Array(excluded), "included_lines": Array(included)])
    }
}

@MainActor final class TxtImportModel: ObservableObject {
    @Published var title: String
    @Published var author = ""
    @Published var heuristic = true
    @Published var regex = ""
    @Published var analysis: TxtAnalysis?
    @Published var candidates: [TxtChapterPreview] = []
    @Published var excluded: Set<Int> = []
    @Published var included: Set<Int> = []
    @Published var analyzing = true
    @Published var converting = false
    @Published var failure: String?
    private(set) var file: PreparedTXT?
    private var task: Task<Void, Never>?
    private var generation = UUID()
    private let source: URL
    let library: PreviewLibrary
    init(source: URL, library: PreviewLibrary) {
        self.source = source; self.library = library
        title = source.deletingPathExtension().lastPathComponent
    }
    func prepare() async {
        do {
            let source = source
            let prepared = try await Task.detached(priority: .userInitiated) { try PreparedTXT.stage(source) }.value
            guard !Task.isCancelled else { return }
            file = prepared
            refresh(reset: true)
        } catch { failure = error.localizedDescription; analyzing = false }
    }
    func close() { generation = UUID(); task?.cancel(); file = nil }
    func refresh(reset: Bool = false) {
        guard let file else { return }
        if reset { excluded = []; included = []; candidates = [] }
        task?.cancel()
        let token = UUID(); generation = token
        let heuristic = heuristic, regex = regex, excluded = excluded, included = included
        analyzing = true; failure = nil
        task = Task {
            do {
                try await Task.sleep(for: .milliseconds(300))
                let result = try await library.store.analyzeTXT(file, heuristic: heuristic, regex: regex, excluded: excluded, included: included)
                guard !Task.isCancelled, generation == token else { return }
                analysis = result
                var rows = Dictionary(uniqueKeysWithValues: candidates.map { ($0.line_start, $0) })
                for row in result.chapters where row.is_heading { rows[row.line_start] = row }
                candidates = rows.values.sorted { $0.line_start < $1.line_start }
                analyzing = false
            } catch is CancellationError { }
            catch { if generation == token { failure = error.localizedDescription; analyzing = false } }
        }
    }
    func toggle(_ row: TxtChapterPreview) {
        if excluded.contains(row.id) { excluded.remove(row.id) } else { excluded.insert(row.id) }
        refresh()
    }
    func add(_ row: TxtChapterPreview) {
        included.insert(row.id); excluded.remove(row.id)
        if !candidates.contains(where: { $0.id == row.id }) { candidates.append(row); candidates.sort { $0.id < $1.id } }
        refresh()
    }
    func convert() async -> Bool {
        guard let file, !analyzing, !converting, failure == nil, analysis != nil, !library.busy else { return false }
        converting = true; library.busy = true
        defer { converting = false; library.busy = false }
        do {
            _ = try await library.store.convertTXT(file, title: title, author: author, heuristic: heuristic, regex: regex, excluded: excluded, included: included)
            await library.reload()
            return true
        } catch { failure = error.localizedDescription; return false }
    }
}

struct TxtImportView: View {
    @StateObject private var model: TxtImportModel
    @ObservedObject private var i18n = ReaderI18n.shared
    @Environment(\.dismiss) private var dismiss
    init(source: URL, library: PreviewLibrary) { _model = StateObject(wrappedValue: TxtImportModel(source: source, library: library)) }
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(L("txt.book_title"), text: $model.title).accessibilityIdentifier("txt-title")
                    TextField(L("txt.author_optional"), text: $model.author)
                } footer: { Text(L("txt.source_preserved")) }
                Section {
                    DisclosureGroup(L("txt.advanced")) {
                        Toggle(L("txt.mode_heuristic"), isOn: $model.heuristic)
                        Text(L("txt.heuristic_hint")).font(.footnote).foregroundStyle(.secondary)
                        TextField(L("txt.rule_placeholder"), text: $model.regex)
                            .textInputAutocapitalization(.never).autocorrectionDisabled().accessibilityIdentifier("txt-regex")
                    }
                }
                if model.analyzing { Section { ProgressView(L("txt.analyzing")).accessibilityIdentifier("txt-analyzing") } }
                if let failure = model.failure {
                    Section { Text(failure).foregroundStyle(.red).accessibilityIdentifier("txt-error")
                        if model.file != nil { Button(L("txt.retry")) { model.refresh() } }
                    }
                }
                if let analysis = model.analysis {
                    Section {
                        Text(L("txt.detected_count", analysis.heading_count)).accessibilityIdentifier("txt-count")
                        if analysis.heading_count == 0 { Text(L(model.excluded.isEmpty ? "txt.fallback_hint" : "txt.no_selection")).font(.footnote).foregroundStyle(.secondary) }
                        if analysis.chapter_count > 500 { Text(L("txt.preview_limit", 500)).font(.footnote).foregroundStyle(.secondary) }
                        if !model.excluded.isEmpty { Button(L("txt.restore_titles")) { model.excluded = []; model.refresh() } }
                        NavigationLink { TxtSourceLinesView(model: model) } label: { Label(L("txt.add_heading"), systemImage: "text.badge.plus") }
                            .accessibilityIdentifier("txt-add-heading")
                    }
                    Section {
                        ForEach(model.candidates) { row in
                            HStack(spacing: 12) {
                                Button { model.toggle(row) } label: {
                                    Image(systemName: model.excluded.contains(row.id) ? "circle" : "checkmark.circle.fill")
                                        .font(.title3).frame(minWidth: 32, minHeight: 44)
                                }.buttonStyle(.borderless)
                                    .accessibilityLabel(L("txt.select_heading") + ": " + row.title)
                                    .accessibilityValue(L(model.excluded.contains(row.id) ? "txt.excluded" : "txt.included"))
                                    .accessibilityIdentifier("txt-toggle-\(row.id)")
                                NavigationLink { TxtExcerptView(row: row) } label: {
                                    Text(row.title).foregroundStyle(model.excluded.contains(row.id) ? .secondary : .primary).lineLimit(2)
                                }.accessibilityIdentifier("txt-preview-\(row.id)")
                            }
                        }
                        if model.candidates.isEmpty { ForEach(Array(analysis.chapters.enumerated()), id: \.offset) { _, row in NavigationLink(row.title) { TxtExcerptView(row: row) } } }
                    } header: { Text(L("txt.preview_title")) } footer: { Text(L("txt.review_hint")) }
                }
            }
            .disabled(model.converting)
            .navigationTitle(L("txt.import_title")).navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(L("common.cancel")) { dismiss() }.disabled(model.converting) }
                ToolbarItem(placement: .confirmationAction) {
                    Button { Task { if await model.convert() { dismiss() } } } label: {
                        if model.converting { ProgressView() } else { Text(L("txt.convert_import")) }
                    }.buttonStyle(.borderedProminent)
                        .disabled(model.analyzing || model.converting || model.failure != nil || model.analysis == nil || model.library.busy)
                        .accessibilityIdentifier("txt-convert")
                }
            }
        }
        .interactiveDismissDisabled(model.converting)
        .task { await model.prepare() }
        .onChange(of: model.heuristic) { _, _ in model.refresh(reset: true) }
        .onChange(of: model.regex) { _, _ in model.refresh(reset: true) }
        .onDisappear { model.close() }
    }
}
private struct TxtExcerptView: View {
    let row: TxtChapterPreview
    var body: some View {
        Form { Section { Text(row.excerpt.isEmpty ? row.title : row.excerpt).textSelection(.enabled) } header: { Text(L("txt.excerpt")) } }
            .navigationTitle(row.title).navigationBarTitleDisplayMode(.inline)
    }
}
private struct TxtSourceLinesView: View {
    @ObservedObject var model: TxtImportModel
    @State private var query = ""
    @State private var rows: [TxtChapterPreview] = []
    @State private var searching = true
    @State private var error: String?
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        List {
            Section {
                if searching { ProgressView() }
                if let error { Text(error).foregroundStyle(.red) }
                ForEach(Array(rows.prefix(80))) { row in
                    Button { model.add(row); dismiss() } label: {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(row.title).foregroundStyle(Color(uiColor: .label)).lineLimit(3)
                            Text(row.excerpt).font(.footnote).foregroundStyle(Color(uiColor: .secondaryLabel)).lineLimit(2)
                        }.padding(.vertical, 4)
                    }.accessibilityIdentifier("txt-source-\(row.id)")
                }
                if !searching && rows.isEmpty && error == nil { Text(L("txt.no_matches")).foregroundStyle(.secondary) }
            } header: { Text(L("txt.add_hint")).textCase(nil) }
              footer: { if rows.count > 80 { Text(L("txt.search_limit")) } }
        }
        .navigationTitle(L("txt.add_heading")).navigationBarTitleDisplayMode(.inline)
        .searchable(text: $query, prompt: L("txt.search_source"))
        .task(id: query) {
            searching = true; error = nil
            do {
                try await Task.sleep(for: .milliseconds(300))
                guard let file = model.file else { return }
                let result = try await model.library.store.txtLines(file, query: query)
                try Task.checkCancellation()
                rows = result; searching = false
            } catch is CancellationError { }
            catch { self.error = error.localizedDescription; searching = false }
        }
    }
}

