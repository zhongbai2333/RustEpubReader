import SwiftUI
import PhotosUI
import UniformTypeIdentifiers

struct ReadingAppearanceControls: View {
    @Binding var value: ReadingAppearance
    var activeDark: Bool
    var fontName: String
    var fontSize: Double
    @State private var editing: ReadingStyle?
    @ScaledMetric(relativeTo: .caption) private var minimumCardWidth = 86
    var body: some View {
        AppearanceLinkageControls()
        Section(L("appearance.styles")) {
            ReadingStyleSample(variant: value.style[dark: activeDark], fontName: fontName, fontSize: fontSize)
                .listRowInsets(EdgeInsets())
            LazyVGrid(columns: [GridItem(.adaptive(minimum: minimumCardWidth), spacing: 12)], spacing: 12) {
                ForEach(value.choices) { style in
                    let variant = style[dark: activeDark]
                    let creating = style.id == "custom" && value.custom == nil
                    Button {
                        if creating { editing = .customTemplate } else { value.selected = style.id }
                    } label: {
                        VStack(spacing: 6) {
                            Text(creating ? "+" : "Aa").font(.system(size: 24, design: creating ? .default : .serif))
                                .foregroundStyle(creating ? Color(uiColor: .systemBlue) : Color(uiColor: UIColor(hex: variant.ink)))
                                .frame(maxWidth: .infinity).frame(height: 54)
                                .background(creating ? Color(uiColor: .secondarySystemGroupedBackground) : Color(uiColor: UIColor(hex: variant.paper)), in: RoundedRectangle(cornerRadius: 10))
                                .overlay(alignment: .bottomTrailing) { if value.selected == style.id { Image(systemName: "checkmark.circle.fill").foregroundStyle(.white, Color(uiColor: .systemBlue)).padding(4) } }
                                .overlay { RoundedRectangle(cornerRadius: 10).strokeBorder(value.selected == style.id ? Color(uiColor: .systemBlue) : Color.primary.opacity(0.12), lineWidth: value.selected == style.id ? 2 : 1) }
                            Text(L(style.labelKey)).font(.caption).foregroundStyle(.primary).fixedSize(horizontal: false, vertical: true)
                        }
                    }.buttonStyle(.plain).accessibilityLabel(L(creating ? "appearance.create_custom" : style.labelKey)).accessibilityAddTraits(value.selected == style.id ? [.isSelected] : [])
                }
            }.padding(.vertical, 4)
            Button { editing = value.style } label: { HStack { Text(L("appearance.adjust")); Spacer(); Image(systemName: "chevron.right").font(.caption) } }
            .sheet(item: $editing) { target in
                ReadingStyleEditor(style: target, initiallyDark: activeDark, fontName: fontName, fontSize: fontSize) { draft in
                    let old = value.imageReferences
                    value.save(draft)
                    let retained = value.imageReferences.union([UserDefaults.standard.string(forKey: "preview.backgroundImageName") ?? ""])
                    for name in old.subtracting(retained) { ReaderBackgroundStore.remove(name) }
                }.presentationDetents([.large])
            }
        }
    }
}

