import SwiftUI

struct ReadingSpeechPanel: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var speech: ReadingSpeech
    let current: ReadingAnchor
    @State private var speed = 1.0
    private var title: String { speech.hasSession ? speech.chapterTitle : speech.title(at: current) }
    private var excerpt: String { speech.hasSession ? speech.preview : speech.preview(at: current) }
    private var excerptIsTitle: Bool {
        excerpt.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ") ==
            title.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
    }
    var body: some View {
        Form {
            Section {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(title).font(.subheadline).fontWeight(.medium).lineLimit(2)
                        Text(speech.statusLabel).font(.caption).foregroundStyle(speech.state == .failed ? Color.red : .secondary)
                        if !excerpt.isEmpty && !excerptIsTitle { Text(excerpt).font(.subheadline).foregroundStyle(.secondary).lineLimit(2) }
                    }
                    HStack(spacing: 32) {
                        Button { speech.previous() } label: {
                            Image(systemName: "backward.end.fill").font(.system(size: 18, weight: .semibold))
                                .frame(width: 44, height: 44).contentShape(Rectangle())
                        }.disabled(!speech.canPrevious || !speech.hasSession).accessibilityLabel(L("tts.previous_paragraph"))
                        Button { speech.toggle(at: current) } label: {
                            Image(systemName: speech.actionSymbol).font(.system(size: 24, weight: .semibold))
                                .offset(x: speech.actionSymbol == "play.fill" ? 1 : 0)
                                .frame(width: 50, height: 50)
                                .readerGlass(in: Circle(), interactive: true)
                                .contentShape(Circle())
                                .opacity(!speech.hasSession && speech.startingIndex(at: current) == nil ? 0.35 : 1)
                        }.disabled(!speech.hasSession && speech.startingIndex(at: current) == nil)
                            .accessibilityLabel(speech.actionLabel)
                        Button { speech.next() } label: {
                            Image(systemName: "forward.end.fill").font(.system(size: 18, weight: .semibold))
                                .frame(width: 44, height: 44).contentShape(Rectangle())
                        }.disabled(!speech.canNext || !speech.hasSession).accessibilityLabel(L("tts.next_paragraph"))
                    }.buttonStyle(.plain).foregroundStyle(.primary)
                        .frame(maxWidth: .infinity).padding(.vertical, 8)
                }
                if speech.hasSession {
                    Button { speech.stop() } label: {
                        Label(L("tts.stop_reading"), systemImage: "stop.fill").foregroundStyle(.secondary)
                    }.buttonStyle(.plain)
                }
                if speech.state == .failed {
                    Text(speech.source == .online ? L("tts.error_unavailable_network") : L("tts.error_unavailable_retry"))
                        .font(.footnote).foregroundStyle(.secondary)
                }
                Toggle(L("tts.follow_reading_label"), isOn: $speech.following)
                if !speech.following && speech.hasSession {
                    Button(L("tts.return_to_position"), systemImage: "location.fill") { speech.follow() }
                }
            } footer: {
                if !speech.hasSession && speech.startingIndex(at: current) == nil { Text(L("tts.no_content_after_position")) }
            }
            Section {
                Picker(L("tts.voice_source"), selection: Binding(get: { speech.source }, set: { speech.setSource($0) })) {
                    Text(L("tts.online_voice")).tag(SpeechSource.online)
                    Text(L("tts.system_voice")).tag(SpeechSource.system)
                }.pickerStyle(.segmented)
                Picker(L("tts.voice_timbre"), selection: Binding(get: { speech.source == .online ? speech.onlineVoice : speech.systemVoice }, set: { speech.setVoice($0) })) {
                    if speech.source == .online {
                        ForEach(EdgeSpeech.voices, id: \.0) { id, name in Text(name).tag(id) }
                    } else {
                        ForEach(speech.systemVoices, id: \.identifier) { voice in Text("\(voice.name) · \(voice.language)").tag(voice.identifier) }
                    }
                }.disabled(speech.source == .system && speech.systemVoicesLoading)
                if speech.source == .system && speech.systemVoicesLoading { ProgressView(L("tts.preparing_voice")) }
            } footer: {
                Text(speech.source == .online ? L("tts.online_voice_requires_network") : L("tts.system_voice_uses_device_voices"))
            }
            Section {
                VStack(spacing: 8) {
                    LabeledContent(L("tts.rate"), value: String(format: "%.2f×", locale: ReaderI18n.shared.locale, speed))
                    Slider(value: $speed, in: 0.5...3, step: 0.05, onEditingChanged: { editing in if !editing { speech.setRate(speed) } })
                        .accessibilityLabel(L("tts.rate"))
                }.padding(.vertical, 4)
                VStack(spacing: 8) {
                    LabeledContent(L("tts.volume"), value: "\(Int(speech.volume * 100))%")
                    Slider(value: Binding(get: { speech.volume }, set: { speech.setVolume($0) }), in: 0...1, step: 0.05)
                        .accessibilityLabel(L("tts.volume"))
                }.padding(.vertical, 4)
            }
        }
        .navigationTitle(L("toolbar.tts")).navigationBarTitleDisplayMode(.inline)
        .onAppear { speed = speech.rate }
        .task(id: current.chapter) { speech.prepareText(at: current) }
        .task(id: speech.source) { if speech.source == .system { await speech.prepareSystemVoices() } }
        .onChange(of: speech.rate) { _, value in speed = value }
    }
}

struct ReadingSpeechCapsule: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var speech: ReadingSpeech
    let current: ReadingAnchor
    let open: () -> Void
    var body: some View {
        HStack(spacing: 0) {
            Button { speech.toggle(at: current) } label: {
                Image(systemName: speech.actionSymbol).font(.system(size: 20, weight: .medium)).frame(width: 44, height: 44)
            }.accessibilityLabel(speech.actionLabel)
            Button(action: open) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(speech.chapterTitle).font(.footnote).fontWeight(.medium).lineLimit(1)
                    Text(speech.statusLabel).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }.frame(maxWidth: .infinity, alignment: .leading).frame(height: 44)
            }.accessibilityLabel(L("toolbar.tts"))
            if !speech.following {
                Button { speech.follow() } label: { Image(systemName: "location.fill").frame(width: 44, height: 44) }
                    .accessibilityLabel(L("tts.return_to_position"))
            }
        }.buttonStyle(.plain).readerGlass(in: Capsule(), interactive: true)
    }
}

extension ReadingSpeech {
    var statusLabel: String {
        switch state {
        case .idle: return L("tts.read_from_here_label")
        case .preparing: return L("tts.preparing_voice")
        case .playing: return L("tts.reading_aloud")
        case .paused: return L("tts.paused")
        case .finished: return L("tts.reading_completed")
        case .failed: return L("tts.unavailable")
        }
    }
    var actionLabel: String {
        switch state {
        case .playing, .preparing: return L("tts.pause_reading")
        case .paused: return L("tts.resume_reading")
        case .failed: return L("csc.retry")
        default: return L("tts.start_reading")
        }
    }
    var actionSymbol: String { state == .playing || state == .preparing ? "pause.fill" : state == .failed ? "arrow.clockwise" : "play.fill" }
}
