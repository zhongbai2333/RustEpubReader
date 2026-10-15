import SwiftUI
import UIKit
import CoreText
import CryptoKit
import UniformTypeIdentifiers

struct DownloadableFont: Codable, Identifiable {
    static var sample: String { L("fonts.sample") }
    let id: String
    let name_zh: String
    let name_en: String
    let postscript: String
    let filename: String
    let url: URL
    let sha256: String
    let bytes: Int
    let license: String
    let license_file: String
    let project_url: URL
    let language: String
    let description_zh: String
    let description_en: String
    let license_url: URL
    let license_text: String
    let preview: String
    let preview_text: String
    var title: String { ReaderI18n.shared.resolvedCode == "zh_cn" ? name_zh : name_en }
    var description: String { ReaderI18n.shared.resolvedCode == "zh_cn" ? description_zh : description_en }
    static let catalog: [Self] = {
        guard let url = Bundle.main.url(forResource: "catalog", withExtension: "json", subdirectory: "fonts"),
              let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder().decode([Self].self, from: data)) ?? []
    }()
}

struct ExternalReaderFont: Decodable, Identifiable {
    let id: String
    let name_zh: String
    let name_en: String
    let language: String
    let description_zh: String
    let description_en: String
    let project_url: URL
    let license_url: URL
    let checked_at: String
    var title: String { ReaderI18n.shared.resolvedCode == "zh_cn" ? name_zh : name_en }
    var description: String { ReaderI18n.shared.resolvedCode == "zh_cn" ? description_zh : description_en }
    static let catalog: [Self] = {
        guard let url = Bundle.main.url(forResource: "external", withExtension: "json", subdirectory: "fonts"),
              let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder().decode([Self].self, from: data)) ?? []
    }()
}

enum ReaderFontError: LocalizedError {
    case invalid, tooLarge, failed, conflict
    var errorDescription: String? {
        switch self {
        case .invalid: return L("fonts.invalid")
        case .tooLarge: return L("fonts.too_large")
        case .failed: return L("fonts.failed")
        case .conflict: return L("fonts.conflict")
        }
    }
}

