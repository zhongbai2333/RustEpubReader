import SwiftUI
import PhotosUI
import AVFoundation
import UIKit

#if DEBUG
/// Exercises the real simulator C ABI with synthetic, locally staged inputs only.
enum BookShareRuntimeCheck {
    static func run() {
        Task.detached {
            let files = FileManager.default
            let documents = files.urls(for: .documentDirectory, in: .userDomainMask)[0]
            let caches = files.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            let root = caches.appendingPathComponent("native-book-share-check")
            let client = CoreClient(root: root.path, device: "book-share-check")
            var report: [String: Any]
            do {
                let decoded: [String: String] = try client.call("book_share", ["request": ["op": "decode_image", "path": documents.appendingPathComponent("book-share-qr.png").path]])
                let before: [LibraryEntry] = try client.call("library")
                let state: BookShareState = try client.call("book_share", ["request": ["op": "receive", "link": decoded["link"] ?? ""]])
                let pending: [LibraryEntry] = try client.call("library")
                guard state.preview?.books.count == 1, before.count == pending.count else { throw CoreFailure(message: "Preview imported without confirmation") }
                let accepted: BookShareReply = try client.call("book_share", ["request": ["op": "accept"]])
                let books: [LibraryEntry] = try client.call("library")
                guard (accepted.imported ?? 0) + (accepted.skipped ?? 0) == 1, books.contains(where: { $0.title == state.preview?.books.first?.title }) else { throw CoreFailure(message: "Title or import mismatch") }
                report = ["passed": true, "qr_image_decoded": true, "preview_does_not_import": true, "imported": accepted.imported ?? 0, "skipped": accepted.skipped ?? 0, "title_preserved": true, "github_credentials_used": false]
            } catch { report = ["passed": false, "error": (error as? CoreFailure)?.message ?? String(describing: error)] }
            if let bytes = try? JSONSerialization.data(withJSONObject: report, options: .prettyPrinted) { try? bytes.write(to: caches.appendingPathComponent("book-share-verification.json"), options: .atomic) }
        }
    }
}
#endif

struct BookShareProgress: Decodable {
    var phase = "idle"; var done: UInt64 = 0; var total: UInt64 = 0; var error: String?
}
struct BookShareOutgoing: Decodable { let kind: String; let link: String; let qr: String; let pin: String?; let expires: UInt64?; let count: Int }
struct BookShareInfo: Decodable { let title: String; let hash: String; let size: UInt64 }
struct BookSharePreview: Decodable { let version: Int; let books: [BookShareInfo] }
struct PublishedBookShare: Decodable, Identifiable { let id: String; let owner: String; let repository: String; let release: UInt64; let created: UInt64; let count: Int; let ready: Bool? }
struct BookShareState: Decodable {
    var job_id: String?; var progress = BookShareProgress(); var outgoing: BookShareOutgoing?; var preview: BookSharePreview?; var history: [PublishedBookShare] = []
}
private struct BookShareReply: Decodable { let imported: Int?; let skipped: Int?; let link: String? }

