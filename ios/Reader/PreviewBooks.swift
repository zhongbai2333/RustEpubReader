import SwiftUI
import UIKit

struct LibraryEntry: Codable, Identifiable {
    let id: String
    let title: String
    let path: String
    let coverPath: String?
    let lastChapter: Int
    let lastBlock: Int?
    let lastCharOffset: Int?
    let lastOpened: UInt64
    enum CodingKeys: String, CodingKey {
        case id, title, path
        case coverPath = "cover_path"
        case lastChapter = "last_chapter", lastBlock = "last_block", lastCharOffset = "last_char_offset", lastOpened = "last_opened"
    }
    var anchor: ReadingAnchor { .init(chapter: lastChapter, block: lastBlock ?? 0, charOffset: lastCharOffset ?? 0) }
    var color: Color { Color(red: 0.33, green: 0.31, blue: 0.40) }
}
struct BookSpan: Decodable { let text: String; let style: String; let link_url: String? }
struct BookBlock: Decodable { let type: String; let core_block: Int?; let core_char_offset: Int?; let level: Int?; let spans: [BookSpan]?; let data: String?; let alt: String? }
struct BookChapter: Decodable { let title: String; let blocks: [BookBlock] }
struct BookTOC: Decodable { let title: String; let chapter_index: Int }
struct PreviewBook: Decodable, Identifiable {
    let entry: LibraryEntry
    let chapters: [BookChapter]
    let toc: [BookTOC]
    let cover: String?
    var id: String { entry.id }
    var title: String { entry.title }
    var chapterTitles: [String] { chapters.map(\.title) }
}
struct CloudProgress: Decodable, Identifiable {
    let book_hash: String
    let chapter: Int
    let block: Int
    let char_offset: Int
    let chapter_title: String?
    let timestamp: UInt64
    var id: String { "\(book_hash)-\(timestamp)" }
    var anchor: ReadingAnchor { .init(chapter: chapter, block: block, charOffset: char_offset) }
}
struct BookTransferFailure: Decodable, Identifiable {
    let hash: String; let title: String; let direction: String; let error: String; let attempts: Int; let retry_at: UInt64
    var id: String { direction + hash }
}
struct SyncSummary: Decodable {
    let uploaded: Int; let downloaded: Int; let skipped: Int; let offer: CloudProgress?
    let failed: [BookTransferFailure]?; let next_retry_after: UInt64?; let terminal_error: String?
}

@MainActor final class PreviewLibrary: ObservableObject {
    @Published var books: [LibraryEntry] = []
    @Published var anchors: [String: ReadingAnchor] = [:]
    @Published var busy = false
    @Published var error: String?
    @Published var errorTitle = "无法完成操作"
    @Published var offer: CloudProgress?
    var openPath = ""
    let client: CoreClient
    let store: CoreStore
    private var saveTask: Task<Void, Never>?
    var recent: String { recentBooks.first?.id ?? "" }
    var recentBooks: [LibraryEntry] { books.sorted { $0.lastOpened > $1.lastOpened } }
    init() {
        let documents = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let root = documents.appendingPathComponent("ReaderLibrary", isDirectory: true)
        let device = UserDefaults.standard.string(forKey: "reader.device") ?? UUID().uuidString
        UserDefaults.standard.set(device, forKey: "reader.device")
        client = .init(root: root.path, device: device); store = CoreStore(client: client)
        Task { await reload() }
    }
    func reload() async {
        do {
            let loaded = try await store.library(); books = loaded
            anchors = Dictionary(uniqueKeysWithValues: loaded.map { ($0.id, $0.anchor) })
        } catch { fail(error, title: "无法读取书库") }
    }
    func importBook(_ url: URL) async {
        busy = true; defer { busy = false }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do { _ = try await store.importBook(path: url.path); await reload() }
        catch { fail(error, title: "无法导入书籍") }
    }
    func remove(_ ids: Set<String>) async -> Bool {
        guard !busy else { return false }
        busy = true; defer { busy = false }
        await flush()
        do { try await store.remove(ids: Array(ids)); await reload(); return true }
        catch { fail(error, title: "无法完成操作"); return false }
    }
    func open(_ entry: LibraryEntry) async -> PreviewBook? {
        guard !busy else { return nil }
        busy = true; defer { busy = false }
        do {
            await flush()
            let book = try await store.open(id: entry.id)
            openPath = book.entry.path
            save(book.entry.anchor, for: book)
            return book
        } catch { fail(error, title: "无法打开书籍"); return nil }
    }
    func save(_ anchor: ReadingAnchor, for book: PreviewBook) {
        anchors[book.id] = anchor
        let previous = saveTask, store = self.store
        let backgroundID = UIApplication.shared.beginBackgroundTask(withName: "Save reading position")
        saveTask = Task {
            await previous?.value
            defer { if backgroundID != .invalid { UIApplication.shared.endBackgroundTask(backgroundID) } }
            do {
                let saved = try await store.save(id: book.id, anchor: anchor)
                if let index = books.firstIndex(where: { $0.id == book.id }) { books[index] = saved }
            } catch { fail(error, title: "无法保存阅读进度") }
        }
    }
    func flush() async { await saveTask?.value }
    func closed() { openPath = ""; offer = nil; Task { try? await store.closed() } }
    func fail(_ failure: Error, title: String) { errorTitle = title; error = failure.localizedDescription }
}