/// Font files stay in Application Support; only immutable, validated files are
/// registered. No font bytes are sent to sync or installed system-wide.
@MainActor final class ReaderFonts: ObservableObject {
    static let shared = ReaderFonts()
    @Published private(set) var names: [String] = []
    @Published private(set) var busy: Set<String> = []
    @Published var error: String?
    let directory: URL
    private let defaults: UserDefaults
    init(directory: URL? = nil, defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.directory = directory ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("ReaderFonts", isDirectory: true)
        try? FileManager.default.createDirectory(at: self.directory, withIntermediateDirectories: true)
        for url in (try? FileManager.default.contentsOfDirectory(at: self.directory, includingPropertiesForKeys: nil)) ?? [] {
            if ["ttf", "otf"].contains(url.pathExtension.lowercased()) { try? register(url) }
        }
    }
    func restoreSystemFonts() {
        let saved = defaults.stringArray(forKey: "reader.systemFonts") ?? []
        names = Array(Set(names + saved)).sorted()
        let missing = saved.filter { UIFont(name: $0, size: 18) == nil }
        guard !missing.isEmpty else { return }
        let descriptors = missing.map { CTFontDescriptorCreateWithAttributes([kCTFontNameAttribute: $0] as CFDictionary) }
        CTFontManagerRequestFonts(descriptors as CFArray) { [weak self] _ in
            Task { @MainActor in self?.objectWillChange.send() }
        }
    }
    func rememberSystemFont(_ font: UIFont) {
        var saved = defaults.stringArray(forKey: "reader.systemFonts") ?? []
        if !saved.contains(font.fontName) { saved.append(font.fontName) }
        defaults.set(saved, forKey: "reader.systemFonts")
        names = Array(Set(names + [font.fontName])).sorted()
    }
    func register(_ url: URL) throws {
        guard let descriptors = CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor], !descriptors.isEmpty else { throw ReaderFontError.invalid }
        var failure: Unmanaged<CFError>?
        let registered = CTFontManagerRegisterFontsForURL(url as CFURL, .process, &failure)
        if !registered {
            let code = failure.map { CFErrorGetCode($0.takeRetainedValue()) }
            if code == CTFontManagerError.duplicatedName.rawValue {
                let (_, digest) = try Self.fingerprint(url)
                guard Self.matchingRegisteredFile(descriptors, digest: digest) != nil else { throw ReaderFontError.conflict }
            } else if code != CTFontManagerError.alreadyRegistered.rawValue { throw ReaderFontError.invalid }
        }
        let added = descriptors.compactMap { CTFontDescriptorCopyAttribute($0, kCTFontNameAttribute) as? String }
        guard !added.isEmpty else { throw ReaderFontError.invalid }
        names = Array(Set(names + added)).sorted()
    }
    nonisolated static func matchingRegisteredFile(_ descriptors: [CTFontDescriptor], digest: String) -> URL? {
        var source: URL?
        for descriptor in descriptors {
            guard let name = CTFontDescriptorCopyAttribute(descriptor, kCTFontNameAttribute) as? String else { return nil }
            let font = CTFontCreateWithName(name as CFString, 18, nil)
            guard CTFontCopyPostScriptName(font) as String == name,
                  let url = CTFontCopyAttribute(font, kCTFontURLAttribute) as? URL,
                  source == nil || source == url else { return nil }
            source = url
        }
        guard let source, let actual = try? fingerprint(source), actual.1 == digest else { return nil }
        return source
    }
    nonisolated static func fingerprint(_ url: URL, limit: Int = 64 * 1024 * 1024) throws -> (Int, String) {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hash = SHA256(), count = 0
        while let chunk = try handle.read(upToCount: 256 * 1024), !chunk.isEmpty {
            count += chunk.count
            guard count <= limit else { throw ReaderFontError.tooLarge }
            hash.update(data: chunk)
        }
        return (count, hash.finalize().map { String(format: "%02x", $0) }.joined())
    }
    nonisolated static func prepareImport(_ url: URL, directory: URL) throws -> (URL, Bool) {
        let granted = url.startAccessingSecurityScopedResource()
        defer { if granted { url.stopAccessingSecurityScopedResource() } }
        let ext = url.pathExtension.lowercased()
        guard ["ttf", "otf"].contains(ext) else { throw ReaderFontError.invalid }
        let staged = directory.appendingPathComponent(UUID().uuidString + ".part")
        try Data().write(to: staged, options: .withoutOverwriting)
        defer { try? FileManager.default.removeItem(at: staged) }
        let input = try FileHandle(forReadingFrom: url), output = try FileHandle(forWritingTo: staged)
        defer { try? input.close(); try? output.close() }
        var hash = SHA256(), count = 0
        while let chunk = try input.read(upToCount: 256 * 1024), !chunk.isEmpty {
            count += chunk.count
            guard count <= 64 * 1024 * 1024 else { throw ReaderFontError.tooLarge }
            hash.update(data: chunk); try output.write(contentsOf: chunk)
        }
        try output.synchronize(); try output.close()
        let digest = hash.finalize().map { String(format: "%02x", $0) }.joined()
        guard let faces = CTFontManagerCreateFontDescriptorsFromURL(staged as CFURL) as? [CTFontDescriptor], !faces.isEmpty else { throw ReaderFontError.invalid }
        if let existing = matchingRegisteredFile(faces, digest: digest),
           existing.deletingLastPathComponent().standardizedFileURL == directory.standardizedFileURL { return (existing, false) }
        let target = directory.appendingPathComponent(digest + "." + ext)
        if FileManager.default.fileExists(atPath: target.path) {
            if (try? fingerprint(target).1) == digest { return (target, false) }
            _ = try FileManager.default.replaceItemAt(target, withItemAt: staged)
        } else { try FileManager.default.moveItem(at: staged, to: target) }
        return (target, true)
    }
    func importFiles(_ urls: [URL]) async {
        guard !busy.contains("import") else { return }
        busy.insert("import"); defer { busy.remove("import") }
        for url in urls {
            do {
                let directory = self.directory
                let destination = try await Task.detached(priority: .userInitiated) {
                    try Self.prepareImport(url, directory: directory)
                }.value
                do { try register(destination.0) }
                catch { if destination.1 { try? FileManager.default.removeItem(at: destination.0) }; throw error }
            } catch { self.error = error.localizedDescription }
        }
    }
    func installed(_ font: DownloadableFont) -> Bool {
        names.contains(font.postscript) && (try? directory.appendingPathComponent(font.filename).resourceValues(forKeys: [.fileSizeKey]).fileSize) == font.bytes
    }
    nonisolated static func validate(_ url: URL, as font: DownloadableFont) throws {
        let (count, digest) = try fingerprint(url)
        guard count == font.bytes, digest == font.sha256 else { throw ReaderFontError.invalid }
        guard let faces = CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor], faces.contains(where: { CTFontDescriptorCopyAttribute($0, kCTFontNameAttribute) as? String == font.postscript }) else { throw ReaderFontError.invalid }
    }
    nonisolated static func prepareDownload(_ source: URL, as font: DownloadableFont, directory: URL) throws -> (URL, Bool) {
        try validate(source, as: font)
        let target = directory.appendingPathComponent(font.filename)
        if (try? fingerprint(target).1) == font.sha256 { return (target, false) }
        let staged = directory.appendingPathComponent(UUID().uuidString + ".part")
        defer { try? FileManager.default.removeItem(at: staged) }
        try FileManager.default.copyItem(at: source, to: staged)
        if FileManager.default.fileExists(atPath: target.path) {
            _ = try FileManager.default.replaceItemAt(target, withItemAt: staged)
        } else { try FileManager.default.moveItem(at: staged, to: target) }
        return (target, true)
    }
    func download(_ font: DownloadableFont) async {
        guard !busy.contains(font.id) else { return }
        busy.insert(font.id); defer { busy.remove(font.id) }
        do {
            var request = URLRequest(url: font.url); request.timeoutInterval = 120
            let (temp, response) = try await URLSession.shared.download(for: request)
            defer { try? FileManager.default.removeItem(at: temp) }
            guard let response = response as? HTTPURLResponse, response.statusCode == 200 else { throw ReaderFontError.failed }
            let directory = self.directory
            let destination = try await Task.detached(priority: .userInitiated) {
                try Self.prepareDownload(temp, as: font, directory: directory)
            }.value
            do { try register(destination.0) }
            catch { if destination.1 { try? FileManager.default.removeItem(at: destination.0) }; throw error }
        } catch is CancellationError {
        } catch { self.error = (error as? ReaderFontError)?.localizedDescription ?? L("fonts.failed") }
    }
}

