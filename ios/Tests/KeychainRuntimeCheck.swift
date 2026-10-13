#if DEBUG
import Foundation
// Explicit debug-only launch check. Never reads/writes the real OAuth item.
enum KeychainRuntimeCheck {
    static func run() {
        let service = "com.zhongbai233.epub.reader.github.verification"
        var result: [String: Any] = ["ok":false]
        do {
            let test = GitHubCredential(token: "synthetic-no-access", owner: "test-only", scopes: "repo")
            try TokenKeychain.save(test, service: service)
            let loaded = try TokenKeychain.load(service: service)
            guard loaded?.token == test.token && loaded?.owner == test.owner else { throw CoreFailure(message: "Keychain round trip failed") }
            try TokenKeychain.delete(service: service)
            guard try TokenKeychain.load(service: service) == nil else { throw CoreFailure(message: "Keychain delete failed") }
            result = ["ok":true,"checks":["isolated write", "read", "delete", "missing item"]]
        } catch { result["error"] = (error as? CoreFailure)?.message ?? error.localizedDescription }
        let output = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("keychain-verification.json")
        if let data = try? JSONSerialization.data(withJSONObject: result) { try? data.write(to: output, options: .atomic) }
    }
}
#endif
