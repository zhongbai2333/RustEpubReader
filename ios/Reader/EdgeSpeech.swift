import Foundation

struct SpeechMarker {
    let time: Double
    let offset: Int
    let length: Int
}
struct SpeechAudio {
    let data: Data
    var markers: [SpeechMarker] = []
}
enum SpeechServiceError: Error {
    case timeout, invalidResponse, tooLarge, noAudio, invalidVoice
}

enum EdgeSpeech {
    // Same service and voice IDs as desktop/Android. Omit retired presets.
    static var voices: [(String, String)] { [
        ("zh-CN-XiaoxiaoNeural", L("tts.voice_xiaoxiao")), ("zh-CN-YunyangNeural", L("tts.voice_yunyang")),
        ("zh-CN-XiaoyiNeural", L("tts.voice_xiaoyi")), ("zh-CN-YunjianNeural", L("tts.voice_yunjian")),
        ("zh-CN-YunxiNeural", L("tts.voice_yunxi")),
        ("en-US-AriaNeural", "Aria · en-US"), ("en-US-GuyNeural", "Guy · en-US"), ("ja-JP-NanamiNeural", "Nanami · ja-JP")
    ] }
    private static let serviceToken = "6A5AA1D4EAFF4E9FB37E23D68491D6F4"

    static func synthesize(text: String, voice: String, rate: Double) async throws -> SpeechAudio {
        guard voices.contains(where: { $0.0 == voice }) else { throw SpeechServiceError.invalidVoice }
        guard !text.isEmpty, text.utf8.count <= 8192 else { throw SpeechServiceError.tooLarge }
        let hash = EdgeSpeechToken.generate(at: Date())
        let connection = UUID().uuidString.replacingOccurrences(of: "-", with: "")
        let url = URL(string: "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1?TrustedClientToken=\(serviceToken)&ConnectionId=\(connection)&Sec-MS-GEC=\(hash)&Sec-MS-GEC-Version=2-143.0.3650.75")!
        var request = URLRequest(url: url, timeoutInterval: 30)
        request.setValue("chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold", forHTTPHeaderField: "Origin")
        request.setValue("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36 Edg/143.0.0.0", forHTTPHeaderField: "User-Agent")
        request.setValue("no-cache", forHTTPHeaderField: "Cache-Control")
        request.setValue("no-cache", forHTTPHeaderField: "Pragma")
        request.setValue("en-US,en;q=0.9", forHTTPHeaderField: "Accept-Language")
        request.setValue("muid=\(UUID().uuidString.replacingOccurrences(of: "-", with: ""));", forHTTPHeaderField: "Cookie")
        let socket = URLSession.shared.webSocketTask(with: request)
        socket.resume()
        defer { socket.cancel(with: .normalClosure, reason: nil) }
        return try await withThrowingTaskGroup(of: SpeechAudio.self) { group in
            group.addTask {
                try await withTaskCancellationHandler {
                    let timestamp = DateFormatter()
                    timestamp.locale = Locale(identifier: "en_US_POSIX")
                    timestamp.timeZone = TimeZone(secondsFromGMT: 0)
                    timestamp.dateFormat = "EEE, dd MMM yyyy HH:mm:ss 'GMT'"
                    let date = timestamp.string(from: Date())
                    let config = "X-Timestamp:\(date)\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n{\"context\":{\"synthesis\":{\"audio\":{\"metadataoptions\":{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"true\"},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}"
                    try await socket.send(.string(config))
                    let percent = Int((min(3, max(0.5, rate)) - 1) * 100)
                    let ssml = "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'><voice name='\(voice)'><prosody rate='\(percent >= 0 ? "+" : "")\(percent)%' volume='+0%'>\(SpeechText.xml(text))</prosody></voice></speak>"
                    try await socket.send(.string("X-RequestId:\(connection)\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:\(date)\r\nPath:ssml\r\n\r\n\(ssml)"))
                    var data = Data()
                    var metadata: [(Double, String)] = []
                    while true {
                        try Task.checkCancellation()
                        switch try await socket.receive() {
                        case .data(let packet):
                            let bytes = try audioPayload(packet)
                            guard data.count + bytes.count <= 8 * 1024 * 1024 else { throw SpeechServiceError.tooLarge }
                            data.append(bytes)
                        case .string(let message):
                            guard let separator = message.range(of: "\r\n\r\n") else { continue }
                            let header = String(message[..<separator.lowerBound])
                            if header.components(separatedBy: "\r\n").contains("Path:turn.end") {
                                guard !data.isEmpty else { throw SpeechServiceError.noAudio }
                                return SpeechAudio(data: data, markers: markers(metadata, text: text))
                            }
                            if header.components(separatedBy: "\r\n").contains("Path:audio.metadata"),
                               let json = String(message[separator.upperBound...]).data(using: .utf8),
                               let root = try? JSONSerialization.jsonObject(with: json) as? [String: Any],
                               let items = root["Metadata"] as? [[String: Any]] {
                                for item in items where item["Type"] as? String == "WordBoundary" {
                                    if let value = item["Data"] as? [String: Any], let offset = value["Offset"] as? NSNumber,
                                       let word = value["text"] as? [String: Any], let spoken = word["Text"] as? String,
                                       metadata.count < 8192 { metadata.append((offset.doubleValue / 10_000_000, spoken)) }
                                }
                            }
                        @unknown default: throw SpeechServiceError.invalidResponse
                        }
                    }
                } onCancel: { socket.cancel(with: .goingAway, reason: nil) }
            }
            group.addTask {
                try await Task.sleep(for: .seconds(30))
                throw SpeechServiceError.timeout
            }
            defer { group.cancelAll() }
            return try await group.next()!
        }
    }

    static func audioPayload(_ packet: Data) throws -> Data {
        guard packet.count >= 2 else { throw SpeechServiceError.invalidResponse }
        let length = Int(packet[0]) * 256 + Int(packet[1])
        guard length <= packet.count - 2,
              let header = String(data: packet[2..<(2 + length)], encoding: .utf8) else { throw SpeechServiceError.invalidResponse }
        guard header.components(separatedBy: "\r\n").contains("Path:audio") else { return Data() }
        return Data(packet[(2 + length)...])
    }

    static func markers(_ metadata: [(Double, String)], text: String) -> [SpeechMarker] {
        var cursor = text.startIndex
        var result: [SpeechMarker] = []
        for (time, word) in metadata where !word.isEmpty {
            guard let range = text.range(of: word, options: .literal, range: cursor..<text.endIndex) else { continue }
            result.append(.init(time: time, offset: text[..<range.lowerBound].unicodeScalars.count, length: text[range].unicodeScalars.count))
            cursor = range.upperBound
        }
        return result.sorted { $0.time < $1.time }
    }
}
