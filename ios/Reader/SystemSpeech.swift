import AVFoundation

// Render the system voice to a bounded audio file so both engines use the same
// player, pause state and application volume. This never changes device volume.
@MainActor final class SystemSpeech {
    private let synthesizer = AVSpeechSynthesizer()
    private var continuation: CheckedContinuation<SpeechAudio, Error>?
    private var file: AVAudioFile?
    private let url = FileManager.default.temporaryDirectory.appendingPathComponent("reader-speech-\(UUID().uuidString).caf")
    private var bytes = 0

    static func synthesize(text: String, voice: String, rate: Double) async throws -> SpeechAudio {
        let request = SystemSpeech()
        return try await withThrowingTaskGroup(of: SpeechAudio.self) { group in
            group.addTask { try await request.render(text: text, voice: voice, rate: rate) }
            group.addTask { try await Task.sleep(for: .seconds(45)); throw SpeechServiceError.timeout }
            defer { group.cancelAll() }
            return try await group.next()!
        }
    }
    private func render(text: String, voice: String, rate: Double) async throws -> SpeechAudio {
        guard let selected = AVSpeechSynthesisVoice(identifier: voice) else { throw SpeechServiceError.invalidVoice }
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = selected
        utterance.rate = min(AVSpeechUtteranceMaximumSpeechRate, max(AVSpeechUtteranceMinimumSpeechRate, AVSpeechUtteranceDefaultSpeechRate * Float(rate)))
        utterance.volume = 1
        return try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return try await withCheckedThrowingContinuation { continuation in
                self.continuation = continuation
                synthesizer.write(utterance) { [weak self] buffer in
                    // The synthesizer may reuse its buffer after this callback.
                    guard let pcm = buffer as? AVAudioPCMBuffer,
                          let copy = AVAudioPCMBuffer(pcmFormat: pcm.format, frameCapacity: max(1, pcm.frameLength)) else {
                        Task { @MainActor in self?.finish(.failure(SpeechServiceError.invalidResponse)) }
                        return
                    }
                    copy.frameLength = pcm.frameLength
                    let source = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: pcm.audioBufferList))
                    let destination = UnsafeMutableAudioBufferListPointer(copy.mutableAudioBufferList)
                    for index in source.indices {
                        if let from = source[index].mData, let to = destination[index].mData {
                            memcpy(to, from, Int(source[index].mDataByteSize))
                        }
                    }
                    Task { @MainActor in self?.receive(copy) }
                }
            }
        } onCancel: { Task { @MainActor in self.finish(.failure(CancellationError())) } }
    }
    private func receive(_ buffer: AVAudioPCMBuffer) {
        guard continuation != nil else { return }
        do {
            if buffer.frameLength == 0 {
                file = nil
                let data = try Data(contentsOf: url)
                guard bytes > 0 else { throw SpeechServiceError.noAudio }
                finish(.success(.init(data: data)))
                return
            }
            bytes += Int(buffer.frameLength) * Int(buffer.format.streamDescription.pointee.mBytesPerFrame)
            guard bytes <= 32 * 1024 * 1024 else { throw SpeechServiceError.tooLarge }
            if file == nil { file = try AVAudioFile(forWriting: url, settings: buffer.format.settings) }
            try file?.write(from: buffer)
        } catch { finish(.failure(error)) }
    }
    private func finish(_ result: Result<SpeechAudio, Error>) {
        guard let continuation else { return }
        self.continuation = nil
        synthesizer.stopSpeaking(at: .immediate)
        file = nil
        try? FileManager.default.removeItem(at: url)
        continuation.resume(with: result)
    }
}
