import Foundation

struct GitHubCredential: Codable, Sendable { let token: String; let owner: String; let scopes: String }
struct DeviceCode: Decodable, Identifiable, Sendable {
    let device_code: String
    let user_code: String
    let verification_uri: String
    let expires_in: Int
    let interval: Int
    var id: String { device_code }
}
enum OAuthIssue: Error { case expired, denied, missingScope, invalidResponse, http(Int) }
enum PollOutcome: Equatable { case pending, slowDown, token(String), expired, denied }
struct GitHubOAuth: Sendable {
    static let clientID = "Ov23liG7iXNfGOTAXXxx"
    let session: URLSession
    init(session: URLSession = URLSession(configuration: .ephemeral)) { self.session = session }
    static func hasRepoScope(_ scopes: String) -> Bool { scopes.split(separator: ",").contains { $0.trimmingCharacters(in: .whitespaces) == "repo" } }
    static func outcome(_ bytes: Data) throws -> PollOutcome {
        let object = try JSONSerialization.jsonObject(with: bytes) as? [String: Any] ?? [:]
        if let token = object["access_token"] as? String, !token.isEmpty { return .token(token) }
        switch object["error"] as? String {
        case "authorization_pending": return .pending
        case "slow_down": return .slowDown
        case "expired_token": return .expired
        case "access_denied": return .denied
        default: throw OAuthIssue.invalidResponse
        }
    }
    private func post(_ path: String, fields: [String: String]) async throws -> Data {
        var request = URLRequest(url: URL(string: "https://github.com" + path)!)
        request.httpMethod = "POST"; request.timeoutInterval = 30
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-._~"))
        request.httpBody = fields.sorted(by: { $0.key < $1.key }).map { "\($0.key.addingPercentEncoding(withAllowedCharacters: allowed)!)=\($0.value.addingPercentEncoding(withAllowedCharacters: allowed)!)" }.joined(separator: "&").data(using: .utf8)
        let (data, response) = try await session.data(for: request)
        try Task.checkCancellation()
        guard let response = response as? HTTPURLResponse else { throw OAuthIssue.invalidResponse }
        guard response.statusCode == 200 else { throw OAuthIssue.http(response.statusCode) }
        guard data.count <= 1024 * 1024 else { throw OAuthIssue.invalidResponse }
        return data
    }
    func begin() async throws -> DeviceCode {
        let data = try await post("/login/device/code", fields: ["client_id": Self.clientID, "scope": "repo"])
        let code = try JSONDecoder().decode(DeviceCode.self, from: data)
        guard code.expires_in > 0, code.expires_in <= 3600, code.interval >= 1,
              code.verification_uri == "https://github.com/login/device", !code.user_code.isEmpty, !code.device_code.isEmpty else { throw OAuthIssue.invalidResponse }
        return code
    }
    func authorize(_ code: DeviceCode) async throws -> GitHubCredential {
        let deadline = Date().addingTimeInterval(Double(code.expires_in))
        var interval = max(5, code.interval)
        while Date() < deadline {
            try await Task.sleep(for: .seconds(interval))
            guard Date() < deadline else { throw OAuthIssue.expired }
            let data = try await post("/login/oauth/access_token", fields: ["client_id": Self.clientID, "device_code": code.device_code, "grant_type": "urn:ietf:params:oauth:grant-type:device_code"])
            switch try Self.outcome(data) {
            case .pending: continue
            case .slowDown: interval += 5
            case .expired: throw OAuthIssue.expired
            case .denied: throw OAuthIssue.denied
            case .token(let token): return try await verify(token: token)
            }
        }
        throw OAuthIssue.expired
    }
    func verify(token: String) async throws -> GitHubCredential {
        var request = URLRequest(url: URL(string: "https://api.github.com/user")!)
        request.timeoutInterval = 30
        request.setValue("Bearer " + token, forHTTPHeaderField: "Authorization")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("RustEpubReader-iOS", forHTTPHeaderField: "User-Agent")
        request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
        let (data, response) = try await session.data(for: request)
        try Task.checkCancellation()
        guard let response = response as? HTTPURLResponse else { throw OAuthIssue.invalidResponse }
        guard response.statusCode == 200 else { throw OAuthIssue.http(response.statusCode) }
        let scopes = response.value(forHTTPHeaderField: "X-OAuth-Scopes") ?? ""
        guard Self.hasRepoScope(scopes) else { throw OAuthIssue.missingScope }
        let user = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        guard let login = user?["login"] as? String, !login.isEmpty, login.count <= 39, login.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") }) else { throw OAuthIssue.invalidResponse }
        return .init(token: token, owner: login, scopes: scopes)
    }
}
