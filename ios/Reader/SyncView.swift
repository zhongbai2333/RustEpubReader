import SwiftUI
import SafariServices

private enum SyncPermissionCopy {
    static let purpose = "授权用于创建和读写私密仓库 EpubReaderInfo，在多台设备间同步书籍和阅读进度。同步功能只操作这个仓库；登录不会自动上传数据。"
    static let details = "GitHub 的仓库读写授权（repo）涵盖账号可访问的公开和私密仓库，不能只授予单个仓库。\n本应用将同步读写限定在你账号下的私密仓库 EpubReaderInfo。开启同步后才会创建或使用该仓库，并上传书籍和阅读进度。"
}
struct SyncDesignView: View {
    @ObservedObject var account: GitHubAccount
    @ObservedObject var nearby: NearbySync
    var body: some View {
        Form {
            Section {
                LabeledContent("GitHub 账号", value: account.credential?.owner ?? "未登录")
                LabeledContent("私密仓库", value: "EpubReaderInfo")
                if account.credential == nil { Button("登录 GitHub") { account.login() } }
                else {
                    Button("重新授权") { account.login() }
                    Button("退出登录", role: .destructive) { account.signOut() }
                }
                DisclosureGroup("权限说明") { Text(SyncPermissionCopy.details).font(.footnote).foregroundStyle(.secondary) }
            } header: { Text("GitHub 同步") } footer: { Text(SyncPermissionCopy.purpose) }
            Section {
                Toggle("开启同步", isOn: Binding(get: { account.enabled }, set: { account.setEnabled($0) }))
                    .disabled(account.credential == nil)
                Toggle("将同步提交计入贡献图", isOn: Binding(get: { account.countContributions }, set: { account.setCountContributions($0) }))
                    .disabled(account.syncing)
                Text("默认关闭。关闭时以应用名义提交，不关联你的 GitHub 账号；开启后使用你的 GitHub 身份提交。此开关不影响同步功能。").font(.footnote).foregroundStyle(.secondary)
                DisclosureGroup("贡献图说明") {
                    Text("此设置只影响本机今后的同步提交，从下一次同步起生效；其他设备需分别设置。历史提交不会被重写。创建仓库及初始化可能另有贡献。开启后，提交是否显示在贡献图中，仍取决于 GitHub 的私密贡献（Private contributions）设置。").font(.footnote).foregroundStyle(.secondary)
                }
                if account.syncing {
                    HStack { ProgressView(); Text(account.syncStatus) }
                    Button("取消同步") { account.stopSync() }
                } else {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        if account.enabled && account.nextAllowed > context.date {
                            LabeledContent("稍后重试") { Text(account.nextAllowed, style: .timer).monospacedDigit() }
                        }
                        Button("立即同步") { account.syncNow() }.disabled(!account.enabled || account.nextAllowed > context.date)
                    }
                }
                if let date = account.lastSync { LabeledContent("上次同步") { Text(date, style: .relative) } }
                else { LabeledContent("上次同步", value: "尚未同步") }
                if let summary = account.summary {
                    Text("上传 \(summary.uploaded) 本，下载 \(summary.downloaded) 本")
                    if summary.skipped > 0 { Text("本次跳过 \(summary.skipped) 本").foregroundStyle(.secondary) }
                    if let failures = summary.failed, !failures.isEmpty {
                        DisclosureGroup("\(failures.count) 本书籍传输失败，将单独重试") {
                            ForEach(failures) { failure in
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(failure.title)
                                    Text(failure.error).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                        Button("重试失败书籍") { account.syncNow(retryFailed: true) }
                            .disabled(account.syncing || !account.enabled || account.nextAllowed > Date())
                    }
                }
                if let error = account.error { Text(error).font(.footnote).foregroundStyle(.red).textSelection(.enabled) }
            } footer: { Text("登录并开启同步后，EPUB 文件和阅读进度将同步到你账号下的私密 EpubReaderInfo 仓库。") }
            Section("同步内容") {
                Label("EPUB 文件和阅读进度", systemImage: "book.closed")
                Text("每本书不超过 20 MiB，超出限制的书籍仍可在本机阅读。").foregroundStyle(.secondary)
            }
            Section {
                NavigationLink {
                    Form { NearbySyncView(nearby: nearby) }
                        .navigationTitle("PIN 同步").navigationBarTitleDisplayMode(.inline)
                        .task { await nearby.refresh() }
                } label: { HStack { Text("PIN 同步"); Spacer(); if nearby.listening { Image(systemName: "antenna.radiowaves.left.and.right").foregroundStyle(.secondary) } } }
            }
        }
        .contentMargins(.bottom, 80, for: .scrollContent)
        .navigationTitle("多平台同步")
        .sheet(isPresented: $account.loginPresented, onDismiss: { account.cancelLogin() }) {
            GitHubLoginView(account: account)
        }
    }
}
struct GitHubLoginView: View {
    @ObservedObject var account: GitHubAccount
    @State private var browser = false
    var body: some View {
        NavigationStack {
            Form {
                Section { DisclosureGroup("权限说明") { Text(SyncPermissionCopy.details).font(.footnote).foregroundStyle(.secondary) } }
                    footer: { Text(SyncPermissionCopy.purpose) }
                if let code = account.code {
                    Section {
                        Text("复制以下代码，然后前往 GitHub 完成授权。")
                        Text(code.user_code).font(.title2.monospaced()).textSelection(.enabled)
                        Button("复制代码", systemImage: "doc.on.doc") { UIPasteboard.general.string = code.user_code }
                        Button("打开 GitHub", systemImage: "safari") { browser = true }
                    }
                    Section { HStack { ProgressView(); Text("正在等待授权…") } }
                } else if let error = account.error {
                    Section { Text(error).foregroundStyle(.red); Button("重试") { account.login() } }
                } else { ProgressView() }
            }
            .navigationTitle("登录 GitHub").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("取消登录") { account.cancelLogin() } } }
            .sheet(isPresented: $browser) { GitHubBrowser() }
        }
    }
}
struct GitHubBrowser: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> SFSafariViewController { SFSafariViewController(url: URL(string: "https://github.com/login/device")!) }
    func updateUIViewController(_ controller: SFSafariViewController, context: Context) {}
}
