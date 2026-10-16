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
struct BookSpan: Decodable, Equatable { let text: String; let style: String; let link_url: String? }
struct NativeDimension: Decodable, Sendable, Equatable {
    let px: Double; let em: Double; let percent: Double
    func resolve(_ font: Double, _ extent: Double) -> Double { max(0,px+em*font+(extent.isFinite ? percent*extent : 0)) }
}
struct NativeInline: Decodable, Sendable, Equatable {
    let start: Int; let end: Int; let size: Double?; let color: String?; let family: String?; let bold: Bool?; let italic: Bool?; let underline: Bool; let strike: Bool; let hidden: Bool; let baseline: Double; let small_caps: Bool; let note: String?
}
struct NativeList: Decodable, Sendable, Equatable {let start:Int;let end:Int;let text:String;let depth:Int;let marker:String}
struct NativeRuby: Decodable, Sendable, Equatable { let start: Int; let end: Int; let text: String }
struct NativeCell: Decodable, Sendable, Equatable { let start: Int; let end: Int; let text: String; let header: Bool; let span: Int; let align: String? }
struct NativeTable: Decodable, Sendable, Equatable { let cells: [NativeCell]; let columns: Int }
struct NativeNote: Decodable, Identifiable { let text: String; let href: String; var id: String { href } }
struct NativeStyle: Decodable, Sendable, Equatable {
    var size_px: Double? = nil; var inline: [NativeInline]? = nil; var ruby: [NativeRuby]? = nil; var table: NativeTable? = nil
    var list: [NativeList]? = nil
    var list_depth: Int? = nil; var quote: Bool? = nil; var pre: Bool? = nil
    var inset_left: NativeDimension? = nil; var inset_right: NativeDimension? = nil
    var padding_left: NativeDimension? = nil; var padding_right: NativeDimension? = nil; var padding_top: NativeDimension? = nil; var padding_bottom: NativeDimension? = nil
    var margin_top: NativeDimension? = nil; var margin_bottom: NativeDimension? = nil; var indent_length: NativeDimension? = nil
    var image_width: NativeDimension? = nil; var image_height: NativeDimension? = nil; var image_max_width: NativeDimension? = nil; var image_max_height: NativeDimension? = nil
    var align: String?; var size: Double?; var line: Double?; var indent: Double?; var before: Double?; var after: Double?; var color: String?; var family: String?; var bold: Bool; var italic: Bool; var hidden: Bool; var action: Int?; var text: String?
}
struct NativePresentation: Decodable, Sendable { let styles: [NativeStyle]; let errors: [String] }
struct BookBlock: Decodable, Equatable { var native_style: NativeStyle? = nil; let type: String; let core_block: Int?; let core_char_offset: Int?; let level: Int?; let spans: [BookSpan]?; let data: String?; let alt: String? }
struct BookChapter: Decodable, Equatable { let title: String; let blocks: [BookBlock] }
struct BookTOC: Decodable { let title: String; let chapter_index: Int }
// The chapter catalogue stays small; decoded bodies are retained in a bounded
// cache shared by the reader, search and speech without copying the publication.
final class BookChapterSource: @unchecked Sendable {
    let client: CoreClient; let id: String
    private let lock = NSLock()
    private var cache: [Int: BookChapter] = [:]
    private var order: [Int] = []
    private var costs: [Int:Int] = [:]
    init(client: CoreClient, id: String) { self.client = client; self.id = id }
    func chapter(_ index: Int) throws -> BookChapter {
        lock.lock(); defer { lock.unlock() }
        if let value = cache[index] { order.removeAll {$0 == index}; order.append(index); return value }
        let value: BookChapter = try client.call("chapter", ["id":id,"chapter":index])
        cache[index] = value; order.append(index)
        costs[index]=value.blocks.reduce(0) {sum,block in sum+192+(block.data?.utf8.count ?? 0)+(block.spans ?? []).reduce(0) {$0+$1.text.utf8.count+96} }
        while order.count > 1 && (order.count > 32 || costs.values.reduce(0,+)>16*1024*1024) {
            let oldest=order.removeFirst();cache.removeValue(forKey:oldest);costs.removeValue(forKey:oldest)
        }
        return value
    }
    var residentCount: Int { lock.lock(); defer {lock.unlock()}; return cache.count }
}
struct PreviewBook: Decodable, Identifiable {
    let entry: LibraryEntry
    // Catalogue entries contain titles, with no body for a chapter-backed book.
    let chapters: [BookChapter]
    let toc: [BookTOC]
    let cover: String?
    let chapterSource: BookChapterSource?
    var id: String { entry.id }
    var title: String { entry.title }
    var chapterTitles: [String] { chapters.map(\.title) }
    var loadsChapters: Bool { chapterSource != nil }
    func chapter(at index: Int) throws -> BookChapter {
        guard chapters.indices.contains(index) else { throw CoreFailure(message:"Reading chapter is out of range") }
        return try chapterSource?.chapter(index) ?? chapters[index]
    }
    init(entry: LibraryEntry, chapters: [BookChapter], toc: [BookTOC], cover: String?) {
        self.entry=entry;self.chapters=chapters;self.toc=toc;self.cover=cover;chapterSource=nil
    }
    enum CodingKeys: String,CodingKey {case entry,chapters,toc,cover;case loading="chapter_loading"}
    init(from decoder: Decoder) throws {
        let values=try decoder.container(keyedBy:CodingKeys.self)
        entry=try values.decode(LibraryEntry.self,forKey:.entry)
        chapters=try values.decode([BookChapter].self,forKey:.chapters)
        toc=try values.decode([BookTOC].self,forKey:.toc)
        cover=try values.decodeIfPresent(String.self,forKey:.cover)
        if try values.decodeIfPresent(Bool.self,forKey:.loading) == true {
            guard let client=decoder.userInfo[.readerCoreClient] as? CoreClient else {throw CoreFailure(message:"Missing chapter source")}
            chapterSource=BookChapterSource(client:client,id:entry.id)
        } else {chapterSource=nil}
    }
}
struct CloudProgress: Decodable, Identifiable {
    let book_hash: String
    let chapter: Int
    let block: Int
    let char_offset: Int
    let chapter_title: String?
    let timestamp: UInt64
    var id: String { "\(book_hash)-\(timestamp)-\(chapter)-\(block)-\(char_offset)" }
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

struct ArchivedEntry: Decodable, Identifiable {
    struct Book: Decodable { let hash: String; let title: String; let size: UInt64 }
    let book: Book; let revision: UInt64; let local_available: Bool
    var id: String { book.hash }
}
struct SyncMeter: Decodable {
    let phase: String; let completed: Int; let total: Int; let title: String
    var bytes: Bool? = nil
    var amount: String {
        if bytes == true {return ByteCountFormatter.string(fromByteCount:Int64(completed),countStyle:.file) + " / " + ByteCountFormatter.string(fromByteCount:Int64(total),countStyle:.file)}
        return L("cloud.processed",completed,total)
    }
    var label: String {
        switch phase {
        case "progress": L("cloud.phase_progress")
        case "uploading": L("cloud.phase_uploading")
        case "downloading": L("cloud.phase_downloading")
        case "deleting": L("cloud.phase_deleting")
        case "saving": L("cloud.phase_saving")
        default: L("cloud.phase_preparing")
        }
    }
}

@MainActor final class PreviewLibrary: ObservableObject {
    @Published var books: [LibraryEntry] = [] {
        didSet { recentBooks = books.sorted { $0.lastOpened > $1.lastOpened } }
    }
    @Published var anchors: [String: ReadingAnchor] = [:]
    @Published var archived: [ArchivedEntry] = []
    @Published var busy = false
    @Published var error: String?
    @Published var errorTitle = L("common.error_operation_failed")
    @Published var offer: CloudProgress?
    private(set) var progressRevision: UInt64 = 0
    var openPath = ""
    let client: CoreClient
    let store: CoreStore
    let covers: ShelfCoverStore
    lazy var bookSharing = BookSharing(library: self)
    @Published var receivingSharedBooks = false
    private var saveTask: Task<Void, Never>?
    private var saveRevisions: [String:UInt64] = [:]
    var recent: String { recentBooks.first?.id ?? "" }
    private(set) var recentBooks: [LibraryEntry] = []
    init() {
        let documents = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let root = documents.appendingPathComponent("ReaderLibrary", isDirectory: true)
        var temporary = root.appendingPathComponent(".book-share-tmp", isDirectory: true)
        try? FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        var values = URLResourceValues(); values.isExcludedFromBackup = true
        try? temporary.setResourceValues(values)
        let device = UserDefaults.standard.string(forKey: "reader.device") ?? UUID().uuidString
        UserDefaults.standard.set(device, forKey: "reader.device")
        client = .init(root: root.path, device: device); store = CoreStore(client: client)
        covers = ShelfCoverStore(client: client)
        Task { await reload() }
    }
    func reload() async {
        do {
            let loaded = try await store.library(); books = loaded
            archived = try await store.archiveList()
            anchors = Dictionary(uniqueKeysWithValues: loaded.map { ($0.id, $0.anchor) })
        } catch { fail(error, title: L("library.error_read_failed")) }
    }
    func importBook(_ url: URL) async {
        busy = true; defer { busy = false }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do { _ = try await store.importBook(path: url.path); await reload() }
        catch { fail(error, title: L("library.error_import_failed")) }
    }
    func remove(_ ids: Set<String>) async -> Bool {
        guard !busy else { return false }
        busy = true; defer { busy = false }
        await flush()
        do { try await store.archive(ids: Array(ids)); await reload(); return true }
        catch { fail(error, title: L("common.error_operation_failed")); return false }
    }
    func archiveAction(_ entry: ArchivedEntry, action: String) async {
        guard !busy else { return }; busy = true; defer { busy = false }
        do { try await store.archiveAction(hash: entry.id, action: action); await reload() }
        catch { fail(error, title: L("common.error_operation_failed")) }
    }
    func rename(_ entry: LibraryEntry, title: String) async -> Bool {
        guard !busy else { return false }
        busy = true; defer { busy = false }
        await flush()
        do { _ = try await store.rename(id: entry.id, title: title); await reload(); return true }
        catch { fail(error, title: L("common.error_operation_failed")); return false }
    }
    func exportBooks(_ ids: [String], directory: URL) async throws -> [URL] {
        try await store.exportBooks(ids: ids, directory: directory.path).map { URL(fileURLWithPath: $0) }
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
        } catch { fail(error, title: L("reader.error_open_failed")); return nil }
    }
    func save(_ anchor: ReadingAnchor, for book: PreviewBook) {
        guard books.contains(where: {$0.id==book.id}) else { return }
        if anchors[book.id] != anchor { progressRevision &+= 1 }
        anchors[book.id] = anchor
        saveRevisions[book.id,default:0] &+= 1
        let saveRevision=saveRevisions[book.id]!
        let editedAt=UInt64(Date().timeIntervalSince1970*1000)
        let previous = saveTask, store = self.store
        let backgroundID = UIApplication.shared.beginBackgroundTask(withName: "Save reading position")
        saveTask = Task {
            await previous?.value
            defer { if backgroundID != .invalid { UIApplication.shared.endBackgroundTask(backgroundID) } }
            guard saveRevisions[book.id] == saveRevision else { return }
            do {
                let saved = try await store.save(id: book.id, anchor: anchor, editedAt: editedAt)
                if let index = books.firstIndex(where: { $0.id == book.id }) { books[index] = saved }
                if anchors[book.id] == anchor { anchors[book.id] = saved.anchor }
            } catch { fail(error, title: L("reader.error_save_progress_failed")) }
        }
    }
    func flush() async { await saveTask?.value }
    func closed() { openPath = ""; offer = nil; Task { try? await store.closed() } }
    func fail(_ failure: Error, title: String) { errorTitle = title; error = (failure as? CoreFailure).map {ReaderErrors.display($0.message)} ?? L("common.error_operation_incomplete") }
}