@MainActor final class BookSharing: ObservableObject {
    @Published var state = BookShareState()
    @Published var busy = false
    @Published var error: String?
    @Published var result: (Int, Int)?
    @Published var link = ""
    private let client: CoreClient
    private weak var library: PreviewLibrary?
    private var generation = UUID()
    private var currentOperation = ""
    var hasGitHubStatus: Bool { ["publish", "delete_share"].contains(currentOperation) }
    func cancelAuthenticatedWork() { if busy && ["publish", "delete_share"].contains(currentOperation) { cancel() } }
    private var jobID: String?
    init(library: PreviewLibrary) { self.library = library; client = library.client }
    var phase: String { state.progress.phase }
    var activeLan: Bool { state.outgoing?.kind == "lan" && ["waiting", "transferring"].contains(phase) }
    func command(_ operation: String, _ fields: [String: Any] = [:], cleanup: URL? = nil) {
        guard !busy else { return }
        var request = fields; request["op"] = operation; currentOperation = operation
        if ["start_lan", "receive", "publish", "delete_share"].contains(operation) {
            do { let armed: [String: String] = try client.call("book_share", ["request": ["op": "arm", "operation": operation]]); jobID = armed["job_id"]; request["job_id"] = jobID
                let history = state.history; state = BookShareState(); state.job_id = jobID; state.history = history
                state.progress.phase = operation == "receive" ? "receiving" : operation == "delete_share" ? "deleting" : "preparing"
            }
            catch { self.error = error.localizedDescription; return }
        }
        let intent = UUID(); generation = intent; busy = true; error = nil; result = nil
        if operation == "accept" { state.progress.phase = "importing" }
        let client = self.client, input = request
        Task {
            defer { if let cleanup { try? FileManager.default.removeItem(at: cleanup) } }
            do {
                let reply: BookShareReply = try await Task.detached { try client.call("book_share", ["request": input]) }.value
                guard generation == intent else { return }
                let snapshot: BookShareState = try await Task.detached { try client.call("book_share", ["request": ["op": "state"]]) }.value
                guard generation == intent else { return }
                state = snapshot; jobID = snapshot.job_id; busy = false
                if operation == "accept" { result = (reply.imported ?? 0, reply.skipped ?? 0); await library?.reload() }
                if operation == "decode_image", let link = reply.link { receive(link) }
            } catch { if generation == intent { busy = false; self.error = error.localizedDescription; await poll() } }
        }
    }
    func receive(_ value: String) { link = value; command("receive", ["link": value]) }
    func scanImage(_ bytes: Data) {
        guard !busy, bytes.count <= 16 * 1024 * 1024 else { return }
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("book-share-qr-\(UUID().uuidString).image")
        do { try bytes.write(to: path, options: .atomic); command("decode_image", ["path": path.path], cleanup: path) }
        catch { self.error = error.localizedDescription }
    }
    func poll() async {
        let intent = generation, client = self.client
        do { let snapshot: BookShareState = try await Task.detached { try client.call("book_share", ["request": ["op": "state"]]) }.value
            if generation == intent { state = snapshot; jobID = snapshot.job_id }
        } catch {}
    }
    func resetReceiver() {
        cancel()
        let history = state.history
        state = BookShareState(); state.history = history
        jobID = nil; result = nil; link = ""
    }
    func cancel() {
        generation = UUID(); busy = false; error = nil
        let intent = generation
        let client = self.client, stopping = jobID
        Task {
            let snapshot: BookShareState? = try? await Task.detached { try client.call("book_share", ["request": ["op": "cancel", "job_id": stopping ?? ""]]) }.value
            if generation == intent, jobID == stopping, let snapshot { state = snapshot }
        }
        UIApplication.shared.isIdleTimerDisabled = false
    }
}

struct BookShareStatusView: View {
    @ObservedObject var sharing: BookSharing
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if sharing.phase != "idle" && sharing.phase != "error" { Text(L("transfer." + sharing.phase)).font(.headline) }
            if sharing.state.progress.total > 0 {
                ProgressView(value: Double(sharing.state.progress.done), total: Double(sharing.state.progress.total))
                Text((Double(sharing.state.progress.done) / Double(sharing.state.progress.total)).formatted(.percent.precision(.fractionLength(0)))).font(.caption).monospacedDigit()
            }
            if let error = sharing.error ?? sharing.state.progress.error { Text(L("transfer.failed", error)).font(.footnote).foregroundStyle(.red) }
            if let (imported, skipped) = sharing.result { Text(L("transfer.result", imported, skipped)) }
        }
    }
}
struct ShareBookSummary: View {
    let books: [LibraryEntry]
    let covers: ShelfCoverStore
    @State private var cover: UIImage?
    @State private var selection = false
    var body: some View {
        HStack(spacing: 14) {
            Group {
                if let cover { Image(uiImage: cover).resizable().scaledToFill() }
                else { Image(systemName: "book.closed").font(.title3).foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity).background(Color(.tertiarySystemFill)) }
            }.frame(width: 38, height: 54).clipShape(RoundedRectangle(cornerRadius: 5))
            VStack(alignment: .leading, spacing: 5) {
                Text(books.first?.title ?? "").font(.headline).lineLimit(2)
                Text(L("manage.count", books.count)).font(.footnote).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if books.count > 1 { Image(systemName: "chevron.right").font(.footnote).foregroundStyle(.tertiary) }
        }.contentShape(Rectangle()).onTapGesture { if books.count > 1 { selection = true } }
            .task(id: books.first?.id) {
                if let book = books.first {
                    let loaded = await covers.image(id: book.id, path: book.path)
                    if !Task.isCancelled { cover = loaded }
                }
            }
            .sheet(isPresented: $selection) {
                NavigationStack {
                    List(books) { Text($0.title) }
                        .navigationTitle(L("share_ui.selected")).navigationBarTitleDisplayMode(.inline)
                        .toolbar { ToolbarItem(placement: .confirmationAction) { Button(L("settings.close")) { selection = false } } }
                }
            }
    }
}

