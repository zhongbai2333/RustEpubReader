import Foundation
import SwiftUI
struct PairedReader: Decodable, Identifiable { let device_id: String; let device_name: String; var id: String { device_id } }
struct NearbyState: Decodable { let address: String; let status: String; let paired: [PairedReader]; let result: SyncSummary? }
@MainActor final class NearbySync: ObservableObject {
    @Published var listening = false
    @Published var busy = false
    @Published var address = ""
    @Published var pin = ""
    @Published var remoteAddress = ""
    @Published var remotePin = ""
    @Published var state: NearbyState?
    @Published var error: String?
    private var intent = UUID()
    private var operation: Task<Void,Never>?
    private var polling: Task<Void,Never>?
    let library: PreviewLibrary
    init(library: PreviewLibrary) { self.library = library }
    func stop() { intent = UUID(); reader_ios_stop_nearby(); operation?.cancel(); polling?.cancel(); busy = false; listening = false; address = "" }
    func refresh() async {
        let client = library.client
        do { state = try await Task.detached { try client.call("peer_state", as: NearbyState.self) }.value; address = state?.address ?? ""; if let summary = state?.result { library.offer = summary.offer; await library.reload() } }
        catch { self.error = L("sync.error_connection_failed") }
    }
    func start() {
        guard !busy else { return }; error = nil; busy = true
        pin = String(format: "%04d", Int.random(in: 0...9999))
        let generation = UUID(); intent = generation; let client = library.client, pin = self.pin
        operation = Task {
            do { let _: [String: Bool] = try await Task.detached { try client.call("peer_start", ["pin":pin]) }.value
                guard intent == generation && !Task.isCancelled else { return }
                busy = false; listening = true; await refresh()
                polling = Task { while !Task.isCancelled { try? await Task.sleep(for: .seconds(2)); if !Task.isCancelled { await refresh() } } }
            } catch { if intent == generation { busy = false; self.error = L("sync.error_connection_failed") } }
        }
    }
    func connect(peer: String? = nil) {
        guard !busy else { return }; error = nil; busy = true
        let generation = UUID(); intent = generation; let client = library.client, address = remoteAddress, pin = remotePin, path = library.openPath
        operation = Task {
            await library.flush()
            do {
                let summary: SyncSummary = try await Task.detached { try client.call("peer_connect", ["address":address,"pin":pin,"open_path":path,"peer_id":peer ?? ""]) }.value
                guard intent == generation && !Task.isCancelled else { return }
                library.offer = summary.offer; await library.reload(); await refresh(); busy = false
            } catch { if intent == generation { busy = false; self.error = L("sync.error_connection_failed") } }
        }
    }
    func remove(_ id: String) { let client = library.client; Task { let _: [String:Bool]? = try? await Task.detached { try client.call("peer_remove",["peer_id":id]) }.value; await refresh() } }
}
struct NearbySyncView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var nearby: NearbySync
    var body: some View {
        Section {
            Toggle(L("sync.allow_device_connection"), isOn: Binding(get: { nearby.listening }, set: { if $0 { nearby.start() } else { nearby.stop() } })).disabled(nearby.busy)
            if nearby.listening { LabeledContent(L("sync.local_address"), value: nearby.address).textSelection(.enabled); LabeledContent(L("sync.local_pin"), value: nearby.pin).monospacedDigit().textSelection(.enabled) }
            TextField(L("sync.peer_address"), text: $nearby.remoteAddress, prompt: Text("192.168.1.2:14528")).textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.numbersAndPunctuation)
            TextField(L("sync.peer_pin"), text: $nearby.remotePin).keyboardType(.numberPad).onChange(of: nearby.remotePin) { _, value in nearby.remotePin = String(value.filter(\.isNumber).prefix(4)) }
            if nearby.busy { ProgressView(); Button(L("sync.cancel_connection")) { nearby.stop() } }
            else { Button(L("sync.connect_and_sync")) { nearby.connect() }.disabled(nearby.remoteAddress.isEmpty || nearby.remotePin.count != 4) }
            if let error = nearby.error { Text(LM(error)).font(.footnote).foregroundStyle(.red) }
            if let state = nearby.state, state.status == "synced" { Text(L("sync.synced")).foregroundStyle(.secondary) }
            ForEach(nearby.state?.paired ?? []) { peer in
                HStack { Text(peer.device_name); Spacer(); Button(L("sync.unpair"), role: .destructive) { nearby.remove(peer.id) } }
            }
        } header: { Text(L("sync.pin_sync")) } footer: { Text(L("sync.pin_sync_desc")) }
    }
}
