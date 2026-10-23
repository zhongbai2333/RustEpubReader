import Foundation
import SwiftUI
import Darwin
struct PairedReader: Decodable, Identifiable { let device_id: String; let device_name: String; var id: String { device_id } }
struct NearbyState: Decodable { let address: String; let status: String; let device_id:String; let device_name:String; let paired: [PairedReader]; let result: SyncSummary? }
@MainActor final class NearbySync: ObservableObject {
    @Published var listening = false
    @Published var busy = false
    @Published var address = ""
    @Published var pin = ""
    @Published var remoteAddress = ""
    @Published var remotePin = ""
    @Published var state: NearbyState?
    @Published var error: String?
    @Published var discovered:[NativeNearbyPeer]=[]
    private let discovery=NativeNearbyDiscovery()
    private var foreground=true
    private var desiredListening=false
    private var discoveryRequested=false
    private var lastAttempt:[String:Date]=[:]
    func setForeground(_ active:Bool){foreground=active;if active{Task{await resumeIfPaired(active)}}else{let wanted=desiredListening;stop();desiredListening=wanted;discovery.stop();discovered=[]}}
    func resumeIfPaired(_ active:Bool) async{foreground=active;guard active else{return};await refresh();guard foreground else{return};if discoveryRequested || !(state?.paired.isEmpty ?? true){browse()};if desiredListening && !listening{start()}}
    func browse(){
        guard foreground else{return};discoveryRequested=true
        discovery.onPeers={ [weak self] peers in Task{@MainActor in
            guard let self,self.foreground else{return};self.discovered=peers.filter{$0.id != self.state?.device_id}
            self.reconnectIfAvailable()
        }}
        discovery.start()
    }
    private func reconnectIfAvailable(){
        guard foreground,!busy,let peer=discovered.first(where:{p in state?.paired.contains(where:{$0.id==p.id})==true}),Date().timeIntervalSince(lastAttempt[peer.id] ?? .distantPast)>60 else{return}
        lastAttempt[peer.id]=Date();remoteAddress=peer.address;remotePin="";connect(peer:peer.id)
    }
    func connectDiscovered(_ peer:NativeNearbyPeer){remoteAddress=peer.address;if state?.paired.contains(where:{$0.id==peer.id})==true {remotePin="";connect(peer:peer.id)}}

    private var intent = UUID()
    private var operation: Task<Void,Never>?
    private var polling: Task<Void,Never>?
    let library: PreviewLibrary
    init(library: PreviewLibrary) { self.library = library }
    func stop() { desiredListening=false; discovery.unpublish(); intent = UUID(); reader_ios_stop_nearby(); operation?.cancel(); polling?.cancel(); busy = false; listening = false; address = "" }
    func refresh() async {
        let client = library.client
        do { state = try await Task.detached { try client.call("peer_state", as: NearbyState.self) }.value; address = state?.address ?? ""; listening = !address.isEmpty
            if let state,listening,let port=Int32(address.split(separator:":").last ?? "") {discovery.publish(id:state.device_id,name:state.device_name,port:port)}else{discovery.unpublish()}
            if let summary = state?.result { library.offer = summary.offer; await library.reload() }
            reconnectIfAvailable()
        }
        catch { self.error = L("sync.error_connection_failed") }
    }
    func start() {
        guard !busy else { return }; polling?.cancel();polling=nil;desiredListening=true; error = nil; busy = true
        pin = String(format: "%04d", Int.random(in: 0...9999))
        let generation = UUID(); intent = generation; let client = library.client, pin = self.pin
        operation = Task {
            do { let _: [String: Bool] = try await Task.detached { try client.call("peer_start", ["pin":pin]) }.value
                guard intent == generation && !Task.isCancelled else { return }
                busy = false; listening = true; browse(); await refresh()
                polling = Task { while !Task.isCancelled { try? await Task.sleep(for: .seconds(2)); if !Task.isCancelled { await refresh() } } }
            } catch { if intent == generation { busy = false; self.error = L("sync.error_connection_failed") } }
        }
    }
    func connect(peer: String? = nil) {
        guard !busy else { return };polling?.cancel();polling=nil; error = nil; busy = true
        if let peer {lastAttempt[peer]=Date()}
        let generation = UUID(); intent = generation; let client = library.client, address = remoteAddress, pin = remotePin, path = library.openPath
        operation = Task {
            await library.flush()
            do {
                let summary: SyncSummary = try await Task.detached { try client.call("peer_connect", ["address":address,"pin":pin,"open_path":path,"peer_id":peer ?? ""]) }.value
                guard intent == generation && !Task.isCancelled else { return }
                library.offer = summary.offer; await library.reload(); await refresh(); busy = false
                if foreground && desiredListening && !listening { start() }
            } catch { if intent == generation { busy = false; self.error = L("sync.error_connection_failed");await refresh();if foreground && desiredListening && !listening {let message=self.error;start();self.error=message} } }
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
            Text(L("services.foreground")).font(.footnote).foregroundStyle(.secondary)
            ForEach(nearby.discovered){peer in
                Button{nearby.connectDiscovered(peer)}label:{VStack(alignment:.leading){Text(peer.name);Text(peer.address).font(.caption).foregroundStyle(.secondary)}}.disabled(nearby.busy)
            }
            ForEach(nearby.state?.paired ?? []) { peer in
                HStack { Text(peer.device_name); Spacer(); if let discovered=nearby.discovered.first(where:{$0.id==peer.id}) {Button(L("services.reconnect")){nearby.connectDiscovered(discovered)}.disabled(nearby.busy)}; Button(L("sync.unpair"), role: .destructive) { nearby.remove(peer.id) } }
            }
        } header: { Text(L("sync.pin_sync")) } footer: { Text(L("sync.pin_sync_desc")) }
        .task{nearby.browse();await nearby.refresh()}
    }
}

