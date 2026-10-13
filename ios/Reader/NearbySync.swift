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
        catch { self.error = "连接失败，请检查地址、PIN 和本地网络权限。" }
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
            } catch { if intent == generation { busy = false; self.error = "连接失败，请检查地址、PIN 和本地网络权限。" } }
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
            } catch { if intent == generation { busy = false; self.error = "连接失败，请检查地址、PIN 和本地网络权限。" } }
        }
    }
    func remove(_ id: String) { let client = library.client; Task { let _: [String:Bool]? = try? await Task.detached { try client.call("peer_remove",["peer_id":id]) }.value; await refresh() } }
}
struct NearbySyncView: View {
    @ObservedObject var nearby: NearbySync
    var body: some View {
        Section {
            Toggle("允许设备连接", isOn: Binding(get: { nearby.listening }, set: { if $0 { nearby.start() } else { nearby.stop() } })).disabled(nearby.busy)
            if nearby.listening { LabeledContent("本机地址", value: nearby.address).textSelection(.enabled); LabeledContent("本机 PIN", value: nearby.pin).monospacedDigit().textSelection(.enabled) }
            TextField("对方设备地址", text: $nearby.remoteAddress, prompt: Text("192.168.1.2:14528")).textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.numbersAndPunctuation)
            TextField("对方 PIN", text: $nearby.remotePin).keyboardType(.numberPad).onChange(of: nearby.remotePin) { _, value in nearby.remotePin = String(value.filter(\.isNumber).prefix(4)) }
            if nearby.busy { ProgressView(); Button("取消连接") { nearby.stop() } }
            else { Button("连接并同步") { nearby.connect() }.disabled(nearby.remoteAddress.isEmpty || nearby.remotePin.count != 4) }
            if let error = nearby.error { Text(error).font(.footnote).foregroundStyle(.red) }
            if let state = nearby.state, state.status == "synced" { Text("已同步").foregroundStyle(.secondary) }
            ForEach(nearby.state?.paired ?? []) { peer in
                HStack { Text(peer.device_name); Spacer(); Button("解除配对", role: .destructive) { nearby.remove(peer.id) } }
            }
        } header: { Text("PIN 同步") } footer: { Text("仅在同一网络中使用。开启后，配对设备可读取本机书籍和阅读进度；离开前台时连接会停止。请在对方设备的同步页查看地址和四位 PIN。") }
    }
}
