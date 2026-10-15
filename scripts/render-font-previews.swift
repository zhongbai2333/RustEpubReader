#!/usr/bin/env swift
// Build-time only: swift scripts/render-font-previews.swift /path/to/verified-fonts
// Apps ship these small masks, never the source fonts. Rendering uses the file's
// own CoreText descriptor, and rejects missing glyphs or clipped ink.
import Foundation
import CoreText
import CoreGraphics
import CryptoKit
import ImageIO
import UniformTypeIdentifiers

struct Entry: Decodable { let id: String; let filename: String; let sha256: String; let postscript: String; let language: String }
struct Sample: Codable {
    let id: String; let file: String; let sha256: String; let font_sha256: String
    let postscript: String; let text: String; let width: Int; let height: Int
}
struct Manifest: Codable { let renderer: String; let font_size: Int; let samples: [Sample] }
func require(_ condition: @autoclosure () -> Bool, _ reason: String) throws {
    if !condition() { throw NSError(domain: "FontPreview", code: 1, userInfo: [NSLocalizedDescriptionKey: reason]) }
}
func digest(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }
let root = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().deletingLastPathComponent()
try require(CommandLine.arguments.count == 2, "Usage: swift scripts/render-font-previews.swift FONT_DIRECTORY")
let inputs = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let catalogURL = root.appendingPathComponent("resources/fonts/catalog.json")
let entries = try JSONDecoder().decode([Entry].self, from: Data(contentsOf: catalogURL))
let output = root.appendingPathComponent("resources/fonts/previews", isDirectory: true)
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
let width = 1200, height = 216, size: CGFloat = 68
var samples: [Sample] = []
for entry in entries {
    let file = inputs.appendingPathComponent(entry.filename)
    let source = try Data(contentsOf: file)
    try require(digest(source) == entry.sha256, "Font checksum: \(entry.id)")
    guard let descriptors = CTFontManagerCreateFontDescriptorsFromURL(file as CFURL) as? [CTFontDescriptor], let descriptor = descriptors.first else { fatalError("Font descriptor: \(entry.id)") }
    let font = CTFontCreateWithFontDescriptor(descriptor, size, nil)
    try require(CTFontCopyPostScriptName(font) as String == entry.postscript, "Unexpected face: \(entry.id)")
    let traditional = entry.id.hasSuffix("-tc")
    let lines = entry.language == "en" ? ["The moon rises over a quiet sea.", "Aa Bb Qq · 0123456789"] :
        (traditional ? ["春江潮水連海平，海上明月共潮生。", "書中自有山川日月。Aa 0123"] : ["春江潮水连海平，海上明月共潮生。", "书中自有山川日月。Aa 0123"])
    guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fatalError("Bitmap") }
    context.setAllowsAntialiasing(true); context.setShouldAntialias(true)
    for (index, text) in lines.enumerated() {
        let units = Array(text.utf16); var glyphs = [CGGlyph](repeating: 0, count: units.count)
        try require(CTFontGetGlyphsForCharacters(font, units, &glyphs, units.count) && glyphs.allSatisfy { $0 != 0 }, "Missing sample glyph: \(entry.id)")
        let attributes: [NSAttributedString.Key: Any] = [.init(kCTFontAttributeName as String): font,
            .init(kCTForegroundColorAttributeName as String): CGColor(gray: 1, alpha: 1)]
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attributes))
        context.textPosition = .zero
        let ink = CTLineGetImageBounds(line, context)
        let origin = CGPoint(x: 24, y: height - 86 - index * 88)
        let bounds = ink.offsetBy(dx: origin.x, dy: origin.y)
        try require(bounds.minX >= 10 && bounds.maxX <= CGFloat(width - 10) && bounds.minY >= 10 && bounds.maxY <= CGFloat(height - 10), "Clipped sample: \(entry.id) \(bounds)")
        context.textPosition = origin; CTLineDraw(line, context)
    }
    guard let image = context.makeImage() else { fatalError("Image") }
    let url = output.appendingPathComponent(entry.id + ".png")
    guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else { fatalError("PNG destination") }
    CGImageDestinationAddImage(destination, image, nil)
    try require(CGImageDestinationFinalize(destination), "PNG write: \(entry.id)")
    let data = try Data(contentsOf: url)
    samples.append(Sample(id: entry.id, file: entry.id + ".png", sha256: digest(data), font_sha256: entry.sha256,
        postscript: entry.postscript, text: lines.joined(separator: "\n"), width: width, height: height))
}
let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
try encoder.encode(Manifest(renderer: "CoreText; white glyphs on transparent RGBA", font_size: Int(size), samples: samples)).write(to: output.appendingPathComponent("manifest.json"), options: .atomic)
// Keep the catalog's accessibility text tied to the rendered specimen.
var records = try JSONSerialization.jsonObject(with: Data(contentsOf: catalogURL)) as! [[String: Any]]
for index in records.indices {
    let sample = samples.first { $0.id == records[index]["id"] as? String }!
    records[index]["preview"] = "previews/" + sample.file
    records[index]["preview_text"] = sample.text
}
try JSONSerialization.data(withJSONObject: records, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]).write(to: catalogURL, options: .atomic)
print("Rendered \(samples.count) previews with exact source faces, glyph coverage and ink bounds verified.")