struct NativeNearbyPeer:Identifiable {let id:String;let name:String;let address:String}
/// Foundation owns Bonjour sockets and permissions; untrusted TXT never grants pairing.
final class NativeNearbyDiscovery:NSObject,NetServiceBrowserDelegate,NetServiceDelegate {
    var onPeers:(([NativeNearbyPeer])->Void)?
    private var browser:NetServiceBrowser?
    private var services:[String:NetService]=[:]
    private var peers:[String:NativeNearbyPeer]=[:]
    private var advertisement:NetService?
    private var advertisedPort:Int32=0
    func start(){guard browser==nil else{return};let browser=NetServiceBrowser();self.browser=browser;browser.delegate=self;browser.searchForServices(ofType:"_epubreader._tcp.",inDomain:"local.")}
    func stop(){browser?.stop();browser?.delegate=nil;browser=nil;for service in services.values{service.stop();service.delegate=nil};services=[:];peers=[:];onPeers?([]);unpublish()}
    func publish(id:String,name:String,port:Int32){
        guard !id.isEmpty,port>0 else{return};if advertisement != nil && advertisedPort==port{return};unpublish()
        let service=NetService(domain:"local.",type:"_epubreader._tcp.",name:"reader-\(id.prefix(24))",port:port)
        service.setTXTRecord(NetService.data(fromTXTRecord:["id":Data(id.utf8),"name":Data(name.prefix(80).utf8)]));advertisement=service;advertisedPort=port;service.publish()
    }
    func unpublish(){advertisement?.stop();advertisement=nil;advertisedPort=0}
    func netServiceBrowser(_ browser:NetServiceBrowser,didFind service:NetService,moreComing:Bool){services[service.name]=service;service.delegate=self;service.resolve(withTimeout:5)}
    func netServiceBrowser(_ browser:NetServiceBrowser,didRemove service:NetService,moreComing:Bool){services.removeValue(forKey:service.name)?.stop();peers.removeValue(forKey:service.name);onPeers?(Array(peers.values))}
    func netServiceDidResolveAddress(_ sender:NetService){
        guard services[sender.name] === sender,let data=sender.txtRecordData() else{return};let txt=NetService.dictionary(fromTXTRecord:data)
        guard let idData=txt["id"],let id=String(data:idData,encoding:.utf8),UUID(uuidString:id) != nil else{return}
        let name=txt["name"].flatMap{String(data:$0,encoding:.utf8)} ?? sender.name
        for address in sender.addresses ?? [] {
            let host:String?=address.withUnsafeBytes{raw in
                guard let ptr=raw.baseAddress?.assumingMemoryBound(to:sockaddr.self),ptr.pointee.sa_family==sa_family_t(AF_INET) else{return nil}
                var host=[CChar](repeating:0,count:Int(NI_MAXHOST));guard getnameinfo(ptr,socklen_t(address.count),&host,socklen_t(host.count),nil,0,NI_NUMERICHOST)==0 else{return nil};return String(cString:host)
            }
            if let host{peers[sender.name]=NativeNearbyPeer(id:id,name:String(name.prefix(80)),address:"\(host):\(sender.port)");onPeers?(Array(peers.values));break}
        }
    }
    func netService(_ sender:NetService,didNotResolve errorDict:[String:NSNumber]){services.removeValue(forKey:sender.name);peers.removeValue(forKey:sender.name);onPeers?(Array(peers.values))}
}
