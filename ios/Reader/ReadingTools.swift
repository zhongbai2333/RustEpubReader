import SwiftUI
import UIKit
import CoreText
import UniformTypeIdentifiers
struct ReadingToolsMenu: UIViewRepresentable {
    var tint: UIColor
    var contents: () -> Void
    var settings: () -> Void
    var speech: () -> Void
    var annotations: () -> Void
    var correction: () -> Void
    var search: () -> Void
    var statistics: () -> Void
    var night: Bool
    var dayNight: () -> Void
    var visibility: (Bool) -> Void
    typealias Coordinator = ReaderQuickMenuCoordinator
    func makeCoordinator() -> ReaderQuickMenuCoordinator { ReaderQuickMenuCoordinator(self) }
    func makeUIView(context: Context) -> ReaderMenuButton {
        let button = ReaderMenuButton(type: .system)
        configureReaderGlassButton(button, nativeGlass: false)
        updateReaderGlassButton(button, symbol: "ellipsis", size: 18, tint: tint)
        button.accessibilityLabel = L("reader.menu")
        button.showsMenuAsPrimaryAction = true
        button.preferredMenuElementOrder = .fixed
        button.visibility = { [weak coordinator = context.coordinator] in coordinator?.parent.visibility($0) }
        button.menu = context.coordinator.menu(for: button)
        return button
    }
    func updateUIView(_ button: ReaderMenuButton, context: Context) {
        context.coordinator.parent = self
        updateReaderGlassButton(button, symbol: "ellipsis", size: 18, tint: tint)
        if !button.menuVisible { button.menu = context.coordinator.menu(for: button) }
    }
    static func dismantleUIView(_ button: ReaderMenuButton, coordinator: ReaderQuickMenuCoordinator) {
        button.cancelMenu()
    }
}
final class ReaderMenuButton: UIButton {
    var visibility: ((Bool) -> Void)?
    private(set) var menuVisible = false
    func cancelMenu() {
        contextMenuInteraction?.dismissMenu()
        menuVisible = false; visibility?(false)
    }
    override func contextMenuInteraction(_ interaction: UIContextMenuInteraction,
        willDisplayMenuFor configuration: UIContextMenuConfiguration, animator: UIContextMenuInteractionAnimating?) {
        super.contextMenuInteraction(interaction, willDisplayMenuFor: configuration, animator: animator)
        menuVisible = true; visibility?(true)
    }
    override func contextMenuInteraction(_ interaction: UIContextMenuInteraction,
        willEndFor configuration: UIContextMenuConfiguration, animator: UIContextMenuInteractionAnimating?) {
        super.contextMenuInteraction(interaction, willEndFor: configuration, animator: animator)
        let finish = { [weak self] in
            guard let self else { return }
            self.menuVisible = false; self.visibility?(false)
        }
        if let animator { animator.addCompletion(finish) } else { finish() }
    }
}

