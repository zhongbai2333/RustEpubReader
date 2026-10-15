import Foundation
import CryptoKit
import OnnxRuntimeBindings
import SwiftUI

struct SpellingSuggestion: Identifiable {
    var id: String { "\(block)-\(offset)-\(corrected)" }
    let block: Int
    let offset: Int
    let original: String
    let corrected: String
    let confidence: Float
}
actor MacBERTEngine {
    private var environment: ORTEnv?
    private var session: ORTSession?
    private var vocab: [String] = []
    private var ids: [String: Int64] = [:]
    func unload() { session = nil; environment = nil; vocab = []; ids = [:] }
    func load(_ directory: URL) throws {
        let words = try String(contentsOf: directory.appendingPathComponent("csc-vocab.txt"), encoding: .utf8).split(separator: "\n", omittingEmptySubsequences: true).map { String($0).trimmingCharacters(in: .newlines) }
        guard words.count > 1000, words.count < 100_000 else { throw URLError(.cannotParseResponse) }
        let env = try ORTEnv(loggingLevel: .error)
        let options = try ORTSessionOptions(); try options.setIntraOpNumThreads(2)
        let loaded = try ORTSession(env: env, modelPath: directory.appendingPathComponent("csc-macbert-int8.onnx").path, sessionOptions: options)
        environment = env; session = loaded; vocab = words
        ids = Dictionary(words.enumerated().map { ($1, Int64($0)) }, uniquingKeysWith: { first, _ in first })
    }
    func check(_ blocks: [(Int, String)], threshold: Float) throws -> [SpellingSuggestion] {
        guard let session else { return [] }
        var suggestions: [SpellingSuggestion] = []
        for (block, text) in blocks {
            try Task.checkCancellation()
            let scalars = Array(text.unicodeScalars)
            // Keep all scalar offsets, including whitespace and non-BMP characters.
            for begin in stride(from: 0, to: scalars.count, by: 126) {
                try Task.checkCancellation()
                let end = min(scalars.count, begin+126)
                var input: [Int64] = [ids["[CLS]"] ?? 101], offsets: [Int?] = [nil]
                for i in begin..<end where !CharacterSet.whitespacesAndNewlines.contains(scalars[i]) {
                    input.append(ids[String(scalars[i]).lowercased()] ?? ids["[UNK]"] ?? 100); offsets.append(i)
                }
                input.append(ids["[SEP]"] ?? 102); offsets.append(nil)
                var mask = Array(repeating: Int64(1), count: input.count)
                while input.count < 128 { input.append(0); mask.append(0); offsets.append(nil) }
                func tensor(_ values: [Int64]) throws -> ORTValue {
                    let bytes = values.withUnsafeBytes { NSMutableData(bytes: $0.baseAddress!, length: $0.count) }
                    return try ORTValue(tensorData: bytes, elementType: .int64, shape: [1, 128])
                }
                let inputs = ["input_ids": try tensor(input), "attention_mask": try tensor(mask), "token_type_ids": try tensor(Array(repeating: 0, count: 128))]
                let names = try session.outputNames(); guard let name = names.first else { throw URLError(.cannotParseResponse) }
                let output = try session.run(withInputs: inputs, outputNames: [name], runOptions: nil)
                guard let value = output[name] else { throw URLError(.cannotParseResponse) }
                let shape = try value.tensorTypeAndShapeInfo().shape.map(\.intValue)
                guard shape == [1, 128, vocab.count] else { throw URLError(.cannotParseResponse) }
                let data = try value.tensorData()
                guard data.length == 128*vocab.count*MemoryLayout<Float>.size else { throw URLError(.cannotParseResponse) }
                let logits = data.bytes.bindMemory(to: Float.self, capacity: data.length/4)
                for token in 1..<128 {
                    guard let offset = offsets[token], input[token] != (ids["[UNK]"] ?? 100) else { continue }
                    let original = String(scalars[offset])
                    guard Self.cjk(scalars[offset].value), !Self.protected.contains(original) else { continue }
                    let base = token*vocab.count
                    var best = 0
                    for i in 1..<vocab.count where logits[base+i] > logits[base+best] { best = i }
                    guard best != Int(input[token]) else { continue }
                    let maximum = logits[base+best]
                    var sum: Float = 0
                    for i in 0..<vocab.count { sum += exp(logits[base+i]-maximum) }
                    let predicted = 1/sum, old = exp(logits[base+Int(input[token])]-maximum)/sum
                    let corrected = vocab[best]
                    guard predicted >= threshold, predicted-old >= 0.30, corrected.unicodeScalars.count == 1, let scalar = corrected.unicodeScalars.first, Self.cjk(scalar.value), !Self.confused.contains(where: { $0.contains(original) && $0.contains(corrected) }) else { continue }
                    if offset > 0 && scalars[offset-1] == scalar || offset+1 < scalars.count && scalars[offset+1] == scalar { continue }
                    suggestions.append(.init(block: block, offset: offset, original: original, corrected: corrected, confidence: predicted))
                }
            }
        }
        return suggestions
    }
    static func cjk(_ value: UInt32) -> Bool { (0x4e00...0x9fff).contains(value) || (0x3400...0x4dbf).contains(value) || (0xf900...0xfaff).contains(value) }
    static let confused: [Set<String>] = [Set(["他","她","它","牠","祂"]),Set(["的","得","地"]),Set(["做","作"]),Set(["哪","那"]),Set(["在","再"])]
    static let protected = Set("吗吧呢啊呀哇哦嗯喔噢啦嘛咯喽嘞罢咧的得地了过着么个们这那就都也又才把被让给向往从到为而且或与及我你他她它谁啥哪几多些别是其悉有没有不会能要去来说看想知道".map(String.init))
}
@MainActor final class ChineseCorrection: ObservableObject {
    @Published var busy = false
    @Published var ready = false
    @Published var available = false
    @Published var error: String?
    @Published var suggestions: [SpellingSuggestion] = []
    @Published var threshold = 0.90
    let engine = MacBERTEngine()
    let directory: URL
    private var operation: Task<Void, Never>?
    private var generation = UUID()
    private static let assets: [(String, Int, String)] = [
        ("csc-macbert-int8.onnx",104067762,"11bd7da0471e0bce6306a18a3273542d0436a1329ce7c94f259b87634dc9f260"),
        ("csc-vocab.txt",109540,"45bbac6b341c319adc98a532532882e91a9cefc0329aa57bac9ae761c27b291c")]
    init(root: String) {
        directory = URL(fileURLWithPath: root).appendingPathComponent("models")
        available = Self.assets.allSatisfy { FileManager.default.fileExists(atPath: directory.appendingPathComponent($0.0).path) }
    }
    func cancel() { generation = UUID(); operation?.cancel(); operation = nil; busy = false }
    func prepare(download: Bool) {
        guard !busy else { return }; busy = true; error = nil
        let intent = UUID(); generation = intent
        operation = Task {
            do {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                if download {
                    for (name, size, hash) in Self.assets {
                        try Task.checkCancellation()
                        var request = URLRequest(url: URL(string: "https://dl.zhongbai233.com/models/\(name)")!); request.timeoutInterval = 120
                        let (temporary, response) = try await URLSession.shared.download(for: request)
                        defer { try? FileManager.default.removeItem(at: temporary) }
                        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw URLError(.badServerResponse) }
                        let valid = try await Task.detached { () -> Bool in
                            let values = try temporary.resourceValues(forKeys: [.fileSizeKey]); guard values.fileSize == size else { return false }
                            let handle = try FileHandle(forReadingFrom: temporary); defer { try? handle.close() }
                            var sha = SHA256(); while let chunk = try handle.read(upToCount: 1024*1024), !chunk.isEmpty { sha.update(data: chunk) }
                            return sha.finalize().map { String(format: "%02x", $0) }.joined() == hash
                        }.value
                        guard valid, generation == intent, !Task.isCancelled else { throw URLError(.cancelled) }
                        let target = directory.appendingPathComponent(name)
                        if FileManager.default.fileExists(atPath: target.path) { _ = try FileManager.default.replaceItemAt(target, withItemAt: temporary) }
                        else { try FileManager.default.moveItem(at: temporary, to: target) }
                        var excluded = directory; var values = URLResourceValues(); values.isExcludedFromBackup = true; try excluded.setResourceValues(values)
                    }
                }
                try Task.checkCancellation()
                try await engine.load(directory)
                guard generation == intent && !Task.isCancelled else { await engine.unload(); return }
                ready = true; available = true
            } catch { if generation == intent && !Task.isCancelled { self.error = L("common.error_model_load_failed") } }
            if generation == intent { busy = false }
        }
    }
    func check(chapter: BookChapter) {
        guard ready && !busy else { return }; busy = true; error = nil
        let intent = UUID(); generation = intent; let level = Float(threshold)
        let blocks = chapter.blocks.filter { $0.type == "Paragraph" || $0.type == "Heading" }.map { ($0.core_block ?? 0, ($0.spans ?? []).map(\.text).joined()) }
        operation = Task {
            do { let result = try await engine.check(blocks, threshold: level); if generation == intent && !Task.isCancelled { suggestions = result } }
            catch { if generation == intent && !Task.isCancelled { self.error = L("common.error_model_load_failed") } }
            if generation == intent { busy = false }
        }
    }
    func remove() {
        cancel(); busy = true; ready = false; available = false; suggestions = []
        Task { await engine.unload(); for (name,_,_) in Self.assets { try? FileManager.default.removeItem(at: directory.appendingPathComponent(name)) }; busy = false }
    }
}
