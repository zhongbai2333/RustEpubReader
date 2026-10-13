#if DEBUG
import Foundation
import UIKit
enum ModelRuntimeCheck {
    static func run() {
        Task {
            let directory = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("ReaderLibrary/models")
            var report: [String: Any] = [:]
            do {
                let engine = MacBERTEngine()
                let start = Date(); try await engine.load(directory)
                report["loaded"] = true; report["load_seconds"] = Date().timeIntervalSince(start)
                let blocks = [(7,"😀 e\u{301} 我今夫很高兴。我们一起学习中文。"),
                              (8,"我的名子叫小明。我喜欢这个电恼。今天天汽很好。"),
                              (9,"少先队员因该为老人让坐。我们要保持环境整洁。")]
                let suggestions = try await engine.check(blocks, threshold: 0.8)
                report["inference_completed"] = true
                report["suggestion_count"] = suggestions.count
                report["scalar_offsets_valid"] = suggestions.allSatisfy { suggestion in
                    guard let text = blocks.first(where: { $0.0 == suggestion.block })?.1 else { return false }
                    let scalars = Array(text.unicodeScalars)
                    return scalars.indices.contains(suggestion.offset) && String(scalars[suggestion.offset]) == suggestion.original
                }
                report["inference_seconds"] = Date().timeIntervalSince(start) - (report["load_seconds"] as? Double ?? 0)
                await engine.unload()
            } catch { report["loaded"] = false; report["error_type"] = String(describing: type(of: error)) }
            let target = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("model-verification.json")
            if let data = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted,.sortedKeys]) { try? data.write(to: target, options: .atomic) }
        }
    }
}
#endif
