import UIKit
import CoreText

private final class ImageMetrics {
    let size: CGSize
    init(_ size: CGSize) { self.size = size }
}
enum BookTypesetter {
    static let imageKey = NSAttributedString.Key("ReaderImage")
    static func chapters(_ chapters: [BookChapter], options: ReaderOptions, rect: CGRect) -> [LaidOutChapter] {
        let font = UIFont(name: options.fontName, size: options.fontSize) ?? .systemFont(ofSize: options.fontSize)
        return chapters.map { chapter in
            let text = NSMutableAttributedString(string: "")
            var coordinates: [BlockCoordinate] = []
            for (index, block) in chapter.blocks.enumerated() {
                let source = (block.spans ?? []).map(\.text).joined()
                coordinates.append(.init(block: block.core_block ?? index, start: text.length, source: source, base: block.core_char_offset ?? 0))
                let paragraph = NSMutableParagraphStyle()
                paragraph.paragraphSpacing = font.pointSize * 0.65
                paragraph.alignment = .justified
                switch block.type {
                case "Paragraph", "Heading":
                    let heading = block.type == "Heading"
                    let size = heading ? font.pointSize * max(1.05, 1.5 - Double(block.level ?? 1) * 0.08) : font.pointSize
                    paragraph.minimumLineHeight = size * options.lineHeight
                    paragraph.maximumLineHeight = paragraph.minimumLineHeight
                    paragraph.firstLineHeadIndent = heading ? 0 : font.pointSize * 2
                    if heading { paragraph.alignment = .center; paragraph.paragraphSpacing = 24 }
                    for span in block.spans ?? [] {
                        var traits: UIFontDescriptor.SymbolicTraits = []
                        if heading || span.style == "Bold" || span.style == "BoldItalic" { traits.insert(.traitBold) }
                        if span.style == "Italic" || span.style == "BoldItalic" { traits.insert(.traitItalic) }
                        let base = font.withSize(size)
                        let styled = base.fontDescriptor.withSymbolicTraits(traits).map { UIFont(descriptor: $0, size: size) } ?? base
                        var attrs: [NSAttributedString.Key: Any] = [.font: styled, .foregroundColor: options.ink, .paragraphStyle: paragraph]
                        if span.link_url != nil { attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue }
                        text.append(NSAttributedString(string: span.text, attributes: attrs))
                    }
                case "Image":
                    paragraph.alignment = .center
                    if let encoded = block.data, let bytes = Data(base64Encoded: encoded), let image = UIImage(data: bytes), image.size.width > 0, image.size.height > 0 {
                        let factor = min(1, min(rect.width / image.size.width, max(40, rect.height - 36) / image.size.height))
                        let size = CGSize(width: image.size.width * factor, height: image.size.height * factor)
                        var callbacks = CTRunDelegateCallbacks(version: kCTRunDelegateVersion1,
                            dealloc: { Unmanaged<ImageMetrics>.fromOpaque($0).release() },
                            getAscent: { Unmanaged<ImageMetrics>.fromOpaque($0).takeUnretainedValue().size.height },
                            getDescent: { _ in 0 },
                            getWidth: { Unmanaged<ImageMetrics>.fromOpaque($0).takeUnretainedValue().size.width })
                        let pointer = Unmanaged.passRetained(ImageMetrics(size)).toOpaque()
                        if let delegate = CTRunDelegateCreate(&callbacks, pointer) {
                            text.append(NSAttributedString(string: "\u{fffc}", attributes: [NSAttributedString.Key(kCTRunDelegateAttributeName as String): delegate, imageKey: image, .paragraphStyle: paragraph, .foregroundColor: UIColor.clear]))
                        } else { Unmanaged<ImageMetrics>.fromOpaque(pointer).release() }
                    } else {
                        text.append(NSAttributedString(string: block.alt ?? "[Image]", attributes: [.font: font, .foregroundColor: options.ink, .paragraphStyle: paragraph]))
                    }
                case "Separator":
                    paragraph.alignment = .center
                    text.append(NSAttributedString(string: "—", attributes: [.font: font, .foregroundColor: options.ink, .paragraphStyle: paragraph]))
                default: break
                }
                text.append(NSAttributedString(string: "\n", attributes: [.font: font, .foregroundColor: options.ink, .paragraphStyle: paragraph]))
            }
            if text.length == 0 {
                coordinates.append(.init(block: 0, start: 0, source: ""))
                text.append(NSAttributedString(string: chapter.title + "\n", attributes: [.font: font, .foregroundColor: options.ink]))
            }
            return LaidOutChapter(text: text, coordinates: coordinates)
        }
    }
    static func drawImages(frame: CTFrame, context: CGContext) {
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = Array(repeating: CGPoint.zero, count: lines.count)
        CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
        let origin = CTFrameGetPath(frame).boundingBox.origin
        for (index, line) in lines.enumerated() {
            for run in CTLineGetGlyphRuns(line) as! [CTRun] {
                let attrs = CTRunGetAttributes(run) as NSDictionary
                guard let image = attrs[imageKey] as? UIImage, let cgImage = image.cgImage else { continue }
                var ascent: CGFloat = 0, descent: CGFloat = 0
                let width = CTRunGetTypographicBounds(run, CFRange(location: 0, length: 0), &ascent, &descent, nil)
                let x = CTLineGetOffsetForStringIndex(line, CTRunGetStringRange(run).location, nil)
                context.draw(cgImage, in: CGRect(x: origin.x + origins[index].x + x, y: origin.y + origins[index].y - descent, width: width, height: ascent + descent))
            }
        }
    }
}
