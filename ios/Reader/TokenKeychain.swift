import Foundation
import Security

enum TokenKeychain {
    static func query(service: String) -> [String: Any] { [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: "oauth"] }
    static func load(service: String = "com.zhongbai233.epub.reader.github") throws -> GitHubCredential? {
        var request = query(service: service); request[kSecReturnData as String] = true; request[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(request as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw CoreFailure(message: "Cannot read GitHub credentials from Keychain (OSStatus \(status))") }
        return try JSONDecoder().decode(GitHubCredential.self, from: data)
    }
    static func save(_ credential: GitHubCredential, service: String = "com.zhongbai233.epub.reader.github") throws {
        let data = try JSONEncoder().encode(credential)
        let attrs: [String: Any] = [kSecValueData as String: data, kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly]
        var status = SecItemUpdate(query(service: service) as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound { status = SecItemAdd(query(service: service).merging(attrs) { _, new in new } as CFDictionary, nil) }
        guard status == errSecSuccess else { throw CoreFailure(message: "Cannot save GitHub credentials to Keychain (OSStatus \(status))") }
    }
    static func delete(service: String = "com.zhongbai233.epub.reader.github") throws {
        let status = SecItemDelete(query(service: service) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw CoreFailure(message: "Cannot remove GitHub credentials from Keychain (OSStatus \(status))") }
    }
}
