import Foundation
import UIKit

struct CoreFailure: LocalizedError { let message: String; var errorDescription: String? { ReaderErrors.display(message) } }
struct CoreEnvelope<T: Decodable>: Decodable { let ok: Bool; let result: T?; let error: String? }
struct CoreClient: Sendable {
    let root: String
    let device: String
    func call<T: Decodable>(_ operation: String, _ fields: [String: Any] = [:], as: T.Type = T.self) throws -> T {
        var request = fields; request["op"] = operation; request["root"] = root; request["device"] = device
        let input = try JSONSerialization.data(withJSONObject: request)
        let result = String(decoding: input, as: UTF8.self).withCString { reader_ios_request($0) }
        guard let result else { throw CoreFailure(message: "Reader core returned no result") }
        defer { reader_ios_free(result) }
        let data = Data(bytes: result, count: strlen(result))
        let envelope = try JSONDecoder().decode(CoreEnvelope<T>.self, from: data)
        guard envelope.ok, let value = envelope.result else { throw CoreFailure(message: envelope.error ?? "Reader core failed") }
        return value
    }
}
// All local mutations and sync application run off the UI thread, in order.
actor CoreStore {
    let client: CoreClient
    init(client: CoreClient) { self.client = client }
    func closed() throws { let _: [String: Bool] = try client.call("closed") }
    func library() throws -> [LibraryEntry] { try client.call("library") }
    func importBook(path: String) throws -> LibraryEntry { try client.call("import", ["path": path]) }
    func annotations(id: String, fields: [String: Any]) throws -> ReaderAnnotations { try client.call("annotations", fields.merging(["id": id]) { _, new in new }) }
    func remove(ids: [String]) throws { let _: [String: Int] = try client.call("remove", ["ids": ids]) }
    func rename(id: String, title: String) throws -> LibraryEntry { try client.call("rename", ["id": id, "title": title]) }
    func exportBooks(ids: [String], directory: String) throws -> [String] { try client.call("export_books", ["ids": ids, "directory": directory]) }
    func open(id: String) throws -> PreviewBook { try client.call("open", ["id": id]) }
    func save(id: String, anchor: ReadingAnchor) throws -> LibraryEntry {
        try client.call("save", ["id": id, "chapter": anchor.chapter, "block": anchor.block, "char_offset": anchor.charOffset])
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
        struct Response: Decodable { let data: String? }
        guard let reply: Response = try? client.call("shelf_cover", ["id": id]),
              let encoded = reply.data, let data = Data(base64Encoded: encoded),
              let source = UIImage(data: data), let image = source.preparingForDisplay() else { return nil }
        cache.setObject(image, forKey: key, cost: Int(image.size.width * image.size.height * 4))
        return image
    }
}
