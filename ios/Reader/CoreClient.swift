import Foundation
import UIKit

extension CodingUserInfoKey { static let readerCoreClient = CodingUserInfoKey(rawValue:"readerCoreClient")! }

struct CoreFailure: LocalizedError { let message: String; var errorDescription: String? { ReaderErrors.display(message) } }
struct CoreEnvelope<T: Decodable>: Decodable { let ok: Bool; let result: T?; let error: String? }
struct CoreClient: Sendable {
    let root: String
    let device: String
    // Evaluation-only; the production chapter/image cache is unchanged.
    func resourceImage(id: String, reference: String) throws -> Data {
        let request: [String:Any] = ["root":root,"device":device,"id":id,"reference":reference]
        let input = try JSONSerialization.data(withJSONObject:request)
        let handle = String(decoding:input,as:UTF8.self).withCString {reader_ios_image_read($0)}
        guard let handle else {throw CoreFailure(message:"Image resource unavailable")}
        defer {reader_ios_image_free(handle)}
        let length=reader_ios_image_length(handle)
        guard length>0, length<=32*1024*1024, let bytes=reader_ios_image_data(handle) else {throw CoreFailure(message:"Invalid image resource")}
        return Data(bytes:bytes,count:length)
    }
    func call<T: Decodable>(_ operation: String, _ fields: [String: Any] = [:], as: T.Type = T.self) throws -> T {
        var request = fields; request["op"] = operation; request["root"] = root; request["device"] = device
        let input = try JSONSerialization.data(withJSONObject: request)
        let result = String(decoding: input, as: UTF8.self).withCString { reader_ios_request($0) }
        guard let result else { throw CoreFailure(message: "Reader core returned no result") }
        defer { reader_ios_free(result) }
        let data = Data(bytes: result, count: strlen(result))
        let decoder=JSONDecoder(); decoder.userInfo[.readerCoreClient]=self
        let envelope = try decoder.decode(CoreEnvelope<T>.self, from: data)
        guard envelope.ok, let value = envelope.result else { throw CoreFailure(message: envelope.error ?? "Reader core failed") }
        return value
    }
}
struct AnnotationMarkdownDocument: Decodable { let filename: String; let markdown: String }
struct LibraryBackupSummary: Decodable, Sendable {
    let manifest_sha256: String
    let schema_version: Int; let books: Int; let archived: Int; let files: Int; let file_bytes: UInt64; let created_at: UInt64
}
struct LibraryBackupProgress: Decodable, Sendable {
    let running: Bool; let phase: String; let completed: Int; let total: Int
}

// All local mutations and sync application run off the UI thread, in order.
actor CoreStore {
    let client: CoreClient
    init(client: CoreClient) { self.client = client }
    func closed() throws { let _: [String: Bool] = try client.call("closed") }
    func library() throws -> [LibraryEntry] { try client.call("library") }
    func backup(action: String, path: String, expected: String?=nil) throws -> LibraryBackupSummary {
        var fields: [String:Any] = ["action":action,"path":path]
        if let expected {fields["expected_manifest"]=expected}
        return try client.call("backup",fields)
    }
    func importBook(path: String) throws -> LibraryEntry { try client.call("import", ["path": path]) }
    func annotationDetails(id:String,key:String) throws -> AnnotationSyncDetails {try client.call("annotation_tools",["action":"details","id":id,"key":key])}
    func exportAnnotations(id: String) throws -> URL {
        let document: AnnotationMarkdownDocument = try client.call("annotation_tools",["action":"export","id":id])
        guard URL(fileURLWithPath:document.filename).lastPathComponent == document.filename, document.filename.hasSuffix(".md") else {throw CoreFailure(message:"Invalid annotation export filename")}
        let directory=FileManager.default.temporaryDirectory.appendingPathComponent("AnnotationExports",isDirectory:true).appendingPathComponent(UUID().uuidString,isDirectory:true)
        try FileManager.default.createDirectory(at:directory,withIntermediateDirectories:true)
        let url=directory.appendingPathComponent(document.filename)
        try document.markdown.write(to:url,atomically:true,encoding:.utf8)
        return url
    }
    func annotations(id: String, fields: [String: Any]) throws -> ReaderAnnotations { try client.call("annotations", fields.merging(["id": id]) { _, new in new }) }
    func remove(ids: [String]) throws { let _: [String: Int] = try client.call("remove", ["ids": ids]) }
    func archive(ids: [String]) throws {
        let entries: [LibraryEntry] = try client.call("library")
        let selected = ids.compactMap { id in entries.first(where: { $0.id == id })?.path }
        guard selected.count == ids.count else { throw CoreFailure(message: "Book is no longer in the library") }
        let _: [String:Int] = try client.call("archive", ["action":"archive","paths":selected])
    }
    func archiveList() throws -> [ArchivedEntry] { try client.call("archive") }
    func archiveAction(hash: String, action: String) throws { let _: [String:Int] = try client.call("archive",["action":action,"hash":hash]) }
    func rename(id: String, title: String) throws -> LibraryEntry { try client.call("rename", ["id": id, "title": title]) }
    func exportBooks(ids: [String], directory: String, mode:String="original") throws -> [String] { try client.call("export_books", ["ids": ids, "directory": directory,"mode":mode]) }
    func open(id: String, token: UInt64? = nil) throws -> PreviewBook {
        var fields: [String:Any] = ["id":id]
        if let token { fields["open_token"] = token }
        let book: PreviewBook = try client.call("open", fields)
        let chapter=min(max(0,book.entry.lastChapter),max(0,book.chapters.count-1))
        for index in max(0,chapter-1)..<min(book.chapters.count,chapter+2) { _ = try book.chapter(at:index, openToken: token) }
        return book
    }
    func save(id: String, anchor: ReadingAnchor, editedAt: UInt64) throws -> LibraryEntry {
        try client.call("save", ["id": id, "chapter": anchor.chapter, "block": anchor.block, "char_offset": anchor.charOffset,"edited_at":editedAt])
    }
    func apply(epoch: UInt64, openPath: String) throws -> SyncSummary { try client.call("apply_sync", ["epoch": epoch, "open_path": openPath]) }
}

// Independent serial worker: shelf extraction/decode never queues ahead of an
// open/save/sync operation on CoreStore and never performs disk IO in View.body.
actor ShelfCoverStore {
    private let client: CoreClient
    private let cache = NSCache<NSString, UIImage>()
    init(client: CoreClient) {
        self.client = client
        cache.countLimit = 64; cache.totalCostLimit = 64 * 1024 * 1024
    }
    func image(id: String, path: String) -> UIImage? {
        if Task.isCancelled { return nil }
        let attributes = try? FileManager.default.attributesOfItem(atPath: path)
        let modified = (attributes?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
        let key = "\(id):\(path):\(attributes?[.size] ?? 0):\(modified)" as NSString
        if let image = cache.object(forKey: key) { return image }
        #if READER_PERFORMANCE_CHECK
        ShelfProbeMetrics.shared.requested(id)
        #endif
        struct Response: Decodable { let data: String? }
        guard let reply: Response = try? client.call("shelf_cover", ["id": id]),
              let encoded = reply.data, let data = Data(base64Encoded: encoded),
              let source = UIImage(data: data), let image = source.preparingForDisplay() else { return nil }
        cache.setObject(image, forKey: key, cost: Int(image.size.width * image.size.height * 4))
        return image
    }
}
