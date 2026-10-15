import Foundation

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
