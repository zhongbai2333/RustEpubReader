import SwiftUI
import AVFoundation

// Only value descriptors cross the worker boundary. Querying the system voice
// service may block on a cold device; it must never run while constructing a reader.
struct SystemSpeechVoice: Sendable {
    let identifier: String
    let name: String
    let language: String
}
struct SystemVoiceSnapshot: Sendable {
    let voices: [SystemSpeechVoice]
    let preferred: String
    func selected(_ saved: String) -> String {
        if voices.contains(where: {$0.identifier == saved}) { return saved }
        if voices.contains(where: {$0.identifier == preferred}) { return preferred }
        return voices.first?.identifier ?? ""
    }
}
actor SystemVoiceCatalog {
    static let shared = SystemVoiceCatalog()
    private let loader: @Sendable () async -> SystemVoiceSnapshot
    private var cached: SystemVoiceSnapshot?
    private var loadedAt: ContinuousClock.Instant?
    private var pending: Task<SystemVoiceSnapshot, Never>?
    init(loader: (@Sendable () async -> SystemVoiceSnapshot)? = nil) {
        self.loader = loader ?? { await SystemVoiceCatalog.query() }
    }
    func load() async -> SystemVoiceSnapshot {
        if let cached, let loadedAt, loadedAt.duration(to: .now) < .seconds(60) { return cached }
        if let pending { return await pending.value }
        let loader = self.loader
        let task = Task { await loader() }; pending = task
        let result = await task.value
        cached = result; loadedAt = .now; pending = nil
        return result
    }
    nonisolated private static func query() async -> SystemVoiceSnapshot {
        await Task.detached(priority: .utility) {
            let voices = AVSpeechSynthesisVoice.speechVoices().map {
                SystemSpeechVoice(identifier: $0.identifier, name: $0.name, language: $0.language)
            }.sorted { left, right in
                if left.language.hasPrefix("zh") != right.language.hasPrefix("zh") { return left.language.hasPrefix("zh") }
                return left.name < right.name
            }
            return SystemVoiceSnapshot(voices: voices,
                preferred: AVSpeechSynthesisVoice(language: "zh-CN")?.identifier ?? "")
        }.value
    }
}

enum SpeechState: Equatable { case idle, preparing, playing, paused, finished, failed }
enum SpeechSource: String { case online, system }

@MainActor final class ReadingSpeech: NSObject, ObservableObject, AVAudioPlayerDelegate {
    @Published private(set) var state = SpeechState.idle
    @Published private(set) var highlight: SpeechHighlight?
    @Published private(set) var index = 0
    @Published private(set) var source: SpeechSource
    @Published private(set) var onlineVoice: String
    @Published private(set) var systemVoice: String
    @Published private(set) var rate: Double
    @Published private(set) var volume: Double
    @Published var following = true
    private(set) var failureCode = ""
    let book: PreviewBook
    @Published private(set) var systemVoices: [SystemSpeechVoice] = []
    @Published private(set) var systemVoicesLoading = false
    private let voiceCatalog: SystemVoiceCatalog
    private var pendingVoiceLoads = 0
    private var audioSessionActive = false
    private(set) var units: [SpeechTextUnit] = []
    var audible: Bool { state == .playing && player?.isPlaying == true }
    private var player: AVAudioPlayer?
    private var load: Task<Void, Never>?
    private var prefetch: Task<SpeechAudio, Error>?
    private var prefetchIndex: Int?
    private var progress: Task<Void, Never>?
    private var generation = 0
    private var paused = false
    private var audio: SpeechAudio?
    private var utteranceUnit: SpeechTextUnit?
    private var observers: [NSObjectProtocol] = []
    private var correctionKeys: [String]?
    private var chapterCorrections: [ReaderCorrection] = []
    private var speechChapter: Int?
    private var chapterFailure: Error?
    private let defaults: UserDefaults
    private let synthesisOverride: ((String, String, Double) async throws -> SpeechAudio)?
    var hasSession: Bool { state == .preparing || state == .playing || state == .paused || state == .failed }
    var canPrevious: Bool { previousIndex != nil || (book.loadsChapters && (speechChapter ?? 0) > 0) }
    var canNext: Bool { nextIndex != nil || (book.loadsChapters && (speechChapter ?? book.chapters.count)+1 < book.chapters.count) }
    var preview: String { utteranceUnit?.text ?? (units.indices.contains(index) ? units[index].text : "") }
    var chapterTitle: String {
        let chapter = highlight?.start.chapter ?? (units.indices.contains(index) ? units[index].anchor.chapter : 0)
        return book.chapters.indices.contains(chapter) ? book.chapters[chapter].title : book.title
    }
    private var previousIndex: Int? {
        guard units.indices.contains(index) else { return nil }
        let current = units[index].anchor
        guard let previous = units.indices[..<index].last(where: { units[$0].anchor.chapter != current.chapter || units[$0].anchor.block != current.block }) else { return nil }
        let target = units[previous].anchor
        return units.indices.first(where: { units[$0].anchor.chapter == target.chapter && units[$0].anchor.block == target.block })
    }
    private var nextIndex: Int? {
        guard units.indices.contains(index) else { return nil }
        let current = units[index].anchor
        return units.indices[(index + 1)...].first(where: { units[$0].anchor.chapter != current.chapter || units[$0].anchor.block != current.block })
    }