// Native controls remain above the embedded UIKit reader in the touch hierarchy.
struct ReaderChromeButton: UIViewRepresentable {
    var symbol: String
    var label: String
    var tint: UIColor
    var enabled = true
    var nativeGlass = true
    var action: ()->Void
    final class Coordinator: NSObject {
        var action: ()->Void
        init(_ action:@escaping ()->Void) {self.action=action}
        @objc func activate() {action()}
    }
    func makeCoordinator()->Coordinator {Coordinator(action)}
    func makeUIView(context:Context)->ReaderChromeControl {
        let button=ReaderChromeControl(type:.system)
        configureReaderGlassButton(button, nativeGlass: nativeGlass)
        button.addTarget(context.coordinator,action:#selector(Coordinator.activate),for:.touchUpInside)
        return button
    }
    func updateUIView(_ button:ReaderChromeControl,context:Context) {
        context.coordinator.action=action
        updateReaderGlassButton(button, symbol: symbol, size: 17, tint: tint)
        button.accessibilityLabel=label;button.tintColor=tint;button.isEnabled=enabled
    }
}
final class ReaderChromeControl: UIButton {}

struct ReaderHighlight: Decodable, Identifiable {
    let id: String; let chapter: Int; let start_block: Int; let start_offset: Int; let end_block: Int; let end_offset: Int; let color: String
    var start: ReadingAnchor { .init(chapter: chapter, block: start_block, charOffset: start_offset) }
    var end: ReadingAnchor { .init(chapter: chapter, block: end_block, charOffset: end_offset) }
    var tint: UIColor { switch color { case "Green": return .systemGreen; case "Blue": return .systemBlue; case "Pink": return .systemPink; default: return .systemYellow } }
}
struct ReaderNote: Decodable { let highlight_id: String; let content: String }
struct ReaderBookmark: Decodable, Identifiable { let chapter: Int; let block: Int; var id: String { "\(chapter)-\(block)" } }
struct ReaderCorrection: Decodable { let chapter: Int; let block_idx: Int; let char_offset: Int; let original: String; let corrected: String; let status: String }
struct ReaderAnnotations: Decodable {
    var highlights: [ReaderHighlight]; var notes: [ReaderNote]; var bookmarks: [ReaderBookmark]; var corrections: [ReaderCorrection]?
    static var empty: Self { .init(highlights: [], notes: [], bookmarks: [], corrections: []) }
}
struct SelectedPassage: Identifiable { let id = UUID(); let start: ReadingAnchor; let end: ReadingAnchor; let text: String }
struct NativeDictionary: UIViewControllerRepresentable {
    let term: String
    func makeUIViewController(context: Context) -> UIReferenceLibraryViewController { UIReferenceLibraryViewController(term: term) }
    func updateUIViewController(_ controller: UIReferenceLibraryViewController, context: Context) {}
}
@MainActor final class ReadingAnnotations: ObservableObject {
    @Published var value = ReaderAnnotations.empty
    @Published var revision = 0
    @Published var busy = false
    let library: PreviewLibrary
    let book: PreviewBook
    init(library: PreviewLibrary, book: PreviewBook) { self.library = library; self.book = book }
    @discardableResult func perform(_ fields: [String: Any] = [:]) async -> Bool {
        guard !busy else { return false }; busy = true; defer { busy = false }
        do { value = try await library.store.annotations(id: book.id, fields: fields); revision += 1; return true }
        catch { library.fail(error, title: L("common.error_operation_failed")); return false }
    }
    @discardableResult func highlight(_ selected: SelectedPassage, color: String, note: String = "") async -> Bool {
        return await perform(["action": "highlight", "chapter": selected.start.chapter, "start_block": selected.start.block, "start_offset": selected.start.charOffset, "end_block": selected.end.block, "end_offset": selected.end.charOffset, "color": color, "note": note])
    }
    func excerpt(_ highlight: ReaderHighlight) -> String {
        guard book.chapters.indices.contains(highlight.chapter) else { return "" }
        return ((try? book.chapter(at:highlight.chapter))?.blocks ?? []).filter { $0.type == "Paragraph" || $0.type == "Heading" }.filter { ($0.core_block ?? 0) >= highlight.start_block && ($0.core_block ?? 0) <= highlight.end_block }.map { block in
            let text = (block.spans ?? []).map(\.text).joined(); let scalars = Array(text.unicodeScalars)
            let begin = (block.core_block ?? 0) == highlight.start_block ? highlight.start_offset : 0
            let end = (block.core_block ?? 0) == highlight.end_block ? highlight.end_offset : scalars.count
            return String(String.UnicodeScalarView(scalars.dropFirst(max(0,begin)).prefix(max(0,end-begin))))
        }.joined(separator: "\n")
    }
}
struct AnnotationsView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var annotations: ReadingAnnotations
    let current: ReadingAnchor
    let jump: (ReadingAnchor) -> Void
    var body: some View {
        List {
            Section(L("annotations.bookmarks")) {
                Button(L("context.bookmark"), systemImage: "bookmark") { Task { await annotations.perform(["action":"bookmark", "chapter":current.chapter, "block":current.block]) } }.disabled(annotations.busy)
                ForEach(annotations.value.bookmarks) { bookmark in
                    Button(L("annotations.chapter", bookmark.chapter+1)) { jump(.init(chapter: bookmark.chapter, block: bookmark.block)) }
                        .swipeActions { Button(L("library.delete"), role: .destructive) { Task { await annotations.perform(["action":"bookmark", "chapter":bookmark.chapter,"block":bookmark.block]) } } }
                }
            }
            Section(L("reader.highlights_and_notes")) {
                if annotations.value.highlights.isEmpty { Text(L("reader.long_press_hint")).foregroundStyle(.secondary) }
                ForEach(annotations.value.highlights) { item in
                    Button { jump(item.start) } label: {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(annotations.excerpt(item)).lineLimit(3).foregroundStyle(.primary)
                            if let note = annotations.value.notes.first(where: { $0.highlight_id == item.id }) { Text(note.content).font(.subheadline).foregroundStyle(.secondary) }
                            Text(L("annotations.chapter", item.chapter+1)).font(.caption).foregroundStyle(.secondary)
                        }.padding(.leading, 8).overlay(alignment: .leading) { Rectangle().fill(Color(uiColor: item.tint)).frame(width: 3) }
                    }.swipeActions { Button(L("reader.delete_highlight"), role: .destructive) { Task { await annotations.perform(["action":"delete", "highlight_id":item.id]) } } }
                }
            }
        }.navigationTitle(L("annotations.title")).navigationBarTitleDisplayMode(.inline)
    }
}
struct SelectedTextView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    let passage: SelectedPassage
    @ObservedObject var annotations: ReadingAnnotations
    @Environment(\.dismiss) private var dismiss
    @State private var note = ""
    @State private var dictionary = false
    var body: some View {
        NavigationStack {
            Form {
                Section { Text(passage.text).textSelection(.enabled) }
                Section {
                    Button(L("context.copy"), systemImage: "doc.on.doc") { UIPasteboard.general.string = passage.text; dismiss() }
                    Button(L("reader.look_up"), systemImage: "character.book.closed") { dictionary = true }
                }
                Section(L("selection.highlight")) {
                    HStack {
                        ForEach([("Yellow",L("common.color_yellow")),("Green",L("common.color_green")),("Blue",L("common.color_blue")),("Pink",L("common.color_pink"))], id: \.0) { item in
                            Button { Task { if await annotations.highlight(passage, color: item.0) { dismiss() } } } label: {
                                VStack(spacing: 6) {
                                    Circle().fill(highlightColor(item.0)).frame(width: 22, height: 22)
                                    Text(item.1).font(.caption).foregroundStyle(.primary)
                                }.frame(maxWidth: .infinity, minHeight: 48)
                            }.buttonStyle(.plain)
                        }
                    }.disabled(annotations.busy)
                }
                Section(L("annotations.note")) {
                    TextEditor(text: $note).frame(minHeight: 100)
                    Button(L("reader.save_note")) { Task { if await annotations.highlight(passage, color: "Yellow", note: note) { dismiss() } } }.disabled(note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || annotations.busy)
                }
            }.navigationTitle(L("reader.selected_text")).navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button(L("common.deselect")) { dismiss() } } }
                .sheet(isPresented: $dictionary) { NativeDictionary(term: passage.text) }
        }
    }
    private func highlightColor(_ color: String) -> Color {
        switch color { case "Green": return .green; case "Blue": return .blue; case "Pink": return .pink; default: return .yellow }
    }
}
struct CorrectionView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var model: ChineseCorrection
    @ObservedObject var annotations: ReadingAnnotations
    let chapter: Int
    var body: some View {
        Form {
            Section {
                LabeledContent("MacBERT", value: model.ready ? L("csc.model_ready") : model.available ? L("common.not_loaded") : L("common.not_downloaded"))
                if model.busy { HStack { ProgressView(); Text(L("common.processing")) }; Button(L("share.cancel")) { model.cancel() } }
                else if !model.ready { Button(model.available ? L("csc.load_model") : L("csc.download_model")) { model.prepare(download: !model.available) } }
                else { Button(L("csc.check_current_chapter")) { if let content=try? annotations.book.chapter(at:chapter) {model.check(chapter:content)} } }
                if model.available && !model.busy { Button(L("csc.delete_model"), role: .destructive) { model.remove() } }
                Picker(L("csc.correction_intensity"), selection: $model.threshold) { Text(L("csc.conservative")).tag(0.97); Text(L("csc.standard")).tag(0.90); Text(L("csc.intensity_aggressive")).tag(0.80) }.disabled(model.busy)
                if let error = model.error { Text(LM(error)).foregroundStyle(.red) }
            } footer: { Text(L("csc.first_use_hint")) }
            Section(L("csc.correction_suggestions")) {
                ForEach(model.suggestions) { suggestion in
                    VStack(alignment: .leading, spacing: 8) {
                        Text("\(suggestion.original) → \(suggestion.corrected)").font(.headline)
                        Text(String(format: "%.0f%%", locale: ReaderI18n.shared.locale, suggestion.confidence*100)).font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button(L("csc.accept")) { Task { if await annotations.perform(["action":"correction","chapter":chapter,"block":suggestion.block,"char_offset":suggestion.offset,"original":suggestion.original,"corrected":suggestion.corrected]) { model.suggestions.removeAll { $0.id == suggestion.id } } } }.disabled(annotations.busy)
                            Button(L("csc.ignore")) { model.suggestions.removeAll { $0.id == suggestion.id } }
                        }
                    }
                }
            }
        }.navigationTitle(L("settings.csc_settings")).navigationBarTitleDisplayMode(.inline)
    }
}

