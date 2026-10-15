import Foundation
import UIKit

@MainActor enum FontsRuntimeCheck {
    private static var ran = false
    private static func require(_ value: @autoclosure () -> Bool) throws {
        if !value() { throw ReaderFontError.invalid }
    }
    static func run() async {
        guard !ran else { return }; ran = true
        let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let root = documents.appendingPathComponent("font-check")
        let store = ReaderFonts(directory: root)
        var report: [String: Any] = [:]
        do {
            let wenkai = DownloadableFont.catalog.first { $0.id == "lxgw-wenkai" }!
            let sans = DownloadableFont.catalog.first { $0.id == "source-han-sans-sc" }!
            let source = documents.appendingPathComponent(wenkai.filename)
            try ReaderFonts.validate(source, as: wenkai)
            await store.importFiles([source])
            try require(store.error == nil && store.names.contains(wenkai.postscript))
            let count = try FileManager.default.contentsOfDirectory(atPath: root.path).count
            await store.importFiles([source])
            let afterDuplicate = try FileManager.default.contentsOfDirectory(atPath: root.path).count
            try require(afterDuplicate == count)
            report["import_and_duplicate"] = true
            let invalid = documents.appendingPathComponent("invalid.ttf")
            try Data("not a font".utf8).write(to: invalid)
            await store.importFiles([invalid]); try require(store.error != nil)
            try require(store.names.contains(wenkai.postscript))
            do { try ReaderFonts.validate(invalid, as: wenkai); throw ReaderFontError.failed }
            catch ReaderFontError.invalid { report["invalid_and_checksum_rejected"] = true }
            let repair = documents.appendingPathComponent("font-repair-check")
            try FileManager.default.createDirectory(at: repair, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: repair) }
            try Data("partial download".utf8).write(to: repair.appendingPathComponent(wenkai.filename))
            let repaired = try ReaderFonts.prepareDownload(source, as: wenkai, directory: repair)
            try ReaderFonts.validate(repaired.0, as: wenkai)
            try require(repaired.1)
            report["partial_download_repaired_atomically"] = true
            let restored = ReaderFonts(directory: root)
            try require(restored.names.contains(wenkai.postscript))
            guard let font = UIFont(name: wenkai.postscript, size: 18) else { throw ReaderFontError.invalid }
            try require(ReaderFontPicker.supportsChinese(font) && ReaderFontPicker.supportsLatin(font))
            report["persistent_registration_and_glyphs"] = true
            try require(ReaderFontPicker.families(for: .latin).contains("PingFang SC"))
            report["cjk_fonts_available_for_latin"] = true
            let chapter = try JSONDecoder().decode(BookChapter.self, from: Data(#"{"title":"Font check","blocks":[{"type":"Paragraph","spans":[{"text":"Aa 0123 中文","style":"Normal"}]}]}"#.utf8))
            var options = ReaderOptions(fontSize: 18, lineHeight: 1.5, fontName: "", theme: "light", sliding: false, cjkFontName: wenkai.postscript)
            let bounds = CGRect(x: 0, y: 0, width: 320, height: 600)
            let followed = BookTypesetter.chapters([chapter], options: options, rect: bounds)[0].text
            try require((followed.attribute(.font, at: 0, effectiveRange: nil) as? UIFont)?.fontName == wenkai.postscript)
            options.fontName = "Helvetica"
            let separate = BookTypesetter.chapters([chapter], options: options, rect: bounds)[0].text
            try require((separate.attribute(.font, at: 0, effectiveRange: nil) as? UIFont)?.fontName == "Helvetica")
            report["latin_follows_chinese_and_can_override"] = true
            let fixtures = documents.appendingPathComponent("font-catalog")
            if FileManager.default.fileExists(atPath: fixtures.path) {
                for entry in DownloadableFont.catalog {
                    guard let imageURL = Bundle.main.resourceURL?.appendingPathComponent("fonts").appendingPathComponent(entry.preview),
                          let image = UIImage(contentsOfFile: imageURL.path)?.cgImage else { throw ReaderFontError.invalid }
                    try require(image.width == 1200 && image.height == 216)
                    let file = fixtures.appendingPathComponent(entry.filename)
                    try ReaderFonts.validate(file, as: entry)
                    try store.register(file)
                    guard let font = UIFont(name: entry.postscript, size: 18) else { throw ReaderFontError.invalid }
                    try require(ReaderFontPicker.supportsLatin(font))
                    if entry.language == "zh" { try require(ReaderFontPicker.supportsChinese(font)) }
                }
                report["catalog_faces_and_glyphs"] = DownloadableFont.catalog.count
                report["bundled_preview_images"] = DownloadableFont.catalog.count
                try require(ExternalReaderFont.catalog.count == 2)
                report["official_only_entries"] = true
            }
            store.error = nil
            await store.download(sans)
            guard store.error == nil, store.installed(sans) else { throw ReaderFontError.failed }
            try ReaderFonts.validate(root.appendingPathComponent(sans.filename), as: sans)
            report["real_download_verified_and_registered"] = true
            report["passed"] = true
        } catch { report["passed"] = false; report["error"] = error.localizedDescription }
        try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]).write(to: documents.appendingPathComponent("font-check.json"))
    }
}