struct SystemReaderFontPicker: UIViewControllerRepresentable {
    let onSelect: (UIFont) -> Void
    let onDismiss: () -> Void
    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeUIViewController(context: Context) -> UIFontPickerViewController {
        let configuration = UIFontPickerViewController.Configuration()
        configuration.includeFaces = true
        let picker = UIFontPickerViewController(configuration: configuration)
        picker.delegate = context.coordinator
        return picker
    }
    func updateUIViewController(_ picker: UIFontPickerViewController, context: Context) { context.coordinator.parent = self }
    final class Coordinator: NSObject, UIFontPickerViewControllerDelegate {
        var parent: SystemReaderFontPicker
        init(_ parent: SystemReaderFontPicker) { self.parent = parent }
        func fontPickerViewControllerDidPickFont(_ viewController: UIFontPickerViewController) {
            if let descriptor = viewController.selectedFontDescriptor { parent.onSelect(UIFont(descriptor: descriptor, size: 18)) }
            parent.onDismiss()
        }
        func fontPickerViewControllerDidCancel(_ viewController: UIFontPickerViewController) { parent.onDismiss() }
    }
}

@MainActor private enum ReaderFontPreviewAssets {
    static let cache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>(); cache.countLimit = 24; cache.totalCostLimit = 24 * 1024 * 1024; return cache
    }()
    static func image(_ path: String) -> UIImage? {
        if let image = cache.object(forKey: path as NSString) { return image }
        guard let file = Bundle.main.resourceURL?.appendingPathComponent("fonts").appendingPathComponent(path),
              let image = UIImage(contentsOfFile: file.path) else { return nil }
        cache.setObject(image, forKey: path as NSString, cost: Int(image.size.width * image.size.height * 4))
        return image
    }
}

private struct ReaderFontSpecimen: View {
    let font: DownloadableFont
    var body: some View {
        if let image = ReaderFontPreviewAssets.image(font.preview) {
            Image(uiImage: image).renderingMode(.template).resizable()
                .aspectRatio(1200.0 / 216.0, contentMode: .fit).frame(maxWidth: 320, alignment: .leading)
                .foregroundStyle(.primary).accessibilityLabel(font.preview_text)
        }
    }
}