    init(book: PreviewBook, defaults: UserDefaults = .standard,
         voiceCatalog: SystemVoiceCatalog = .shared,
         synthesis: ((String, String, Double) async throws -> SpeechAudio)? = nil) {
        self.book = book; self.defaults = defaults; self.voiceCatalog = voiceCatalog; synthesisOverride = synthesis
        source = SpeechSource(rawValue: defaults.string(forKey: "reader.speech.source") ?? "") ?? .online
        let savedOnline = defaults.string(forKey: "reader.speech.onlineVoice") ?? ""
        onlineVoice = EdgeSpeech.voices.contains(where: { $0.0 == savedOnline }) ? savedOnline : "zh-CN-XiaoxiaoNeural"
        systemVoice = defaults.string(forKey: "reader.speech.systemVoice") ?? ""
        rate = defaults.object(forKey: "reader.speech.rate") == nil ? 1 : min(3, max(0.5, defaults.double(forKey: "reader.speech.rate")))
        volume = defaults.object(forKey: "reader.speech.volume") == nil ? 1 : min(1, max(0, defaults.double(forKey: "reader.speech.volume")))
        super.init()
        refreshText(corrections: [])
        observers.append(NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] notification in
            if (notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? NSNumber)?.uintValue == AVAudioSession.InterruptionType.began.rawValue {
                Task { @MainActor in self?.pause() }
            }
        })
        observers.append(NotificationCenter.default.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { [weak self] notification in
            if (notification.userInfo?[AVAudioSessionRouteChangeReasonKey] as? NSNumber)?.uintValue == AVAudioSession.RouteChangeReason.oldDeviceUnavailable.rawValue {
                Task { @MainActor in self?.pause() }
            }
        })
    }
    deinit { observers.forEach(NotificationCenter.default.removeObserver) }

    func prepareSystemVoices() async {
        guard systemVoices.isEmpty else { return }
        pendingVoiceLoads += 1; systemVoicesLoading = true
        defer { pendingVoiceLoads -= 1; systemVoicesLoading = pendingVoiceLoads > 0 }
        let snapshot = await voiceCatalog.load()
        guard !Task.isCancelled else { return }
        systemVoices = snapshot.voices
        systemVoice = snapshot.selected(systemVoice)
    }
    private func deactivateAudioSession(requested: Bool = false) {
        // System synthesis can touch the session before a player exists. Cleanup
        // still runs for a cancelled/failed request, but not for an unused reader.
        guard requested || audioSessionActive else { return }
        audioSessionActive = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func refreshText(corrections: [ReaderCorrection]) {
        let keys = corrections.filter { $0.status == "accepted" }.map { "\($0.chapter)/\($0.block_idx)/\($0.char_offset)/\($0.original)/\($0.corrected)" }
        guard keys != correctionKeys else { return }
        correctionKeys = keys
        let restore = highlight?.start
        let active = state == .playing || state == .preparing || state == .paused
        let wasPaused = paused
        chapterCorrections=corrections
        units=makeUnits(indices:book.loadsChapters ? speechChapter.map {[$0]} ?? [] : Array(book.chapters.indices),corrections:corrections)
        if active, let restore { start(at: restore, remainPaused: wasPaused) }
    }
    private func makeUnits(indices:[Int],corrections:[ReaderCorrection]) -> [SpeechTextUnit] {
        let sources = indices.flatMap { chapter -> [SpeechSourceBlock] in
            let content:BookChapter
            do {content=try book.chapter(at:chapter)} catch {chapterFailure=error;return []}
            return content.blocks.enumerated().compactMap { index, block -> SpeechSourceBlock? in
                guard block.type == "Paragraph" || block.type == "Heading" else { return nil }
                var scalars = Array((block.spans ?? []).map(\.text).joined().unicodeScalars)
                let blockIndex = block.core_block ?? index, base = block.core_char_offset ?? 0
                for correction in corrections where correction.chapter == chapter && correction.block_idx == blockIndex && correction.status == "accepted" {
                    let offset = correction.char_offset - base
                    let old = Array(correction.original.unicodeScalars), replacement = Array(correction.corrected.unicodeScalars)
                    if old.count == replacement.count && offset >= 0 && offset + old.count <= scalars.count && Array(scalars[offset..<(offset + old.count)]) == old {
                        scalars.replaceSubrange(offset..<(offset + old.count), with: replacement)
                    }
                }
                return .init(text: String(String.UnicodeScalarView(scalars)), anchor: .init(chapter: chapter, block: blockIndex, charOffset: base))
            }
        }
        return SpeechText.units(sources,from:.init(chapter:0))
    }
    private func prepareChapter(_ chapter:Int, direction:Int = 1) {
        guard book.loadsChapters else {return}
        chapterFailure=nil
        var next=chapter
        while book.chapters.indices.contains(next) {
            speechChapter=next;units=makeUnits(indices:[next],corrections:chapterCorrections)
            if let chapterFailure {fail(chapterFailure);return}
            if !units.isEmpty {return}
            next += direction
        }
        units=[]
    }

    func startingIndex(at anchor: ReadingAnchor) -> Int? {
        units.firstIndex { unit in
            unit.anchor.chapter > anchor.chapter || unit.anchor.chapter == anchor.chapter &&
                (unit.anchor.block > anchor.block || unit.anchor.block == anchor.block && unit.end.charOffset > anchor.charOffset)
        }
    }
    func preview(at anchor: ReadingAnchor) -> String {
        let available = book.loadsChapters ? makeUnits(indices:[anchor.chapter],corrections:chapterCorrections) : units
        guard let unit = available.first(where: {$0.end.chapter > anchor.chapter || $0.end.chapter == anchor.chapter && ($0.end.block > anchor.block || $0.end.block == anchor.block && $0.end.charOffset > anchor.charOffset)}) else {return ""}
        return SpeechText.units([.init(text: unit.text, anchor: unit.anchor)], from: anchor).first?.text ?? unit.text
    }
    func title(at anchor: ReadingAnchor) -> String {
        book.chapters.indices.contains(anchor.chapter) ? book.chapters[anchor.chapter].title : book.title
    }
    func start(at anchor: ReadingAnchor, remainPaused: Bool = false) {
        if book.loadsChapters && speechChapter != anchor.chapter { prepareChapter(anchor.chapter) }
        if chapterFailure != nil {return}
        guard let target = startingIndex(at: anchor) else { stop(); state = .finished; return }
        play(target, from: anchor, remainPaused: remainPaused)
    }
    func toggle(at anchor: ReadingAnchor) {
        if state == .paused { resume() }
        else if state == .playing || state == .preparing { pause() }
        else if state == .failed { start(at: highlight?.start ?? anchor) }
        else { start(at: anchor) }
    }
    func pause() {
        guard state == .playing || state == .preparing else { return }
        paused = true; player?.pause(); state = .paused
    }
    func resume() {
        guard state == .paused else { return }
        paused = false
        if let player { if player.play() { state = .playing } else { fail(SpeechServiceError.noAudio) } }
        else { state = .preparing }
    }
    func stop() {
        let requested = hasSession
        cancel()
        paused = false; highlight = nil; utteranceUnit = nil; state = .idle
        deactivateAudioSession(requested: requested)
    }
    func previous() {
        if let target=previousIndex {play(target,remainPaused:paused)}
        else if book.loadsChapters, let chapter=speechChapter, chapter > 0 {
            prepareChapter(chapter-1,direction:-1)
            if let last=units.last {play(units.firstIndex(where: {$0.anchor.block == last.anchor.block}) ?? 0,remainPaused:paused)}
        }
    }
    func next() {
        if let target=nextIndex {play(target,remainPaused:paused)}
        else if book.loadsChapters, let chapter=speechChapter, chapter+1 < book.chapters.count {prepareChapter(chapter+1);play(0,remainPaused:paused)}
    }
    func follow() { following = true }
    func detach() { if hasSession { following = false } }
    func setSource(_ value: SpeechSource) { source = value; defaults.set(value.rawValue, forKey: "reader.speech.source"); restart() }
    func setVoice(_ value: String) {
        if source == .online { onlineVoice = value; defaults.set(value, forKey: "reader.speech.onlineVoice") }
        else { systemVoice = value; defaults.set(value, forKey: "reader.speech.systemVoice") }
        restart()
    }
    func setRate(_ value: Double) { rate = min(3, max(0.5, value)); defaults.set(rate, forKey: "reader.speech.rate"); restart() }
    func setVolume(_ value: Double) {
        volume = min(1, max(0, value)); defaults.set(volume, forKey: "reader.speech.volume"); player?.volume = Float(volume)
    }
    private func restart() {
        guard state == .playing || state == .preparing || state == .paused || state == .failed else { return }
        let follow = following
        if let anchor = highlight?.start { start(at: anchor, remainPaused: paused) }
        else if units.indices.contains(index) { play(index, remainPaused: paused) }
        following = follow
    }
    private func cancel() {
        generation += 1
        load?.cancel(); load = nil
        prefetch?.cancel(); prefetch = nil; prefetchIndex = nil
        progress?.cancel(); progress = nil
        player?.delegate = nil; player?.stop(); player = nil; audio = nil
    }
    private func synthesize(_ text: String, source: SpeechSource, voice: String, rate: Double) async throws -> SpeechAudio {
        if let synthesisOverride { return try await synthesisOverride(text, voice, rate) }
        if source == .online { return try await EdgeSpeech.synthesize(text: text, voice: voice, rate: rate) }
        let available = await voiceCatalog.load()
        try Task.checkCancellation()
        return try await SystemSpeech.synthesize(text: text, voice: available.selected(voice), rate: rate)
    }
    private func play(_ target: Int, from anchor: ReadingAnchor? = nil, remainPaused: Bool = false) {
        if book.loadsChapters, target >= units.count, let chapter=speechChapter, chapter+1 < book.chapters.count {
            prepareChapter(chapter+1);play(0,remainPaused:remainPaused);return
        }
        if chapterFailure != nil {return}
        guard units.indices.contains(target) else { stop(); state = .finished; return }
        let promoted = prefetchIndex == target && anchor == nil ? prefetch : nil
        if promoted != nil { prefetch = nil; prefetchIndex = nil }
        cancel()
        index = target; paused = remainPaused; failureCode = ""
        let unit = units[target]
        utteranceUnit = anchor.flatMap { SpeechText.units([.init(text: unit.text, anchor: unit.anchor)], from: $0).first } ?? unit
        guard let spoken = utteranceUnit else { return }
        state = paused ? .paused : .preparing
        highlight = .init(start: spoken.anchor, end: spoken.end)
        let token = generation, selectedSource = source, voice = source == .online ? onlineVoice : systemVoice, speed = rate
        load = Task { [weak self] in
            guard let self else { return }
            do {
                let result: SpeechAudio
                if let promoted {
                    result = try await withTaskCancellationHandler { try await promoted.value } onCancel: { promoted.cancel() }
                }
                else { result = try await self.synthesize(spoken.text, source: selectedSource, voice: voice, rate: speed) }
                try Task.checkCancellation()
                guard token == self.generation else { return }
                let session = AVAudioSession.sharedInstance()
                try session.setCategory(.playback, mode: .spokenAudio)
                try session.setActive(true)
                self.audioSessionActive = true
                let player = try AVAudioPlayer(data: result.data)
                guard player.duration > 0, player.prepareToPlay() else { throw SpeechServiceError.noAudio }
                player.delegate = self; player.volume = Float(self.volume)
                self.player = player; self.audio = result
                if self.paused { self.state = .paused }
                else { guard player.play() else { throw SpeechServiceError.noAudio }; self.state = .playing }
                self.trackProgress(token: token)
                self.prefetchNext()
            } catch {
                if token == self.generation && !Task.isCancelled { self.fail(error) }
            }
        }
    }
    private func prefetchNext() {
        let next = index + 1
        guard units.indices.contains(next) else { return }
        prefetchIndex = next
        let text = units[next].text, selectedSource = source, voice = source == .online ? onlineVoice : systemVoice, speed = rate
        prefetch = Task { [weak self] in
            guard let self else { throw CancellationError() }
            return try await self.synthesize(text, source: selectedSource, voice: voice, rate: speed)
        }
    }
    private func trackProgress(token: Int) {
        progress = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(150))
                guard let self, token == self.generation else { return }
                if self.state == .playing, let player = self.player, let unit = self.utteranceUnit,
                   let marker = self.audio?.markers.last(where: { $0.time <= player.currentTime }) {
                    let next = SpeechHighlight(start: .init(chapter: unit.anchor.chapter, block: unit.anchor.block, charOffset: unit.anchor.charOffset + marker.offset),
                        end: .init(chapter: unit.anchor.chapter, block: unit.anchor.block, charOffset: unit.anchor.charOffset + marker.offset + marker.length))
                    if next != self.highlight { self.highlight = next }
                }
            }
        }
    }
    private func fail(_ error: Error) {
        let detail = error as NSError
        failureCode = "\(detail.domain):\(detail.code)"
        cancel(); paused = false; state = .failed
        deactivateAudioSession(requested: true)
    }
    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        Task { @MainActor [weak self] in
            guard let self, self.player === player else { return }
            if flag { self.play(self.index + 1, remainPaused: self.paused) }
            else { self.fail(SpeechServiceError.noAudio) }
        }
    }
    nonisolated func audioPlayerDecodeErrorDidOccur(_ player: AVAudioPlayer, error: Error?) {
        Task { @MainActor [weak self] in
            guard let self, self.player === player else { return }
            self.fail(error ?? SpeechServiceError.invalidResponse)
        }
    }
}
