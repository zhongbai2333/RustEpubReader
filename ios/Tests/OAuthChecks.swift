import Foundation

final class MockGitHub: URLProtocol {
    static var handler: ((URLRequest) throws -> (Int, [String: String], Data))?
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        do {
            let (status, headers, data) = try Self.handler!(request)
            let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data); client?.urlProtocolDidFinishLoading(self)
        } catch { client?.urlProtocol(self, didFailWithError: error) }
    }
    override func stopLoading() {}
}
@main struct OAuthChecks {
    static func json(_ value: [String: Any]) throws -> Data { try JSONSerialization.data(withJSONObject: value) }
    static func main() async throws {
        precondition(GitHubOAuth.hasRepoScope("read:user, repo"))
        precondition(!GitHubOAuth.hasRepoScope("public_repo,repo:status"))
        let outcomes: [(String, PollOutcome)] = [("authorization_pending", .pending), ("slow_down", .slowDown), ("expired_token", .expired), ("access_denied", .denied)]
        for (error, expected) in outcomes {
            let actual = try GitHubOAuth.outcome(json(["error":error])); precondition(actual == expected)
        }
        let tokenOutcome = try GitHubOAuth.outcome(json(["access_token":"synthetic-test-token"]))
        precondition(tokenOutcome == .token("synthetic-test-token"))
        let config = URLSessionConfiguration.ephemeral; config.protocolClasses = [MockGitHub.self]
        let oauth = GitHubOAuth(session: URLSession(configuration: config))
        MockGitHub.handler = { request in
            precondition(request.url!.absoluteString == "https://github.com/login/device/code")
            precondition(request.httpMethod == "POST")
            // URLSession converts body to a stream for URLProtocol on some systems.
            if let body = request.httpBody { precondition(String(decoding: body, as: UTF8.self).contains("scope=repo")) }
            return (200, [:], try json(["device_code":"test-device", "user_code":"TEST-CODE", "verification_uri":"https://github.com/login/device", "expires_in":900, "interval":5]))
        }
        let code = try await oauth.begin(); precondition(code.user_code == "TEST-CODE")
        MockGitHub.handler = { request in
            precondition(request.url!.absoluteString == "https://api.github.com/user")
            precondition(request.value(forHTTPHeaderField: "Authorization") == "Bearer synthetic-test-token")
            return (200, ["X-OAuth-Scopes":"repo"], try json(["login":"ios-test"]))
        }
        let credential = try await oauth.verify(token: "synthetic-test-token")
        precondition(credential.owner == "ios-test")
        MockGitHub.handler = { _ in (200, ["X-OAuth-Scopes":"public_repo"], try json(["login":"ios-test"])) }
        do { _ = try await oauth.verify(token: "synthetic-test-token"); preconditionFailure("Missing repo scope must fail") }
        catch OAuthIssue.missingScope {}
        MockGitHub.handler = { _ in (401, [:], Data()) }
        do { _ = try await oauth.verify(token: "synthetic-test-token"); preconditionFailure("Expired token must fail") }
        catch OAuthIssue.http(401) {}
        MockGitHub.handler = { _ in
            (200, [:], try json(["device_code":"test-device", "user_code":"TEST-CODE", "verification_uri":"https://evil.example/steal", "expires_in":900, "interval":5]))
        }
        do { _ = try await oauth.begin(); preconditionFailure("Unexpected authorization site must fail") }
        catch OAuthIssue.invalidResponse {}
        let cancelled = Task { try await oauth.authorize(code) }; cancelled.cancel()
        do { _ = try await cancelled.value; preconditionFailure("Cancelled flow must never return credentials") }
        catch is CancellationError {}
        print("PASS: device request, exact scope, account verification, expired credentials, trusted verification URL, poll outcomes and cancelled flow; no live GitHub requests")
    }
}
