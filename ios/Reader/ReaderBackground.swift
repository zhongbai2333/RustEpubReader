import Foundation
import UIKit
import ImageIO

// A filename, rather than an absolute sandbox path, survives app-container moves.
enum ReaderBackgroundStore {
    enum Failure: Error { case invalidImage, invalidName }
    static var directory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ReaderAppearance", isDirectory: true)
    }
    private static let cache: NSCache<NSString, UIImage> = {
        let value = NSCache<NSString, UIImage>(); value.countLimit = 2; value.totalCostLimit = 32 * 1024 * 1024
        return value
    }()
    static func url(_ name: String) -> URL? {
        guard name.hasPrefix("background-"), name.hasSuffix(".png"), !name.contains("/"), !name.contains("\\"), !name.contains("..") else { return nil }
        return directory.appendingPathComponent(name)
    }
    static func importImage(_ data: Data) throws -> String {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let cg = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: 2048,
                kCGImageSourceShouldCacheImmediately: true
              ] as CFDictionary), cg.width > 0, cg.height > 0,
              let encoded = UIImage(cgImage: cg).pngData() else { throw Failure.invalidImage }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let name = "background-\(UUID().uuidString).png"
        try encoded.write(to: directory.appendingPathComponent(name), options: .atomic)
        let image = UIImage(cgImage: cg)
        cache.setObject(image, forKey: name as NSString, cost: cg.bytesPerRow * cg.height)
        return name
    }
    static func importFile(_ file: URL) throws -> String {
        let scoped = file.startAccessingSecurityScopedResource()
        defer { if scoped { file.stopAccessingSecurityScopedResource() } }
        return try importImage(Data(contentsOf: file, options: .mappedIfSafe))
    }
    static func image(_ name: String) -> UIImage? {
        guard let file = url(name) else { return nil }
        if let value = cache.object(forKey: name as NSString) { return value }
        guard let value = UIImage(contentsOfFile: file.path) else { return nil }
        cache.setObject(value, forKey: name as NSString, cost: (value.cgImage?.bytesPerRow ?? 0) * (value.cgImage?.height ?? 0))
        return value
    }
    static func remove(_ name: String) {
        guard let file = url(name) else { return }
        cache.removeObject(forKey: name as NSString)
        try? FileManager.default.removeItem(at: file)
    }
}

extension ReaderOptions {
    var backgroundImage: UIImage? { backgroundImageEnabled ? ReaderBackgroundStore.image(backgroundImageName) : nil }
    // Background painting changes must not invalidate CoreText layouts or anchors.
    var textLayoutOptions: ReaderOptions {
        var value = self
        value.language = ""; value.backgroundImageName = ""; value.backgroundImageEnabled = false; value.backgroundImageOpacity = 0
        return value
    }
}

import SwiftUI
import PhotosUI
import UniformTypeIdentifiers

struct ReaderBackgroundSettings: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @Binding var name: String
    @Binding var enabled: Bool
    @Binding var opacity: Double
    @State private var photo: PhotosPickerItem?
    @State private var filePicker = false
    @State private var importing = false
    @State private var failed = false
    var body: some View {
        Section {
            if let image = ReaderBackgroundStore.image(name) {
                HStack(spacing: 12) {
                    Image(uiImage: image).resizable().scaledToFill().frame(width: 64, height: 64).clipShape(RoundedRectangle(cornerRadius: 10))
                    Toggle(isOn: $enabled) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(L("settings.show_background_image"))
                            Text(L("settings.image_selected")).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
            PhotosPicker(selection: $photo, matching: .images) {
                Label(importing ? L("common.importing_image") : L("common.select_from_photos"), systemImage: "photo")
            }.disabled(importing)
            Button { filePicker = true } label: { Label(L("common.select_from_files"), systemImage: "folder") }.disabled(importing)
            if !name.isEmpty {
                VStack(spacing: 4) {
                    LabeledContent(L("common.opacity"), value: "\(Int(opacity * 100))%")
                    Slider(value: $opacity, in: 0...1, step: 0.01).accessibilityLabel(L("settings.background_image_opacity"))
                }.disabled(!enabled)
                Button(L("common.remove_image"), role: .destructive) {
                    let old = name; name = ""; ReaderBackgroundStore.remove(old)
                }.disabled(importing)
            }
        } header: { Text(L("common.background_image")) } footer: {
            Text(L("common.background_image_desc"))
        }
        .onChange(of: photo) { _, item in
            guard let item else { return }
            importing = true
            Task {
                defer { importing = false; photo = nil }
                do {
                    guard let data = try await item.loadTransferable(type: Data.self) else { throw ReaderBackgroundStore.Failure.invalidImage }
                    let result = try await Task.detached(priority: .userInitiated) { try ReaderBackgroundStore.importImage(data) }.value
                    replace(with: result)
                } catch { failed = true }
            }
        }
        .fileImporter(isPresented: $filePicker, allowedContentTypes: [.image]) { result in
            switch result {
            case .success(let url):
                importing = true
                Task {
                    defer { importing = false }
                    do { let name = try await Task.detached(priority: .userInitiated) { try ReaderBackgroundStore.importFile(url) }.value; replace(with: name) }
                    catch { failed = true }
                }
            case .failure: failed = true
            }
        }
        .alert(L("common.error_image_unusable"), isPresented: $failed) { Button(L("manage.done"), role: .cancel) {} }
            message: { Text(L("common.error_image_read_failed")) }
    }
    private func replace(with result: String) {
        let old = name; name = result; enabled = true
        if old != result { ReaderBackgroundStore.remove(old) }
    }
}