struct ReaderFontDownloads: View {
    @ObservedObject private var fonts = ReaderFonts.shared
    @ObservedObject private var language = ReaderI18n.shared
    @Binding var selection: String
    var kind: ReaderFontPicker.Kind = .chinese
    @State private var importing = false
    private var code: String { kind == .chinese ? "zh" : "en" }
    var body: some View {
        List {
            Section {
                Text(L("fonts.description")).foregroundStyle(.secondary)
                Text(L("fonts.preview_note")).font(.caption).foregroundStyle(.secondary)
                Text(L(kind == .chinese ? "fonts.use_chinese" : "fonts.use_english")).font(.subheadline)
                Button(L("fonts.import")) { importing = true }.disabled(fonts.busy.contains("import"))
                if fonts.busy.contains("import") { HStack { ProgressView(); Text(L("fonts.importing")) } }
            }
            Section {
                ForEach(DownloadableFont.catalog.filter { $0.language == code }) { font in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(font.title).font(.headline)
                        Text(font.description).font(.subheadline).foregroundStyle(.secondary)
                        ReaderFontSpecimen(font: font)
                        HStack {
                            Text(ByteCountFormatter.string(fromByteCount: Int64(font.bytes), countStyle: .file)).font(.caption).foregroundStyle(.secondary)
                            Spacer()
                            if fonts.installed(font) {
                                if selection == font.postscript { Label(L("fonts.selected"), systemImage: "checkmark").font(.subheadline).foregroundStyle(.secondary) }
                                else { Button(L("fonts.use")) { selection = font.postscript } }
                            } else if fonts.busy.contains(font.id) {
                                ProgressView().accessibilityLabel(L("fonts.downloading"))
                            } else {
                                Button(L("fonts.download")) { Task { await fonts.download(font) } }
                            }
                        }
                        NavigationLink { ReaderFontLicense(font: font) } label: {
                            Text(L("fonts.details") + " · " + font.license).font(.caption)
                        }
                    }.padding(.vertical, 5).buttonStyle(.borderless)
                }
            } header: { Text(L("fonts.direct")) } footer: { Text(L("fonts.open_note")) }
            if ExternalReaderFont.catalog.contains(where: { $0.language == code }) {
                Section {
                    ForEach(ExternalReaderFont.catalog.filter { $0.language == code }) { font in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(font.title).font(.headline)
                            Text(font.description).font(.subheadline).foregroundStyle(.secondary)
                            Text(L("fonts.personal")).font(.caption).foregroundStyle(.secondary)
                            HStack { Link(L("fonts.official_preview"), destination: font.project_url); Spacer(); Link(L("fonts.official_terms"), destination: font.license_url) }.font(.subheadline)
                            Text(L("fonts.checked") + " · " + font.checked_at).font(.caption2).foregroundStyle(.secondary)
                        }.padding(.vertical, 5)
                    }
                } header: { Text(L("fonts.external")) } footer: { Text(L("fonts.personal_note") + "\n" + L("fonts.import_note")) }
            }
        }.navigationTitle(L("fonts.library")).navigationBarTitleDisplayMode(.inline)
            .fileImporter(isPresented: $importing, allowedContentTypes: [UTType(filenameExtension: "ttf") ?? .font, UTType(filenameExtension: "otf") ?? .font], allowsMultipleSelection: true) { result in
                switch result {
                case .success(let urls): Task { await fonts.importFiles(urls) }
                case .failure(let error): fonts.error = error.localizedDescription
                }
            }
            .alert(L("fonts.error"), isPresented: Binding(get: { fonts.error != nil }, set: { if !$0 { fonts.error = nil } })) { Button(L("error.ok")) { fonts.error = nil } } message: { Text(fonts.error ?? "") }
    }
}

private struct ReaderFontLicense: View {
    let font: DownloadableFont
    var body: some View {
        List {
            Section(font.title) {
                Text(L("fonts.open_note"))
                Link(L("fonts.official"), destination: font.project_url)
                Link(L("fonts.official_terms"), destination: font.license_url)
            }
            Section(L("fonts.license_text")) { Text(font.license_text).font(.footnote).textSelection(.enabled) }
        }.navigationTitle(L("fonts.details")).navigationBarTitleDisplayMode(.inline)
    }
}