struct ReaderAppearanceSettings: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @Binding var fontSize: Double
    @Binding var lineHeight: Double
    @Binding var fontName: String
    @Binding var theme: String
    @Binding var readingMode: String
    @Binding var backgroundHex: String
    @Binding var textHex: String
    @Binding var paragraphSpacing: Double
    @Binding var pageMargin: Double
    @Binding var firstLineIndent: Double
    @Binding var justified: Bool
    @State private var confirmReset = false
    @State private var choosingLanguage = false
    var pageAnimation: Binding<String> = .constant("curl")
    private var themes: [(String,String)] { [("paper", L("settings.paper_color")), ("light", L("common.theme_light")), ("warm", L("common.theme_warm")), ("gray", L("common.theme_gray")), ("green", L("common.theme_soft_green")), ("dark", L("common.theme_dark"))] }
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var latinFontName: Binding<String> = .constant("Helvetica")
    var cssPolicy: Binding<String> = .constant("mixed")
    var scriptsEnabled: Binding<Bool> = .constant(true)
    var showImmersiveStatus: Binding<Bool> = .constant(false)
    var backgroundImageName: Binding<String> = .constant("")
    var backgroundImageEnabled: Binding<Bool> = .constant(true)
    var backgroundImageOpacity: Binding<Double> = .constant(0.22)
    var appearance: Binding<ReadingAppearance> = .constant(.init())
    var appearanceDark = false
    var body: some View {
        Form {
            ReadingAppearanceControls(value: appearance, activeDark: appearanceDark, fontName: fontName, fontSize: fontSize)

            Section {
                Text(L("settings.reading_mode_label")).font(.subheadline).foregroundStyle(.secondary)
                Picker(L("settings.reading_mode_label"), selection: $readingMode) {
                    Text(L("toolbar.scroll_mode")).tag("scroll"); Text(L("toolbar.page_mode")).tag("paging")
                }.pickerStyle(.segmented)
                Text(L("settings.page_turn_animation")).font(.subheadline).foregroundStyle(.secondary)
                Picker(L("settings.page_turn_animation"), selection: pageAnimation) {
                    Text(L("settings.realistic")).tag("curl"); Text(L("settings.slide")).tag("slide"); Text(L("settings.cover")).tag("cover"); Text(L("settings.none")).tag("none")
                }.pickerStyle(.segmented)
            } header: { Text(L("settings.page_turn_method")) } footer: {
                Text(L("settings.reading_mode_desc"))
                if reduceMotion && pageAnimation.wrappedValue == "curl" {
                    Text(L("settings.reduce_motion_hint"))
                }
            }
            Section(L("settings.font_group")) {
                HStack {
                    Button { fontSize = max(14, fontSize - 1) } label: { Text("A").font(.system(size: 18)).frame(width: 44, height: 44) }
                        .disabled(fontSize <= 14).accessibilityLabel(L("settings.font_smaller"))
                    Spacer()
                    Text("\(Int(fontSize)) pt").font(.title3).monospacedDigit().accessibilityLabel(L("settings.font_size_label")).accessibilityValue("\(Int(fontSize))")
                    Spacer()
                    Button { fontSize = min(28, fontSize + 1) } label: { Text("A").font(.system(size: 28)).frame(width: 44, height: 44) }
                        .disabled(fontSize >= 28).accessibilityLabel(L("settings.font_larger"))
                }.buttonStyle(.plain)
                NavigationLink {
                    ReaderFontPicker(selection: latinFontName, kind: .latin)
                } label: {
                    HStack { Text(L("settings.english_font")); Spacer(); Text(ReaderFontPicker.label(UIFont(name: latinFontName.wrappedValue, size: 18)?.familyName ?? latinFontName.wrappedValue)).foregroundStyle(.secondary).lineLimit(1) }
                }
                NavigationLink {
                    ReaderFontPicker(selection: $fontName, kind: .chinese)
                } label: {
                    HStack { Text(L("settings.chinese_font")); Spacer(); Text(ReaderFontPicker.label(UIFont(name: fontName, size: 18)?.familyName ?? fontName)).foregroundStyle(.secondary).lineLimit(1) }
                }
            }
            .disabled(cssPolicy.wrappedValue == "publisher")
            Section {
                Picker(L("settings.layout_style"), selection: cssPolicy) {
                    Text(L("settings.layout_native")).tag("none");Text(L("settings.layout_mixed")).tag("mixed");Text(L("settings.layout_classic")).tag("publisher")
                }.pickerStyle(.segmented)
            } header: {Text(L("settings.layout_style"))} footer: {
                Text(cssPolicy.wrappedValue == "none" ? L("settings.layout_native_desc") : cssPolicy.wrappedValue == "publisher" ? L("settings.layout_classic_desc") : L("settings.layout_mixed_desc"))
            }

            Section(L("settings.typography")) {
                settingSlider(L("settings.line_height"), value: $lineHeight, range: 1.2...2.2, step: 0.05, label: String(format: L("settings.line_height_value"), locale: ReaderI18n.shared.locale, lineHeight))
                settingSlider(L("settings.para_spacing"), value: $paragraphSpacing, range: 0...1.5, step: 0.05, label: String(format: L("settings.line_height_value"), locale: ReaderI18n.shared.locale, paragraphSpacing))
                settingSlider(L("settings.margins"), value: $pageMargin, range: 16...48, step: 2, label: "\(Int(pageMargin)) pt").disabled(cssPolicy.wrappedValue != "none")
                settingSlider(L("settings.text_indent"), value: $firstLineIndent, range: 0...2, step: 1, label: L("settings.margin_value", Int(firstLineIndent))).disabled(cssPolicy.wrappedValue != "none")
                Toggle(L("settings.justify_text"), isOn: $justified).disabled(cssPolicy.wrappedValue != "none")
            }
            .disabled(cssPolicy.wrappedValue == "publisher")
            Section {
                NavigationLink(L("settings.more")) {
                    Form {
                        Section {
                            Button {choosingLanguage=true} label: {
                                HStack {Text(L("language.title"));Spacer();Text(readerI18n.choiceLabel).foregroundStyle(.secondary);Image(systemName:"chevron.right").font(.caption).foregroundStyle(.tertiary)}
                            }.foregroundStyle(.primary)
                        }
                        Section {
                            Toggle(L("settings.immersive_status_info"), isOn: showImmersiveStatus)
                        } footer: {Text(L("settings.immersive_status_info_desc"))}
                        Section {
                            Toggle(L("settings.book_scripts"), isOn: scriptsEnabled)
                        } footer: {Text(L("settings.book_scripts_desc"))}
                        Section {
                            Button(L("settings.reset_display")) {confirmReset = true}
                        } footer: {Text(L("settings.reset_display_scope"))}
                    }.navigationTitle(L("settings.more")).navigationBarTitleDisplayMode(.inline)
                        .alert(L("settings.reset_title"),isPresented:$confirmReset) {
                            Button(L("share.cancel"),role:.cancel) {}
                            Button(L("settings.reset_confirm")) {resetDisplay()}
                        } message: {
                            Text(L("settings.reset_message"))
                        }
                }
            }
        }.navigationTitle(L("settings.appearance_title")).navigationBarTitleDisplayMode(.inline)
            .tint(Color(uiColor: .systemBlue))
            .sheet(isPresented:$choosingLanguage) {ReaderLanguagePicker()}
    }
    private func resetDisplay() {
        appearance.wrappedValue = .init()
        fontSize = 18; lineHeight = 1.65; fontName = "PingFangSC-Regular"; latinFontName.wrappedValue = "Helvetica"; theme = "paper"; readingMode = "paging"; pageAnimation.wrappedValue = "curl"
        backgroundHex = "F7F2E8"; textHex = "252525"; paragraphSpacing = 0.65; pageMargin = 24; firstLineIndent = 2; justified = true; cssPolicy.wrappedValue = "mixed"; backgroundImageEnabled.wrappedValue = false; backgroundImageOpacity.wrappedValue = 0.22
    }
    private func colorBinding(_ hex: Binding<String>) -> Binding<Color> {
        Binding(get: { Color(uiColor: UIColor(hex: hex.wrappedValue)) }, set: { hex.wrappedValue = UIColor($0).readerHex })
    }
    private func settingSlider(_ title: String, value: Binding<Double>, range: ClosedRange<Double>, step: Double, label: String) -> some View {
        VStack(spacing: 4) {
            LabeledContent(title, value: label).font(.subheadline)
            Slider(value: value, in: range, step: step).accessibilityLabel(title)
        }
    }
}