private struct ShareMethodRow: View {
    let title: String; let subtitle: String; let icon: String
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: icon).font(.system(size: 20)).foregroundStyle(.secondary).frame(width: 28)
                VStack(alignment: .leading, spacing: 4) { Text(L(title)).font(.body).foregroundStyle(.primary); Text(L(subtitle)).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
                Spacer(minLength: 4)
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
            }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).padding(.horizontal, 16).padding(.vertical, 10).contentShape(Rectangle())
        }.buttonStyle(.plain)
    }
}

struct BookShareSender: View {
    @ObservedObject var sharing: BookSharing
    @ObservedObject var account: GitHubAccount
    let ids: [String]
    @Binding var stage: Int
    let exporting: Bool
    let export: () -> Void
    @State private var copied = false
    private var qrReady: Bool { ["waiting", "published"].contains(sharing.phase) && sharing.state.outgoing?.kind == (stage == 1 ? "lan" : "github") }
    private func local() { stage = 1; sharing.command("start_lan", ["ids": ids]) }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if stage == 0 {
                VStack(spacing: 0) {
                    ShareMethodRow(title: "share_ui.nearby", subtitle: "share_ui.nearby_hint", icon: "wifi", action: local)
                    Divider().padding(.leading, 58)
                    ShareMethodRow(title: "transfer.github", subtitle: "share_ui.cloud_hint", icon: "icloud.and.arrow.up") { stage = 2 }
                    Divider().padding(.leading, 58)
                    ShareMethodRow(title: "share_ui.export_file", subtitle: "share_ui.export_hint", icon: "square.and.arrow.up", action: export)
                }.background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 14)).disabled(exporting || sharing.busy || ids.isEmpty)
                if exporting { ProgressView().frame(maxWidth: .infinity) }
                DisclosureGroup(L("share_ui.details")) { VStack(alignment: .leading, spacing: 8) { Text(L("transfer.limits")); Text(L("transfer.lan_hint")) }.font(.footnote).foregroundStyle(.secondary).padding(.top, 8) }.font(.footnote)
                if !sharing.state.history.isEmpty { DisclosureGroup(L("transfer.history")) { PublishedBookShares(sharing: sharing, account: account) }.font(.footnote) }
            } else if stage == 2 && !qrReady {
                Text(L("share_ui.public_hint")).font(.subheadline).foregroundStyle(.secondary)
                Button {
                    if let credential = account.credential, GitHubOAuth.hasRepoScope(credential.scopes) { sharing.command("publish", ["ids": ids, "token": credential.token]) }
                    else { account.login() }
                } label: { Text(L(account.credential.map { GitHubOAuth.hasRepoScope($0.scopes) } == true ? "share_ui.create_link" : "transfer.login")).frame(maxWidth: .infinity).padding(.vertical, 5) }.buttonStyle(.borderedProminent).disabled(sharing.busy || ids.isEmpty)
                if sharing.hasGitHubStatus { BookShareStatusView(sharing: sharing) }
                if sharing.busy { Button(L("transfer.cancel")) { sharing.cancel() }.font(.subheadline) }
                DisclosureGroup(L("share_ui.repository_details")) { VStack(alignment: .leading, spacing: 8) { Text(L("transfer.github_hint")); Text(L("transfer.limits")) }.font(.footnote).foregroundStyle(.secondary).padding(.top, 8) }.font(.footnote)
                if !sharing.state.history.isEmpty { DisclosureGroup(L("transfer.history")) { PublishedBookShares(sharing: sharing, account: account) }.font(.footnote) }
            } else if qrReady, let outgoing = sharing.state.outgoing {
                VStack(spacing: 12) {
                    if let data = Data(base64Encoded: outgoing.qr), let image = UIImage(data: data) {
                        Image(uiImage: image).resizable().interpolation(.none).scaledToFit().frame(width: 240, height: 240).padding(10).background(.white, in: RoundedRectangle(cornerRadius: 12)).accessibilityLabel(L("share_ui.qr_accessibility"))
                    }
                    if let expires = outgoing.expires {
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            let seconds = max(0, Int(expires) - Int(context.date.timeIntervalSince1970))
                            Text(L("transfer.remaining", Duration.seconds(seconds).formatted(.time(pattern: .minuteSecond)))).font(.subheadline.weight(.medium)).monospacedDigit()
                        }
                        Text(L("share_ui.keep_open")).font(.caption).foregroundStyle(.secondary)
                    }
                    Text(L("transfer.credential_hint")).font(.footnote).foregroundStyle(Color.primary.opacity(0.72)).multilineTextAlignment(.center)
                    Button {
                        UIPasteboard.general.string = outgoing.link; copied = true
                        Task { try? await Task.sleep(for: .seconds(2.5)); copied = false }
                    } label: { Label(L(copied ? "transfer.copied" : "transfer.copy_link"), systemImage: copied ? "checkmark" : "link").frame(maxWidth: .infinity).padding(.vertical, 5) }.buttonStyle(.borderedProminent)
                    if sharing.activeLan { Button(L("share_ui.end")) { sharing.cancel(); stage = 0 }.font(.subheadline).tint(.primary) }
                    if sharing.phase == "published" { Text(L("transfer.published_hint")).font(.caption).foregroundStyle(.secondary) }
                }.frame(maxWidth: .infinity)
            } else {
                BookShareStatusView(sharing: sharing)
                if sharing.busy { ProgressView().frame(maxWidth: .infinity); Button(L("transfer.cancel")) { sharing.cancel(); stage = 0 } }
                else if stage == 1 { Button(L("share_ui.retry"), action: local).buttonStyle(.borderedProminent) }
            }
        }
        .task { while !Task.isCancelled { await sharing.poll(); try? await Task.sleep(for: .milliseconds(500)) } }
        .onChange(of: sharing.activeLan) { _, value in UIApplication.shared.isIdleTimerDisabled = value }
        .onDisappear { sharing.cancel() }
    }
}
struct PublishedBookShares: View {
    @ObservedObject var sharing: BookSharing
    @ObservedObject var account: GitHubAccount
    @State private var deleting: PublishedBookShare?
    var body: some View {
        if !sharing.state.history.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(L("transfer.history")).font(.headline)
                ForEach(sharing.state.history.reversed()) { share in
                    HStack {
                        VStack(alignment: .leading) { Text(L("manage.count", share.count)); Text(Date(timeIntervalSince1970: Double(share.created)), format: .dateTime.year().month().day().hour().minute()).font(.caption).foregroundStyle(.secondary); if share.ready == false && !sharing.busy { Text(L("transfer.unfinished")).font(.caption) } }
                        Spacer()
                        Button(L("transfer.delete"), role: .destructive) { deleting = share }.disabled(sharing.busy || account.credential == nil)
                    }
                }
            }.confirmationDialog(L("transfer.delete"), isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
                Button(L("transfer.delete"), role: .destructive) { if let item = deleting, let token = account.credential?.token { sharing.command("delete_share", ["id": item.id, "token": token]) }; deleting = nil }
                Button(L("settings.cancel"), role: .cancel) { deleting = nil }
            } message: { Text(L("transfer.delete_confirm")) }
        }
    }
}
struct ReceiveBooksView: View {
    @ObservedObject var sharing: BookSharing
    @ObservedObject var account: GitHubAccount
    @Environment(\.dismiss) private var dismiss
    @State private var scanning = false
    @State private var editingLink = false
    @State private var image: PhotosPickerItem?
    @FocusState private var linkFocused: Bool
    private var inputVisible: Bool { !sharing.busy && sharing.result == nil && sharing.state.preview == nil }
    private var displayedError: String? { sharing.error ?? sharing.state.progress.error }
    var body: some View {
        NavigationStack {
            List {
                if let (imported, skipped) = sharing.result {
                    Section {
                        Label(L("transfer.imported"), systemImage: "checkmark.circle")
                            .font(.headline)
                        Text(L("transfer.result", imported, skipped))
                    }
                } else if sharing.busy {
                    Section {
                        HStack(spacing: 14) {
                            ProgressView()
                            Text(L("transfer." + sharing.phase))
                        }.padding(.vertical, 8)
                        if sharing.state.progress.total > 0 {
                            ProgressView(value: Double(sharing.state.progress.done), total: Double(sharing.state.progress.total))
                        }
                    }
                } else if let preview = sharing.state.preview {
                    Section {
                        Text(L("receive.ready_hint")).font(.subheadline).foregroundStyle(.secondary)
                    } header: { Text(L("receive.preview_title")) }
                    Section {
                        ForEach(Array(preview.books.enumerated()), id: \.offset) { _, book in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(book.title).lineLimit(2)
                                Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: book.size), countStyle: .file))
                                    .font(.footnote).foregroundStyle(.secondary)
                            }.padding(.vertical, 2)
                        }
                    } header: { Text(L("manage.count", preview.books.count)) }
                } else if editingLink {
                    Section {
                        TextField(L("receive.link_placeholder"), text: $sharing.link, axis: .vertical)
                            .lineLimit(1...3).textInputAutocapitalization(.never).autocorrectionDisabled()
                            .keyboardType(.URL).focused($linkFocused)
                            .accessibilityLabel(L("receive.link_title"))
                    } header: { Text(L("receive.link_title")) }
                      footer: { Text(L("receive.no_login")) }
                } else {
                    Section {
                        Button { scanning = true } label: { sourceLabel("transfer.scan", "qrcode.viewfinder") }
                        PhotosPicker(selection: $image, matching: .images) { sourceLabel("transfer.image", "photo") }
                        Button { editingLink = true } label: { sourceLabel("transfer.link", "link") }
                    } footer: { Text(L("receive.no_login")) }
                }
                if let error = displayedError {
                    Section { Text(L("transfer.failed", error)).font(.footnote).foregroundStyle(.red) }
                }
            }.listStyle(.insetGrouped)
             .scrollDismissesKeyboard(.interactively)
             .safeAreaInset(edge: .bottom, spacing: 0) { footer }
             .navigationTitle(L(editingLink && inputVisible ? "receive.link_title" : "transfer.receive"))
             .navigationBarTitleDisplayMode(.inline)
             .toolbar {
                 if editingLink && inputVisible {
                     ToolbarItem(placement: .cancellationAction) {
                         Button { linkFocused = false; editingLink = false } label: {
                             Label(L("settings.back"), systemImage: "chevron.left")
                         }
                     }
                 }
                 ToolbarItem(placement: .confirmationAction) {
                     Button(L("settings.close")) { sharing.cancel(); dismiss() }
                 }
             }
             .sheet(isPresented: $scanning) {
                 NavigationStack {
                     ShareQRScanner { payload in scanning = false; sharing.receive(payload) }
                         .ignoresSafeArea(edges: .bottom)
                         .navigationTitle(L("transfer.scan")).navigationBarTitleDisplayMode(.inline)
                         .toolbar { ToolbarItem(placement: .confirmationAction) { Button(L("settings.close")) { scanning = false } } }
                 }
             }
             .task(id: image) {
                 if let selected = image, let bytes = try? await selected.loadTransferable(type: Data.self), !Task.isCancelled {
                     image = nil; sharing.scanImage(bytes)
                 }
             }
             .task(id: editingLink && inputVisible) { linkFocused = editingLink && inputVisible }
             .task {
                 editingLink = !sharing.link.isEmpty
                 while !Task.isCancelled { await sharing.poll(); try? await Task.sleep(for: .milliseconds(500)) }
             }
        }.interactiveDismissDisabled(sharing.busy)
         .presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
    }
    private func sourceLabel(_ key: String, _ symbol: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).foregroundStyle(.tint).frame(width: 24)
            Text(L(key)).foregroundStyle(Color.primary)
            Spacer(minLength: 8)
            Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(Color(.tertiaryLabel))
        }.padding(.vertical, 6).contentShape(Rectangle())
    }
    @ViewBuilder private var footer: some View {
        if sharing.result != nil || sharing.busy || sharing.state.preview != nil || editingLink {
            VStack(spacing: 8) {
                if sharing.result != nil {
                    Button(L("manage.done")) { dismiss() }.buttonStyle(.borderedProminent)
                } else if sharing.busy {
                    Button(L("transfer.cancel")) { sharing.cancel() }.buttonStyle(.bordered)
                } else if sharing.state.preview != nil {
                    Button(L("transfer.import")) { sharing.command("accept") }.buttonStyle(.borderedProminent)
                    Button(L("receive.choose_again")) { sharing.resetReceiver(); editingLink = false }
                } else {
                    Button(L("transfer.connect")) {
                        linkFocused = false
                        sharing.receive(sharing.link.trimmingCharacters(in: .whitespacesAndNewlines))
                    }.buttonStyle(.borderedProminent).disabled(sharing.link.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }.controlSize(.large).frame(maxWidth: .infinity).padding(.horizontal, 20).padding(.vertical, 12).background(.bar)
        }
    }
}
struct ShareQRScanner: UIViewControllerRepresentable {
    let decoded: (String) -> Void
    func makeUIViewController(context: Context) -> ShareQRScanController { let controller = ShareQRScanController(); controller.decoded = decoded; return controller }
    func updateUIViewController(_ controller: ShareQRScanController, context: Context) {}
    static func dismantleUIViewController(_ controller: ShareQRScanController, coordinator: ()) { controller.stop() }
}
final class ShareQRScanController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var decoded: ((String) -> Void)?
    private let session = AVCaptureSession()
    private let sessionQueue = DispatchQueue(label: "reader.book-share.camera")
    private var preview: AVCaptureVideoPreviewLayer?
    private let hint = UILabel()
    private var scanned = false
    override func viewDidLoad() {
        super.viewDidLoad(); view.backgroundColor = .black
        hint.text = L("transfer.camera_hint"); hint.textColor = .white; hint.numberOfLines = 0; hint.textAlignment = .center; view.addSubview(hint)
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: configure()
        case .notDetermined: AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in DispatchQueue.main.async { if granted { self?.configure() } } }
        default: break
        }
    }
    private func configure() {
        guard let camera = AVCaptureDevice.default(for: .video), let input = try? AVCaptureDeviceInput(device: camera), session.canAddInput(input) else { return }
        session.addInput(input)
        let output = AVCaptureMetadataOutput(); guard session.canAddOutput(output) else { return }; session.addOutput(output); output.setMetadataObjectsDelegate(self, queue: .main); output.metadataObjectTypes = [.qr]
        let preview = AVCaptureVideoPreviewLayer(session: session); preview.videoGravity = .resizeAspectFill; view.layer.insertSublayer(preview, at: 0); self.preview = preview
        hint.text = L("transfer.scan"); view.setNeedsLayout(); let session = self.session; sessionQueue.async { session.startRunning() }
    }
    override func viewDidLayoutSubviews() { super.viewDidLayoutSubviews(); preview?.frame = view.bounds; hint.frame = CGRect(x: 20, y: view.safeAreaInsets.top + 20, width: view.bounds.width - 40, height: 90) }
    func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput metadataObjects: [AVMetadataObject], from connection: AVCaptureConnection) {
        guard !scanned, let code = metadataObjects.compactMap({ $0 as? AVMetadataMachineReadableCodeObject }).first, let value = code.stringValue else { return }
        scanned = true; stop(); decoded?(value)
    }
    func stop() { let session = self.session; sessionQueue.async { session.stopRunning() } }
}