struct ReadingStyleEditor: View {
    @EnvironmentObject private var appearanceLink: AppearanceCoordinator
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var draft: ReadingStyle
    @State private var editDark: Bool
    let fontName: String
    let fontSize: Double
    let save: (ReadingStyle) -> Void
    @State private var photo: PhotosPickerItem?
    @State private var filePicker = false
    @State private var importing = false
    @State private var failed = false
    @State private var copied = false
    @State private var token = UUID()
    @State private var added = Set<String>()
    @State private var retained = Set<String>()
    @State private var importTask: Task<Void, Never>?
    init(style: ReadingStyle, initiallyDark: Bool, fontName: String, fontSize: Double, save: @escaping (ReadingStyle) -> Void) {
        _draft = State(initialValue: style); _editDark = State(initialValue: initiallyDark)
        self.fontName = fontName; self.fontSize = fontSize; self.save = save
    }
    private var variant: ReadingAppearanceVariant { draft[dark: editDark] }
    private func color(_ ink: Bool) -> Binding<Color> {
        Binding(get: { Color(uiColor: UIColor(hex: ink ? variant.ink : variant.paper)) }, set: { color in
            if ink { draft[dark: editDark].ink = UIColor(color).readerHex } else { draft[dark: editDark].paper = UIColor(color).readerHex }
        })
    }
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    if typeSize.isAccessibilitySize { targetPicker.pickerStyle(.menu) }
                    else { targetPicker.pickerStyle(.segmented) }
                    ReadingStyleSample(variant: variant, fontName: fontName, fontSize: fontSize, editingDark: editDark)
                        .listRowInsets(EdgeInsets())
                }
                Section {
                    ColorPicker(L("settings.background_color"), selection: color(false), supportsOpacity: false)
                    ReadingColorSwatches(colors: ReadingColorPalette.paper, selected: variant.paper) { draft[dark: editDark].paper = $0 }
                    ColorPicker(L("settings.text_color"), selection: color(true), supportsOpacity: false)
                    ReadingColorSwatches(colors: ReadingColorPalette.ink, selected: variant.ink) { draft[dark: editDark].ink = $0 }
                    if readingContrast(variant) < 4.5 { Text(L("appearance.contrast")).font(.footnote).foregroundStyle(.orange) }
                }
                Section(L("common.background_image")) {
                    if let image = ReaderBackgroundStore.image(variant.image) {
                        HStack {
                            Image(uiImage: image).resizable().scaledToFill().frame(width: 64, height: 64).clipShape(RoundedRectangle(cornerRadius: 10))
                            Text(L("settings.image_selected")); Spacer()
                        }
                    } else { Text(L("settings.image_empty")).foregroundStyle(.secondary) }
                    PhotosPicker(selection: $photo, matching: .images) { Label(L("common.select_from_photos"), systemImage: "photo") }.disabled(importing)
                    Button { filePicker = true } label: { Label(L("common.select_from_files"), systemImage: "folder") }.disabled(importing)
                    if importing { ProgressView(L("common.importing_image")) }
                    if !variant.image.isEmpty {
                        VStack {
                            LabeledContent(L("appearance.strength"), value: "\(Int((variant.strength * 100).rounded()))%")
                            Slider(value: Binding(get: { variant.strength }, set: { draft[dark: editDark].strength = $0 }), in: 0...1, step: 0.01).accessibilityLabel(L("appearance.strength"))
                        }
                        Button(L(editDark ? "appearance.copy_light" : "appearance.copy_dark")) { draft[dark: !editDark].image = variant.image; copied = true }
                        Button(L("common.remove_image"), role: .destructive) { token = UUID(); draft[dark: editDark].image = ""; copied = false }
                    }
                    if copied { Text(L("appearance.copied")).font(.footnote).foregroundStyle(.secondary) }
                }
                Section {
                    Button(L(draft.id == "custom" ? "appearance.reset_custom" : "appearance.restore_preset")) {
                        token = UUID(); copied = false; failed = false
                        draft = ReadingStyle.presets.first { $0.id == draft.id } ?? .customTemplate
                    }.disabled(importing)
                } footer: { Text(L("appearance.reset_note")) }
                Section { Text(L(draft.id == "custom" ? "appearance.custom_save_note" : "appearance.preset_save_note")).font(.footnote).foregroundStyle(.secondary) }
            }
            .navigationTitle(draft.id == "custom" ? L("appearance.custom_title") : L("appearance.adjust_named", L(draft.labelKey))).navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(L("common.cancel")) { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button(L("common.save")) {
                    retained = Set([draft.light.image, draft.dark.image]); save(draft); dismiss()
                }.disabled(importing) }
            }
            .onChange(of: editDark) { _, _ in token = UUID(); copied = false }
            .onChange(of: photo) { _, item in
                guard let item else { return }; let request = UUID(); token = request; let side = editDark; importing = true
                importTask = Task {
                    defer { importing = false; photo = nil }
                    do {
                        guard let data = try await item.loadTransferable(type: Data.self), data.count <= 40 * 1024 * 1024 else { throw ReaderBackgroundStore.Failure.invalidImage }
                        let name = try await Task.detached(priority: .userInitiated) { try ReaderBackgroundStore.importImage(data) }.value
                        complete(name, request: request, side: side)
                    } catch { if !Task.isCancelled { failed = true } }
                }
            }
            .fileImporter(isPresented: $filePicker, allowedContentTypes: [.image]) { result in
                guard case .success(let url) = result else { return }
                let request = UUID(); token = request; let side = editDark; importing = true
                importTask = Task {
                    defer { importing = false }
                    do { let name = try await Task.detached(priority: .userInitiated) { try ReaderBackgroundStore.importFile(url) }.value; complete(name, request: request, side: side) }
                    catch { if !Task.isCancelled { failed = true } }
                }
            }
            .onDisappear {
                token = UUID(); importTask?.cancel()
                for name in added where !retained.contains(name) { ReaderBackgroundStore.remove(name) }
            }
            .alert(L("appearance.image_error"), isPresented: $failed) { Button(L("manage.done"), role: .cancel) {} }
        }
        .readerPanelSurface(appearance: appearanceLink)
        .tint(Color(uiColor: .systemBlue))
    }
    private var targetPicker: some View {
        Picker(L("appearance.edit"), selection: $editDark) {
            Text(L("appearance.edit_light")).tag(false); Text(L("appearance.edit_dark")).tag(true)
        }
    }
    private func complete(_ name: String, request: UUID, side: Bool) {
        guard !Task.isCancelled, request == token, side == editDark else { ReaderBackgroundStore.remove(name); return }
        added.insert(name); draft[dark: side].image = name; copied = false
    }
}