struct ReaderFontPicker: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject private var fonts = ReaderFonts.shared
    enum Kind { case latin, chinese
        var title: String { self == .latin ? L("settings.english_font") : L("settings.chinese_font") }
        var sample: String { self == .latin ? "The quick brown fox · 0123456789" : "春江潮水连海平，海上明月共潮生。" }
    }
    @Binding var selection: String
    var kind: Kind = .chinese
    @State private var search = ""
    @State private var importing = false
    @State private var systemPicker = false
    // Some CJK families can be instantiated but are omitted from familyNames.
    private static let cjkFonts = ["PingFangSC-Regular", "PingFangTC-Regular", "STSongti-SC-Regular", "STSongti-TC-Regular", "SongtiSC-Regular", "SongtiTC-Regular", "STKaitiSC-Regular", "KaitiSC-Regular", "STSong", "STHeitiSC-Light", "STHeitiTC-Light", "HiraginoSansGB-W3"]
        .compactMap { UIFont(name: $0, size: 18) }
    static func label(_ family: String) -> String {
        if family.isEmpty { return L("fonts.follow_chinese") }
        if let font = DownloadableFont.catalog.first(where: { UIFont(name: $0.postscript, size: 18)?.familyName == family }) { return font.title }
        switch family {
        case "PingFang SC": return L("settings.font_pingfang_sc")
        case "PingFang TC": return L("settings.font_pingfang_tc")
        case "PingFang HK": return L("settings.font_pingfang_hk")
        case "PingFang MO": return L("settings.font_pingfang_mo")
        case "Songti SC", "STSong": return L("settings.font_songti_sc")
        case "Songti TC": return L("settings.font_songti_tc")
        case "Kaiti SC": return L("settings.font_kaiti_sc")
        case "Kaiti TC": return L("settings.font_kaiti_tc")
        default: return family
        }
    }
    static func supportsChinese(_ font: UIFont) -> Bool { supports("汉语阅读书籍", font: font) || supports("\u{6F22}\u{8A9E}\u{95B1}\u{8B80}\u{66F8}\u{7C4D}", font: font) }
    static func supportsLatin(_ font: UIFont) -> Bool { supports("Aa0123", font: font) }
    private static func supports(_ text: String, font: UIFont) -> Bool {
        let units = Array(text.utf16)
        var glyphs = [CGGlyph](repeating: 0, count: units.count)
        let ct = CTFontCreateWithName(font.fontName as CFString, 18, nil)
        return CTFontGetGlyphsForCharacters(ct, units, &glyphs, units.count) && glyphs.allSatisfy { $0 != 0 }
    }
    static func representative(_ family: String) -> UIFont? {
        let names = UIFont.fontNames(forFamilyName: family) + ReaderFonts.shared.names.filter { UIFont(name: $0, size: 18)?.familyName == family }
        let name = cjkFonts.first(where: { $0.familyName == family })?.fontName
            ?? names.first(where: { $0.localizedCaseInsensitiveContains("regular") }) ?? names.first ?? family
        return UIFont(name: name, size: 18)
    }
    static func migrateLegacySettings(_ defaults: UserDefaults = .standard) {
        guard defaults.object(forKey: "preview.latinFontName") == nil else { return }
        let old = defaults.string(forKey: "preview.fontName") ?? "PingFangSC-Regular"
        if let font = UIFont(name: old, size: 18), !supportsChinese(font) {
            defaults.set(old, forKey: "preview.latinFontName")
            defaults.set("PingFangSC-Regular", forKey: "preview.fontName")
        } else { defaults.set("Helvetica", forKey: "preview.latinFontName") }
    }
    static func families(for kind: Kind) -> [String] {
        Set(UIFont.familyNames + Self.cjkFonts.map(\.familyName) + ReaderFonts.shared.names.compactMap { UIFont(name: $0, size: 18)?.familyName }).sorted()
            .filter { !["Apple Color Emoji", "Apple Symbols", "Zapf Dingbats", "Symbol", "Webdings", "Wingdings"].contains($0) }
            .filter { family in guard let font = Self.representative(family) else { return false }; return kind == .chinese ? Self.supportsChinese(font) : Self.supportsLatin(font) }
    }
    private var families: [String] {
        Self.families(for: kind).filter { search.isEmpty || $0.localizedCaseInsensitiveContains(search) || Self.label($0).localizedCaseInsensitiveContains(search) }
    }
    var body: some View {
        List {
            if kind == .latin {
                Section {
                    Button { selection = "" } label: {
                        HStack {
                            Text(L("fonts.follow_chinese"))
                            Spacer()
                            if selection.isEmpty { Image(systemName: "checkmark").foregroundStyle(.tint) }
                        }.foregroundStyle(.primary)
                    }.accessibilityAddTraits(selection.isEmpty ? [.isSelected] : [])
                } footer: { Text(L("fonts.follow_chinese_desc")) }
            }
            Section {
                Button(L("fonts.import")) { importing = true }.disabled(fonts.busy.contains("import"))
                Button(L("fonts.system")) { systemPicker = true }
                NavigationLink(L("fonts.library")) { ReaderFontDownloads(selection: $selection, kind: kind) }
                if fonts.busy.contains("import") { HStack { ProgressView(); Text(L("fonts.importing")) } }
            }
            Section(L("fonts.available")) {
            ForEach(families, id: \.self) { family in
            let name = Self.representative(family)?.fontName ?? family
            Button { selection = name } label: {
                HStack {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(Self.label(family)).font(.subheadline)
                        Text(kind.sample).font(Font.custom(name, size: 19)).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 8)
                    if UIFont(name: selection, size: 18)?.familyName == family { Image(systemName: "checkmark").foregroundStyle(.tint) }
                }.padding(.vertical, 4).foregroundStyle(.primary)
            }.buttonStyle(.plain).accessibilityAddTraits(UIFont(name: selection, size: 18)?.familyName == family ? [.isSelected] : [])
            }
            }
        }.searchable(text: $search, prompt: L("settings.search_fonts"))
            .modifier(ReaderSearchNavigation()).navigationTitle(kind.title).navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(Color(uiColor: .systemGroupedBackground), for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
            .fileImporter(isPresented: $importing, allowedContentTypes: [UTType(filenameExtension: "ttf") ?? .font, UTType(filenameExtension: "otf") ?? .font], allowsMultipleSelection: true) { result in
                switch result {
                case .success(let urls): Task { await fonts.importFiles(urls) }
                case .failure(let error): fonts.error = error.localizedDescription
                }
            }
            .sheet(isPresented: $systemPicker) {
                SystemReaderFontPicker(onSelect: { font in
                    fonts.rememberSystemFont(font)
                    if kind == .chinese ? Self.supportsChinese(font) : Self.supportsLatin(font) { selection = font.fontName }
                    else { fonts.error = L("fonts.unsupported") }
                }, onDismiss: { systemPicker = false })
            }
            .alert(L("fonts.error"), isPresented: Binding(get: { fonts.error != nil && !systemPicker }, set: { if !$0 { fonts.error = nil } })) { Button(L("error.ok")) { fonts.error = nil } } message: { Text(fonts.error ?? "") }
    }
}

