#if DEBUG
import AVFoundation
import Foundation

// Real iOS player/state checks use generated WAV; service checks use actual
// returned speech. This diagnostic owns a separate preferences suite only.
@MainActor enum SpeechRuntimeCheck {
    static func run() async {
        var report: [String: Any] = [:]
        let suite = "speech-check-\(UUID().uuidString)"
        let preferences = UserDefaults(suiteName: suite)!
        defer { preferences.removePersistentDomain(forName: suite) }
        do {
            let book = try JSONDecoder().decode(PreviewBook.self, from: Data(#"{"entry":{"id":"speech-fixture","title":"Speech fixture","path":"","last_chapter":0,"last_opened":0},"chapters":[{"title":"One","blocks":[{"type":"Paragraph","core_block":0,"spans":[{"text":"甲😀乙。","style":"Normal"}]},{"type":"Image","core_block":1},{"type":"Paragraph","core_block":2,"spans":[{"text":"丙丁。","style":"Normal"}]}]},{"title":"Two","blocks":[{"type":"Paragraph","core_block":0,"spans":[{"text":"戊己。","style":"Normal"}]}]}],"toc":[]}"#.utf8))
            let wav = waveform()
            let speech = ReadingSpeech(book: book, defaults: preferences) { _, _, _ in
                try await Task.sleep(for: .milliseconds(250))
                return .init(data: wav)
            }
            speech.start(at: .init(chapter: 0)); speech.pause()
            try await Task.sleep(for: .milliseconds(500))
            report["pause_during_preparation"] = speech.state == .paused
            speech.resume()
            report["resume_player"] = speech.state == .playing
            speech.pause(); speech.next()
            try await Task.sleep(for: .milliseconds(350))
            report["skip_image_and_remain_paused"] = speech.highlight?.start.block == 2 && speech.state == .paused
            speech.previous()
            try await Task.sleep(for: .milliseconds(350))
            report["previous_readable_block"] = speech.highlight?.start.block == 0 && speech.state == .paused
            speech.detach(); speech.next()
            report["manual_navigation_detaches"] = !speech.following
            speech.follow(); report["follow_recovers"] = speech.following
            speech.stop(); speech.start(at: .init(chapter: 0)); speech.next(); speech.stop()
            try await Task.sleep(for: .milliseconds(600))
            report["stop_rejects_stale_audio"] = speech.state == .idle && speech.highlight == nil
            speech.start(at: .init(chapter: 0, block: 2))
            report["cross_chapter_playback"] = await wait { speech.highlight?.start.chapter == 1 && speech.state == .playing }
            report["natural_completion"] = await wait { speech.state == .finished && speech.highlight == nil }
            speech.setVolume(0.45); speech.setRate(1.25); speech.setVoice("zh-CN-YunxiNeural")
            let reloaded = ReadingSpeech(book: book, defaults: preferences)
            report["settings_persist"] = reloaded.source == .online && reloaded.onlineVoice == "zh-CN-YunxiNeural" && reloaded.rate == 1.25 && reloaded.volume == 0.45
            speech.stop(); reloaded.stop()
            let failed = ReadingSpeech(book: book, defaults: preferences) { _, _, _ in throw SpeechServiceError.invalidResponse }
            failed.start(at: .init(chapter: 0))
            report["online_failure_no_silent_fallback"] = await wait { failed.state == .failed && failed.source == .online }
            failed.stop()
        } catch { report["state_check_error"] = safeError(error) }
        do {
            let online = try await EdgeSpeech.synthesize(text: "春江潮水连海平，海上明月共潮生。", voice: "zh-CN-XiaoxiaoNeural", rate: 1)
            let player = try AVAudioPlayer(data: online.data)
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .spokenAudio)
            try AVAudioSession.sharedInstance().setActive(true)
            report["online_audio_bytes"] = online.data.count
            report["online_word_markers"] = online.markers.count
            report["online_decoded_seconds"] = player.duration
            report["online_player_started"] = player.prepareToPlay() && player.play()
            try await Task.sleep(for: .milliseconds(400)); player.pause()
            report["online_player_advanced"] = player.currentTime > 0
            player.stop()
        } catch { report["online_error"] = safeError(error) }
        do {
            let voices = AVSpeechSynthesisVoice.speechVoices()
            report["system_voice_count"] = voices.count
            if let voice = AVSpeechSynthesisVoice(language: "zh-CN") ?? voices.first {
                let native = try await SystemSpeech.synthesize(text: "春江潮水连海平。", voice: voice.identifier, rate: 1)
                let player = try AVAudioPlayer(data: native.data)
                report["system_audio_bytes"] = native.data.count
                report["system_decoded_seconds"] = player.duration
                report["system_player_started"] = player.prepareToPlay() && player.play()
                try await Task.sleep(for: .milliseconds(400)); player.stop()
            }
        } catch { report["system_error"] = safeError(error) }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        let target = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("speech-verification.json")
        if let data = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]) { try? data.write(to: target, options: .atomic) }
    }
    private static func wait(_ condition: () -> Bool) async -> Bool {
        for _ in 0..<100 {
            if condition() { return true }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return false
    }
    private static func safeError(_ error: Error) -> String { let e = error as NSError; return "\(e.domain):\(e.code)" }
    private static func waveform() -> Data {
        let count = 8000
        var result = Data()
        func ascii(_ value: String) { result.append(contentsOf: value.utf8) }
        func u16(_ value: UInt16) { var v = value.littleEndian; withUnsafeBytes(of: &v) { result.append(contentsOf: $0) } }
        func u32(_ value: UInt32) { var v = value.littleEndian; withUnsafeBytes(of: &v) { result.append(contentsOf: $0) } }
        ascii("RIFF"); u32(UInt32(36 + count * 2)); ascii("WAVEfmt "); u32(16); u16(1); u16(1); u32(16000); u32(32000); u16(2); u16(16); ascii("data"); u32(UInt32(count * 2))
        for sample in 0..<count { u16(UInt16(bitPattern: Int16(sin(Double(sample) * 2 * .pi * 330 / 16000) * 1000))) }
        return result
    }
}
#endif
