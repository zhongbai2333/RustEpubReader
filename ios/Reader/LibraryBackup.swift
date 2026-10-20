import SwiftUI
import UniformTypeIdentifiers

struct LibraryBackupView: View {
    @ObservedObject var library: PreviewLibrary
    @ObservedObject var account: GitHubAccount
    @ObservedObject private var readerI18n=ReaderI18n.shared
    @State private var choosing=false
    @State private var running=false
    @State private var selected: URL?
    @State private var exported: URL?
    @State private var summary: LibraryBackupSummary?
    @State private var progress: LibraryBackupProgress?
    @State private var message: String?
    @State private var operation="inspect"
    @State private var showTechnical=false
    @State private var showContents=false
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var typeSize
    var onNeedsExpanded: () -> Void = {}
    @State private var failure: String?
    @State private var sharing=false
    private var available: Bool {!library.busy && !account.syncing && library.openPath.isEmpty}
    #if DEBUG
    // The diagnostic renderer supplies a genuinely verified local package. This
    // exercises the production view, not file-picker interaction or user acceptance.
    static func verification(library:PreviewLibrary,account:GitHubAccount,file:URL,summary:LibraryBackupSummary)->Self {
        var result=Self(library:library,account:account)
        result._selected=State(initialValue:file);result._summary=State(initialValue:summary)
        return result
    }
    #endif
    private var stage: String {
        switch progress?.phase {
        case "collecting": return L("backup.collecting")
        case "packing": return L("backup.packing")
        case "verifying", "inspect": return L("backup.verifying")
        case "merging", "restore": return L("backup.merging")
        case "publishing": return L("backup.publishing")
        default: return L("backup.preparing")
        }
    }
    private var confirming: Bool {summary != nil && failure == nil && !running}
    private var finished: Bool {message != nil && failure == nil && !running}
    private var failureTitle: String {
        switch operation {case "export": return "backup.error_create";case "restore": return "backup.error_restore";default:return "backup.error_inspect"}
    }
    private func home() {selected=nil;summary=nil;message=nil;failure=nil;showTechnical=false}
    private func primary(_ key:String,_ identifier:String,action:@escaping ()->Void)->some View {
        Button(action:action) {Text(L(key)).frame(maxWidth:.infinity,minHeight:24)}
            .buttonStyle(.borderedProminent).accessibilityIdentifier(identifier)
    }
    private func actionRow(_ title:String,_ subtitle:String,_ icon:String,_ identifier:String,action:@escaping ()->Void)->some View {
        Button(action:action) {
            HStack(spacing:14) {
                Image(systemName:icon).font(.system(size:21)).foregroundStyle(Color.accentColor).frame(width:26)
                VStack(alignment:.leading,spacing:4) {
                    Text(L(title)).foregroundStyle(Color(uiColor:.label))
                    Text(L(subtitle)).font(.footnote).foregroundStyle(Color(uiColor:.label).opacity(0.72)).fixedSize(horizontal:false,vertical:true)
                }
                Spacer(minLength:4)
                Image(systemName:"chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
            }.padding(.vertical,6).contentShape(Rectangle())
        }.buttonStyle(.plain).disabled(!available).accessibilityIdentifier(identifier)
    }
    var body: some View {
        Form {
            if running {
                Section {
                    if let p=progress,p.total>0 {
                        ProgressView(value:Double(p.completed),total:Double(p.total)) {Text(stage)}
                        Text(L("cloud.processed",p.completed,p.total)).monospacedDigit().foregroundStyle(Color(uiColor:.label).opacity(0.72))
                    } else {ProgressView {Text(stage)}}
                }.accessibilityElement(children:.combine)
            } else if let failure {
                Section {
                    Label(L(failureTitle),systemImage:"exclamationmark.circle").fontWeight(.semibold).foregroundStyle(.red)
                        .accessibilityIdentifier("backup-failure")
                    Text(L(Self.failureHint(failure))).font(.subheadline)
                    DisclosureGroup(L("backup.details"),isExpanded:$showTechnical) {
                        Text(failure).font(.footnote).textSelection(.enabled)
                    }
                }
            } else if confirming,let selected,let summary {
                Section {
                    VStack(alignment:.leading,spacing:10) {
                        HStack(alignment:.top,spacing:12) {
                            Image(systemName:"doc.zipper").font(.title2).foregroundStyle(Color.accentColor)
                            Text(selected.lastPathComponent).font(.subheadline.weight(.medium)).lineLimit(2).truncationMode(.middle).textSelection(.enabled)
                        }
                        Label(L("backup.verified"),systemImage:"checkmark.circle").foregroundStyle(.primary).font(.subheadline)
                            .accessibilityIdentifier("backup-verified")
                        ViewThatFits(in:.horizontal) {
                            HStack(spacing:16) {backupMetrics(summary)}
                            VStack(alignment:.leading,spacing:6) {backupMetrics(summary)}
                        }.font(.subheadline).foregroundStyle(Color(uiColor:.label).opacity(0.72))
                        Text(L("backup.rules")).font(.footnote).foregroundStyle(Color(uiColor:.label).opacity(0.72)).fixedSize(horizontal:false,vertical:true)
                        Button(L("backup.change_file")) {choosing=true}.disabled(!available).accessibilityIdentifier("backup-change")
                            .frame(minHeight:44)
                    }.padding(.vertical,4)
                }
            } else if let message {
                Section {
                    Label(L(message),systemImage:"checkmark.circle").fontWeight(.semibold).foregroundStyle(Color.accentColor)
                        .accessibilityIdentifier("backup-result")
                    if message=="backup.created",let exported {
                        Text(exported.lastPathComponent).font(.subheadline).textSelection(.enabled)
                        Text(L("backup.export_hint")).font(.footnote).foregroundStyle(Color(uiColor:.label).opacity(0.72))
                    }
                }
            } else {
                Section {
                    actionRow("backup.create","backup.create_hint","square.and.arrow.up","backup-create") {perform("export")}
                    actionRow("backup.restore_title","backup.restore_hint","arrow.down.doc","backup-choose") {choosing=true}
                } footer: {Text(L("backup.privacy_short")).foregroundStyle(Color(uiColor:.label))}
                Section {
                    DisclosureGroup(L("backup.contents"),isExpanded:$showContents) {
                        Text(L("backup.scope")).font(.subheadline)
                        Text(L("backup.excludes")).font(.footnote).foregroundStyle(Color(uiColor:.label).opacity(0.72))
                    }
                }
            }
            if !available && !running {Section {Text(L("backup.wait")).font(.footnote).foregroundStyle(Color(uiColor:.label).opacity(0.72))}}
        }
        .navigationTitle(L(confirming ? "backup.confirm_restore" : "backup.title")).navigationBarTitleDisplayMode(.inline)
        .navigationBarBackButtonHidden(running)
        .interactiveDismissDisabled(running)
        .safeAreaInset(edge:.bottom,spacing:0) {
            if confirming || finished || failure != nil {
                VStack(spacing:4) {
                    if confirming,let selected {
                        primary("backup.merge","backup-merge") {perform("restore",url:selected)}
                    } else if failure != nil {
                        primary("backup.retry","backup-retry") {
                            if operation=="export" {perform("export")}
                            else if operation=="restore",let selected {perform("restore",url:selected)}
                            else {choosing=true}
                        }
                    } else if message=="backup.created" {
                        primary("backup.share","backup-share") {sharing=true}
                    } else {
                        primary("manage.done","backup-done") {dismiss()}
                    }
                    Button(L("backup.back"),action:home).frame(minHeight:44).accessibilityIdentifier("backup-home")
                }.controlSize(.large).disabled(!available).padding(.horizontal,20).padding(.top,12)
                    .background(.bar)
            }
        }
        .fileImporter(isPresented:$choosing,allowedContentTypes:[.data]) {result in
            switch result {
            case .success(let url): perform("inspect",url:url)
            case .failure(let error):
                let cocoa=error as NSError
                if cocoa.domain != NSCocoaErrorDomain || cocoa.code != NSUserCancelledError {operation="inspect";failure=error.localizedDescription;showTechnical=false}
            }
        }
        .sheet(isPresented:$sharing) {if let exported {ShareFiles(urls:[exported])}}
        .onChange(of:summary?.manifest_sha256) {_,value in if value != nil {onNeedsExpanded()}}
        .onChange(of:typeSize) {_,_ in if typeSize.isAccessibilitySize {onNeedsExpanded()}}
        .onAppear {if typeSize.isAccessibilitySize {onNeedsExpanded()}}
        .task(id:running) {
            guard running else {return}
            let client=library.client
            while !Task.isCancelled && running {
                progress=try? await Task.detached {try client.call("backup",["action":"status"],as:LibraryBackupProgress.self)}.value
                try? await Task.sleep(for:.milliseconds(200))
            }
        }
    }
    @ViewBuilder private func backupMetrics(_ summary:LibraryBackupSummary)->some View {
        Text(L("backup.books",summary.books))
        Text(L("backup.archived",summary.archived))
        Text(ByteCountFormatter.string(fromByteCount:Int64(clamping:summary.file_bytes),countStyle:.file))
    }
    static func failureHint(_ detail:String)->String {
        let error=detail.lowercased()
        if error.contains("backup changed after verification") {return "backup.error_changed"}
        if error.contains("unsupported library backup version") {return "backup.error_version"}
        if error.contains("no space left") || error.contains("os error 28") {return "backup.error_space"}
        if error.contains("permission denied") || error.contains("access is denied") {return "backup.error_permission"}
        return "backup.error_hint"
    }
    private func perform(_ action: String,url: URL?=nil) {
        guard available else {return}
        running=true;library.busy=true;failure=nil;message=nil;progress=nil;operation=action;showTechnical=false
        if action=="inspect" {selected=nil;summary=nil}
        // Invalidate any already prepared cloud result before changing the catalog.
        account.stopSync()
        Task {
            defer {running=false;library.busy=false;progress=nil}
            do {
                await library.flush()
                let directory=FileManager.default.temporaryDirectory.appendingPathComponent("LibraryBackups",isDirectory:true).appendingPathComponent(UUID().uuidString,isDirectory:true)
                let path: URL
                if action=="export" {
                    try FileManager.default.createDirectory(at:directory,withIntermediateDirectories:true)
                    path=directory.appendingPathComponent("RustEpubReader.readerbackup")
                } else if action=="inspect",let url {
                    let owned=directory.appendingPathComponent(url.lastPathComponent)
                    try await Task.detached {
                        let scoped=url.startAccessingSecurityScopedResource();defer {if scoped {url.stopAccessingSecurityScopedResource()}}
                        try FileManager.default.createDirectory(at:directory,withIntermediateDirectories:true)
                        try FileManager.default.copyItem(at:url,to:owned)
                    }.value
                    path=owned
                } else if let url {path=url}
                else {throw CoreFailure(message:"Missing backup file")}
                let result=try await library.store.backup(action:action,path:path.path,expected:action=="restore" ? summary?.manifest_sha256 : nil)
                switch action {
                case "export": exported=path;message="backup.created";sharing=true
                case "inspect": selected=path;summary=result
                default: await library.backupDidRestore();summary=nil;selected=nil;message="backup.done"
                }
            } catch {failure=error.localizedDescription}
        }
    }
}
