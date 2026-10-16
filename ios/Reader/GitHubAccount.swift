import SwiftUI

@MainActor final class GitHubAccount: ObservableObject {
    @Published private(set) var credential: GitHubCredential?
    @Published private(set) var countContributions = UserDefaults.standard.bool(forKey: "github.syncContributions")
    @Published private(set) var enabled = false
    @Published private(set) var syncing = false
    @Published private(set) var syncStatus = ""
    @Published private(set) var meter: SyncMeter?
    private var meterTask: Task<Void,Never>?
    private var nextAutoSync = Date.distantPast
    private var uploadedProgressRevision: UInt64 = 0
    @Published var loginPresented = false
    @Published private(set) var code: DeviceCode?
    @Published var error: String?
    @Published var summary: SyncSummary?
    @Published var lastSync: Date?
    private var loginTask: Task<Void, Never>?
    private var syncTask: Task<Void, Never>?
    private var timer: Task<Void, Never>?
    private var loginGeneration = 0
    private var generation = 0
    private var foreground = true
    @Published private(set) var nextAllowed = Date.distantPast
    let library: PreviewLibrary
    init(library: PreviewLibrary) {
        self.library = library
        #if DEBUG
        if CommandLine.arguments.contains("--sync-ui-preview") {
            syncing=true; syncStatus=L("cloud.books_syncing")
            meter=SyncMeter(phase:"downloading",completed:8,total:20,title:"Sync UI Preview")
            return
        }
        if CommandLine.arguments.contains("--sync-login-preview") {
            code=DeviceCode(device_code:"local-preview-only",user_code:"DEMO-0000",verification_uri:"https://github.com/login/device",expires_in:900,interval:5)
            loginPresented=true
            return
        }
        if CommandLine.arguments.contains("--book-share-preview") { return }
        #endif
        do { credential = try TokenKeychain.load() } catch { self.error = L("common.error_operation_incomplete") }
        if let credential { enabled = UserDefaults.standard.bool(forKey: "github.enabled.\(credential.owner.lowercased())") && GitHubOAuth.hasRepoScope(credential.scopes) }
        if let credential, let date = UserDefaults.standard.object(forKey: "github.lastSync.\(credential.owner.lowercased())") as? Date { lastSync = date }
    }
    func login() {
        cancelLogin(); setEnabled(false)
        loginPresented = true; error = nil; code = nil
        let intent = loginGeneration
        loginTask = Task {
            do {
                let oauth = GitHubOAuth()
                let deviceCode = try await oauth.begin()
                guard intent == loginGeneration, !Task.isCancelled else { return }
                code = deviceCode
                let verified = try await oauth.authorize(deviceCode)
                guard intent == loginGeneration, !Task.isCancelled else { return }
                try TokenKeychain.save(verified)
                library.bookSharing.cancelAuthenticatedWork()
                credential = verified; enabled = false; lastSync = nil; summary = nil
                UserDefaults.standard.set(false, forKey: "github.enabled.\(verified.owner.lowercased())")
                loginPresented = false; code = nil
            } catch {
                guard intent == loginGeneration, !Task.isCancelled else { return }
                switch error {
                case OAuthIssue.expired: self.error = L("sync.error_auth_code_expired")
                case OAuthIssue.denied: self.error = L("sync.auth_canceled")
                case OAuthIssue.missingScope: self.error = L("sync.error_github_insufficient_permissions")
                case OAuthIssue.invalidResponse: self.error = L("sync.error_github_login_failed")
                case OAuthIssue.http(let status): self.error = L("sync.error_github_login_failed_http", status)
                default: self.error = L("sync.error_github_login_failed")
                }
                code = nil
            }
        }
    }
    func cancelLogin() { loginGeneration += 1; loginTask?.cancel(); loginTask = nil; code = nil; loginPresented = false }
    func setCountContributions(_ value: Bool) {
        guard !syncing else { return }
        countContributions = value
        UserDefaults.standard.set(value, forKey: "github.syncContributions")
    }
    func setEnabled(_ value: Bool) {
        stopSync(); enabled = value && credential.map { GitHubOAuth.hasRepoScope($0.scopes) } == true
        if let credential { UserDefaults.standard.set(enabled, forKey: "github.enabled.\(credential.owner.lowercased())") }
        if enabled { syncNow(); startTimer() } else { timer?.cancel(); timer = nil }
    }
    func signOut() {
        library.bookSharing.cancelAuthenticatedWork()
        cancelLogin(); setEnabled(false)
        do {
            try TokenKeychain.delete()
            if let credential { UserDefaults.standard.removeObject(forKey: "github.lastSync.\(credential.owner.lowercased())") }
            credential = nil; summary = nil; lastSync = nil
        }
        catch { self.error = L("common.error_operation_incomplete") }
    }
    func setForeground(_ value: Bool) {
        #if DEBUG
        if CommandLine.arguments.contains("--sync-ui-preview") {
            foreground=value; syncing=value
            meter=SyncMeter(phase:"downloading",completed:8,total:20,title:"Sync UI Preview")
            return
        }
        #endif
        foreground = value
        if !value { stopSync(); timer?.cancel(); timer = nil }
        else if enabled { syncNow(); startTimer() }
    }
    private func startTimer() {
        timer?.cancel()
        timer = Task {
            while !Task.isCancelled {
                do { try await Task.sleep(for: .seconds(5)) } catch { return }
                if Date() >= nextAutoSync || (library.progressRevision != uploadedProgressRevision && (lastSync == nil || Date().timeIntervalSince(lastSync!) >= 5)) { syncNow() }
            }
        }
    }
    func stopSync() {
        meterTask?.cancel(); meterTask = nil; meter = nil
        generation += 1; reader_ios_cancel_sync(); syncTask?.cancel(); syncTask = nil; syncing = false; syncStatus = ""
    }
    func syncNow(retryFailed: Bool = false) {
        guard enabled, foreground, !syncing, let credential, Date() >= nextAllowed else { return }
        let intent = generation, epoch = reader_ios_epoch(), client = library.client
        let countContributions = self.countContributions
        syncing = true; error = nil; meter = nil
        meterTask = Task {
            while !Task.isCancelled && intent == generation {
                let value = try? await Task.detached { try client.call("sync_status", as: SyncMeter.self) }.value
                guard !Task.isCancelled, intent == generation else { return }
                meter = value
                try? await Task.sleep(for: .milliseconds(200))
            }
        }
        syncTask = Task {
            defer { if intent == generation { meterTask?.cancel(); meterTask = nil; meter = nil; syncing = false; syncTask = nil; syncStatus = "" } }
            do {
                await library.flush()
                let progressRevision=library.progressRevision
                guard intent == generation, foreground, enabled, !Task.isCancelled else { return }
                // Blocking Rust transport executes on a worker; UI remains responsive.
                struct Prepared: Decodable { let epoch: UInt64 }
                for phase in ["progress", "books"] {
                    guard intent == generation, foreground, enabled, !Task.isCancelled else { return }
                    syncStatus = phase == "progress" ? L("cloud.progress_syncing") : L("cloud.books_syncing")
                    let prepared = try await Task.detached(priority: .utility) {
                        try client.call("sync", ["epoch": epoch, "owner": credential.owner, "token": credential.token, "count_contributions": countContributions, "phase": phase, "retry_failed": retryFailed], as: Prepared.self)
                    }.value
                    guard intent == generation, foreground, enabled, self.credential?.owner == credential.owner, !Task.isCancelled else { return }
                    await library.flush()
                    guard intent == generation, foreground, enabled, !Task.isCancelled else { return }
                    let result = try await library.store.apply(epoch: prepared.epoch, openPath: library.openPath)
                    guard intent == generation else { return }
                    if phase == "progress" { uploadedProgressRevision=progressRevision }
                    summary = result; library.offer = result.offer
                    await library.reload(); lastSync = Date()
                    UserDefaults.standard.set(lastSync, forKey: "github.lastSync.\(credential.owner.lowercased())")
                    if phase == "books" { nextAutoSync = Date().addingTimeInterval(Double(result.next_retry_after ?? 300)) }
                    if let failure = result.terminal_error { throw CoreFailure(message: failure) }
                }
            } catch {
                guard intent == generation, !Task.isCancelled else { return }
                let message = (error as? CoreFailure)?.message ?? error.localizedDescription
                if message.hasPrefix("RETRY_AFTER:"), let seconds = Double(message.split(separator: ":").dropFirst().first ?? "") { nextAllowed = Date().addingTimeInterval(max(300, seconds)) }
                else { nextAllowed = Date().addingTimeInterval(30) }
                if message.hasPrefix("REAUTHORIZE:") || message.contains("login expired") || message.contains("GitHub account changed") { setEnabled(false) }
                self.error = ReaderErrors.display(message)
                nextAutoSync = nextAllowed
            }
        }
    }
}
