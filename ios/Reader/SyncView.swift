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
                    SyncProgressContent(account: account)
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
    @State private var copied = false
    @State private var opened = false
    var body: some View {
        NavigationStack {
            Form {
                Section { DisclosureGroup(L("sync.permission_details")) { Text(SyncPermissionCopy.details).font(.footnote).foregroundStyle(.secondary) } }
                    footer: { Text(SyncPermissionCopy.purpose) }
                if let code = account.code {
                    Section {
                        Text(L("sync.copy_code_hint"))
                        Text(code.user_code).font(.title2.monospaced()).textSelection(.enabled)
                        Button(copied ? L("sync.code_copied") : L("sync.copy_code"), systemImage: copied ? "checkmark" : "doc.on.doc") {
                            UIPasteboard.general.string = code.user_code
                            copied = true
                            UINotificationFeedbackGenerator().notificationOccurred(.success)
                        }
                        .task(id: copied) {
                            guard copied else { return }
                            try? await Task.sleep(for: .seconds(2))
                            if !Task.isCancelled { copied = false }
                        }
                        Button(opened ? L("sync.reopen_github") : L("sync.open_github"), systemImage: "safari") {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            browser = true; opened = true
                        }
                        if opened { Text(L("sync.browser_opened")).font(.footnote).foregroundStyle(.secondary) }
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

struct SyncProgressContent: View {
    @ObservedObject var account: GitHubAccount
    var compact = false
    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            if let meter = account.meter, !meter.phase.isEmpty {
                Text(meter.label).font(compact ? .caption2 : .subheadline).foregroundStyle(.secondary)
                if meter.total > 0 {
                    ProgressView(value: Double(meter.completed), total: Double(meter.total))
                    Text(meter.amount).font(.caption2).monospacedDigit().foregroundStyle(.secondary)
                }
                if !compact && !meter.title.isEmpty { Text(meter.title).font(.caption).lineLimit(2).foregroundStyle(.secondary) }
            } else { Text(LM(account.syncStatus)).font(.caption).foregroundStyle(.secondary) }
        }
        .frame(maxWidth: compact ? 180 : .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}
struct ArchiveLibraryView: View {
    @ObservedObject var library: PreviewLibrary
    @Environment(\.dismiss) private var dismiss
    @State private var deleting: ArchivedEntry?
    var body: some View {
        NavigationStack {
            List {
                Section {
                    if library.archived.isEmpty { Text(L("archive.empty")).foregroundStyle(.secondary) }
                    ForEach(library.archived) { entry in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(entry.book.title).font(.headline)
                            Text(L(entry.local_available ? "archive.local" : "archive.cloud")).font(.caption).foregroundStyle(.secondary)
                            HStack {
                                Button(L("archive.restore")) { Task { await library.archiveAction(entry,action:"restore") } }
                                Spacer()
                                Button(L("archive.delete"), role: .destructive) { deleting = entry }
                            }.buttonStyle(.borderless).disabled(library.busy)
                        }.padding(.vertical,4)
                    }
                }
            }
            .navigationTitle(L("archive.title")).navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(L("manage.done")) { dismiss() } } }
            .alert(L("archive.delete_confirm"), isPresented: Binding(get:{deleting != nil},set:{if !$0 {deleting=nil}})) {
                Button(L("archive.delete"),role:.destructive) { if let entry=deleting { Task { await library.archiveAction(entry,action:"delete") } }; deleting=nil }
                Button(L("settings.cancel"),role:.cancel) { deleting=nil }
            } message: { Text(L("archive.delete_note")) }
        }
    }
}