struct ReadingStyleSample: View {
    let variant: ReadingAppearanceVariant
    var fontName: String
    var fontSize: Double
    var editingDark: Bool? = nil
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let editingDark { Text(L("appearance.preview") + " · " + L(editingDark ? "appearance.mode_dark" : "appearance.mode_light")).font(.caption) }
            Text(L("appearance.sample"))
                .font(Font(UIFont(name: fontName, size: fontSize) ?? .systemFont(ofSize: fontSize)))
                .lineSpacing(fontSize * 0.45)
        }
            .foregroundStyle(Color(uiColor: UIColor(hex: variant.ink)))
            .padding(24).frame(maxWidth: .infinity, alignment: .leading)
            .background {
                GeometryReader { geometry in
                    ZStack {
                        Color(uiColor: UIColor(hex: variant.paper))
                        if let image = ReaderBackgroundStore.image(variant.image) {
                            Image(uiImage: image).resizable().scaledToFill().frame(width: geometry.size.width, height: geometry.size.height).clipped().opacity(variant.strength)
                        }
                    }
                }
            }.clipped().accessibilityElement(children: .ignore)
            .accessibilityLabel(L("appearance.preview") + (editingDark.map { " · " + L($0 ? "appearance.mode_dark" : "appearance.mode_light") } ?? "") + ": " + L("appearance.sample"))
    }
}

private func readingContrast(_ variant: ReadingAppearanceVariant) -> Double {
    func luminance(_ hex: String) -> Double {
        let rgb = UInt32(hex, radix: 16) ?? 0
        let c = [Double((rgb >> 16) & 255), Double((rgb >> 8) & 255), Double(rgb & 255)].map { value -> Double in
            let v = value / 255; return v <= 0.04045 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4)
        }
        return c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
    }
    let a = luminance(variant.paper), b = luminance(variant.ink)
    return (max(a,b) + 0.05) / (min(a,b) + 0.05)
}


private struct ReadingColorSwatches: View {
    let colors: [String]
    let selected: String
    let choose: (String) -> Void
    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 44), spacing: 4)], spacing: 4) {
            ForEach(colors, id: \.self) { hex in
                Button { choose(hex) } label: {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color(uiColor: UIColor(hex: hex)))
                        .frame(width: 30, height: 30)
                        .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(Color.primary.opacity(0.25)) }
                        .overlay {
                            if hex.caseInsensitiveCompare(selected) == .orderedSame {
                                Image(systemName: "checkmark.circle.fill").symbolRenderingMode(.palette)
                                    .foregroundStyle(.white, Color(uiColor: .systemBlue))
                            }
                        }.frame(maxWidth: .infinity, minHeight: 44)
                        .contentShape(Rectangle())
                }.buttonStyle(.plain)
                    .accessibilityLabel(L("appearance.hex") + " #" + hex)
                    .accessibilityAddTraits(hex.caseInsensitiveCompare(selected) == .orderedSame ? [.isSelected] : [])
            }
        }
    }
}