private struct ReaderSearchNavigation: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 17.1, *) {
            content.searchPresentationToolbarBehavior(.avoidHidingContent)
        } else {
            content
        }
    }
}


struct ReadingStatusInfo: Equatable {
    let time: String
    let battery: Int?
    var text: String { time }
    var batteryImage: UIImage? {
        // A dash distinguishes unavailable readings from an empty battery.
        return UIGraphicsImageRenderer(size: CGSize(width: 18, height: 9)).image { _ in
            UIColor.label.setStroke(); UIColor.label.setFill()
            let outline = UIBezierPath(roundedRect: CGRect(x: 0.6, y: 0.6, width: 14.8, height: 7.8), cornerRadius: 1.5)
            outline.lineWidth = 1.2; outline.stroke()
            UIBezierPath(roundedRect: CGRect(x: 16, y: 2.5, width: 2, height: 4), cornerRadius: 0.7).fill()
            if let battery {
                let width = 12 * CGFloat(min(100, max(0, battery))) / 100
                if width > 0 { UIBezierPath(roundedRect: CGRect(x: 2, y: 2, width: width, height: 5), cornerRadius: 0.6).fill() }
            } else {
                let dash = UIBezierPath(); dash.move(to: CGPoint(x: 5, y: 4.5)); dash.addLine(to: CGPoint(x: 11, y: 4.5)); dash.lineWidth = 1; dash.stroke()
            }
        }.withRenderingMode(.alwaysTemplate)
    }
    @MainActor private static var monitors = 0
    @MainActor private static var previouslyMonitoring = false
    @MainActor static func beginMonitoring() {
        if monitors == 0 { previouslyMonitoring = UIDevice.current.isBatteryMonitoringEnabled; UIDevice.current.isBatteryMonitoringEnabled = true }
        monitors += 1
    }
    @MainActor static func endMonitoring() {
        monitors = max(0, monitors-1)
        if monitors == 0 { UIDevice.current.isBatteryMonitoringEnabled = previouslyMonitoring }
    }
    @MainActor static func current() -> Self {
        let formatter = DateFormatter(); formatter.dateFormat = "HH:mm"
        let level = UIDevice.current.batteryLevel
        return .init(time: formatter.string(from: Date()), battery: level < 0 ? nil : min(100, max(0, Int((level*100).rounded()))))
    }
}
struct ReadingImmersiveStatus: View {
    let info: ReadingStatusInfo
    let ink: UIColor
    var body: some View {
        HStack(spacing: 5) {
            Text(info.text).monospacedDigit()
            if let image = info.batteryImage { Image(uiImage: image).renderingMode(.template).resizable().scaledToFit().frame(width: 18, height: 9) }
        }.font(.system(size: 11)).foregroundStyle(Color(uiColor: ink.withAlphaComponent(0.58)))
            .allowsHitTesting(false)
    }
}
final class PaperImmersiveStatus: UIStackView {
    private let label = UILabel(), battery = UIImageView()
    override init(frame: CGRect) {
        super.init(frame: frame)
        axis = .horizontal; alignment = .center; spacing = 5
        label.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        battery.contentMode = .scaleAspectFit
        addArrangedSubview(label); addArrangedSubview(battery)
        battery.widthAnchor.constraint(equalToConstant: 18).isActive = true
        battery.heightAnchor.constraint(equalToConstant: 9).isActive = true
        isUserInteractionEnabled = false; accessibilityIdentifier = "reading-immersive-status"
    }
    required init(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    func update(_ info: ReadingStatusInfo?, ink: UIColor) {
        isHidden = info == nil
        label.text = info?.text; label.textColor = ink.withAlphaComponent(0.58)
        battery.image = info?.batteryImage
        battery.isHidden = info == nil; battery.tintColor = label.textColor
        accessibilityLabel = info?.text
    }
}


struct ReadingIntroductionView: View {
    @EnvironmentObject private var appearance: AppearanceCoordinator
    @Environment(\.dynamicTypeSize) private var typeSize
    let dismiss: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                Text(L("reader.intro_title")).font(.title3.weight(.semibold)).accessibilityAddTraits(.isHeader)
                Spacer()
                Button(action: dismiss) { Image(systemName: "xmark").font(.system(size: 17)).frame(width: 44, height: 44) }
                    .readerGlass(in: Circle()).accessibilityLabel(L("settings.close"))
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    row("hand.draw", "reader.intro_turns_title", "reader.intro_turns_mobile")
                    row("ellipsis.circle", "reader.intro_menu_title", "reader.intro_menu_mobile")
                    row("battery.100percent", "reader.intro_status_title", "reader.intro_status_body")
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.scrollBounceBehavior(.basedOnSize)
            Button(action: dismiss) { Text(L("reader.intro_start")).font(.headline).frame(maxWidth: .infinity, minHeight: 36) }
                .buttonStyle(.borderedProminent).controlSize(.large)
        }.padding(24)
            .readerPanelSurface(appearance: appearance)
            .presentationDetents(typeSize.isAccessibilitySize ? [.large] : [.height(430), .large])
            .presentationDragIndicator(.visible)
    }
    private func row(_ symbol: String, _ title: String, _ body: String) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: symbol).font(.system(size: 22)).foregroundStyle(Color.accentColor)
                .frame(width: 26, height: 26).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 6) {
                Text(L(title)).font(.subheadline.weight(.semibold))
                Text(L(body)).font(.subheadline).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }.accessibilityElement(children: .combine)
    }
}
