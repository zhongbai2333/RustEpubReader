import SwiftUI
import SafariServices

private enum SyncPermissionCopy {
    static var purpose: String { L("cloud.permissions") }
    static var details: String { L("sync.scope_detail") }
}
struct SyncDesignView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var account: GitHubAccount
    @ObservedObject var nearby: NearbySync
    var body: some View {
        Form {
            Section {
                LabeledContent(L("sync.account"), value: account.credential?.owner ?? L("sync.not_logged_in"))
                LabeledContent(L("sync.repo"), value: "EpubReaderInfo")
                if account.credential == nil { Button(L("account.login")) { account.login() } }
                else {
                    Button(L("sync.reauthorize")) { account.login() }
                    Button(L("csc.logout"), role: .destructive) { account.signOut() }
                }
                DisclosureGroup(L("sync.permission_details")) { Text(SyncPermissionCopy.details).font(.footnote).foregroundStyle(.secondary) }
            } header: { Text(L("ui.github_sync")) } footer: { Text(SyncPermissionCopy.purpose) }
            Section {
                Toggle(L("cloud.enable"), isOn: Binding(get: { account.enabled }, set: { account.setEnabled($0) }))
                    .disabled(account.credential == nil)
                Toggle(L("sync.contributions"), isOn: Binding(get: { account.countContributions }, set: { account.setCountContributions($0) }))
                    .disabled(account.syncing)
                Text(L("sync.contributions_note")).font(.footnote).foregroundStyle(.secondary)
                DisclosureGroup(L("sync.contributions_details")) {
                    Text(L("sync.contributions_detail")).font(.footnote).foregroundStyle(.secondary)
                }
                if account.syncing {
                    HStack { ProgressView(); Text(LM(account.syncStatus)) }
                    Button(L("sync.cancel_sync")) { account.stopSync() }
                } else {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        if account.enabled && account.nextAllowed > context.date {
                            LabeledContent(L("common.retry_later")) { Text(account.nextAllowed, style: .timer).monospacedDigit() }
                        }
                        Button(L("cloud.now")) { account.syncNow() }.disabled(!account.enabled || account.nextAllowed > context.date)
                    }
                }
                if let date = account.lastSync { LabeledContent(L("sync.last_sync")) { Text(date, style: .relative) } }
                else { LabeledContent(L("sync.last_sync"), value: L("sync.never_synced")) }
                if let summary = account.summary {
                    Text(L("sync.upload_download_count", summary.uploaded, summary.downloaded))
                    if summary.skipped > 0 { Text(L("sync.skipped_count", summary.skipped)).foregroundStyle(.secondary) }
                    if let failures = summary.failed, !failures.isEmpty {
                        DisclosureGroup(L("cloud.files_failed", failures.count)) {
                            ForEach(failures) { failure in
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(failure.title)
                                    Text(L("common.error_operation_incomplete")).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                        Button(L("cloud.retry_files")) { account.syncNow(retryFailed: true) }
                            .disabled(account.syncing || !account.enabled || account.nextAllowed > Date())
                    }
                }
                if let error = account.error { Text(LM(error)).font(.footnote).foregroundStyle(.red).textSelection(.enabled) }
            } footer: { Text(L("sync.github_sync_desc")) }
            Section(L("sync.contents")) {
                Label(L("sync.content_note"), systemImage: "book.closed")
                Text(L("sync.content_limit")).foregroundStyle(.secondary)
            }
            Section {
                NavigationLink {
                    Form { NearbySyncView(nearby: nearby) }
                        .navigationTitle(L("sync.pin_sync")).navigationBarTitleDisplayMode(.inline)
                        .task { await nearby.refresh() }
                } label: { HStack { Text(L("sync.pin_sync")); Spacer(); if nearby.listening { Image(systemName: "antenna.radiowaves.left.and.right").foregroundStyle(.secondary) } } }
            }
        }
        .contentMargins(.bottom, 80, for: .scrollContent)
        .navigationTitle(L("sync.multiplatform"))
        .sheet(isPresented: $account.loginPresented, onDismiss: { account.cancelLogin() }) {
            GitHubLoginView(account: account)
        }
    }
}
struct GitHubLoginView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var account: GitHubAccount
    @State private var browser = false
    var body: some View {
        NavigationStack {
            Form {
                Section { DisclosureGroup(L("sync.permission_details")) { Text(SyncPermissionCopy.details).font(.footnote).foregroundStyle(.secondary) } }
                    footer: { Text(SyncPermissionCopy.purpose) }
                if let code = account.code {
                    Section {
                        Text(L("sync.copy_code_hint"))
                        Text(code.user_code).font(.title2.monospaced()).textSelection(.enabled)
                        Button(L("sync.copy_code"), systemImage: "doc.on.doc") { UIPasteboard.general.string = code.user_code }
                        Button(L("sync.open_github"), systemImage: "safari") { browser = true }
                    }
                    Section { HStack { ProgressView(); Text(L("sync.waiting_for_auth")) } }
                } else if let error = account.error {
                    Section { Text(LM(error)).foregroundStyle(.red); Button(L("csc.retry")) { account.login() } }
                } else { ProgressView() }
            }
            .navigationTitle(L("account.login")).navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button(L("sync.cancel_login")) { account.cancelLogin() } } }
            .sheet(isPresented: $browser) { GitHubBrowser() }
        }
    }
}
struct GitHubBrowser: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> SFSafariViewController { SFSafariViewController(url: URL(string: "https://github.com/login/device")!) }
    func updateUIViewController(_ controller: SFSafariViewController, context: Context) {}
}
