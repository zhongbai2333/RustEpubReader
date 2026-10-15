#if DEBUG
import Foundation
import UIKit
import CoreText

// Runs the real Rust bridge, native decoder, CoreText pagination and image painter.
// The fixture is supplied in this diagnostic app's Documents directory.
enum IllustrationRuntimeCheck {
    static func run() {
        var report: [String: Any] = [:]
        do {
            let files = FileManager.default
            let documents = files.urls(for: .documentDirectory, in: .userDomainMask)[0]
            let root = files.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("ReaderLibrary")
            let client = CoreClient(root: root.path, device: "illustration-check")
            let entry: LibraryEntry = try client.call("import", ["path": documents.appendingPathComponent("illustrations.epub").path])
            let book: PreviewBook = try client.call("open", ["id": entry.id])
            let options = ReaderOptions(fontSize: 18, lineHeight: 1.8, fontName: "PingFangSC-Regular", theme: "light", sliding: false)
            var checks: [[String: Any]] = []
            for width: CGFloat in [320, 640] {
                let rect = CGRect(x: 0, y: 0, width: width, height: 600)
                BookTypesetter.prepareImages(book.chapters)
                let chapters = BookTypesetter.chapters(book.chapters, options: options, rect: rect)
                var decoded = 0
                for chapter in chapters {
                    chapter.text.enumerateAttribute(BookTypesetter.imageKey, in: NSRange(location: 0, length: chapter.text.length)) { value, _, _ in
                        if BookTypesetter.isIllustration(value) { decoded += 1 }
                    }
                }
                let pages = ReadingLayout.paginate(chapters, in: rect)
                var painted = 0
                var firstPagesWithImages = 0
                var seenChapters = Set<Int>()
                for page in pages {
                    let setter = CTFramesetterCreateWithAttributedString(page.text)
                    let frame = CTFramesetterCreateFrame(setter, page.range, CGPath(rect: rect, transform: nil), nil)
                    guard let context = CGContext(data: nil, width: Int(width), height: 600, bitsPerComponent: 8, bytesPerRow: Int(width) * 4,
                        space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue) else { throw CoreFailure(message: "Cannot create image check bitmap") }
                    context.setFillColor(UIColor.white.cgColor); context.fill(rect)
                    BookTypesetter.drawImages(frame: frame, context: context)
                    let bytes = context.data!.assumingMemoryBound(to: UInt8.self)
                    var red = 0, green = 0, blue = 0
                    for offset in stride(from: 0, to: Int(width) * 600 * 4, by: 4) {
                        let r = Int(bytes[offset]), g = Int(bytes[offset + 1]), b = Int(bytes[offset + 2])
                        if r > g + 40 && r > b + 40 { red += 1 }
                        if g > r + 30 && g > b + 20 { green += 1 }
                        if b > r + 40 && b > g + 20 { blue += 1 }
                    }
                    let firstPage = seenChapters.insert(page.chapter).inserted
                    if red > 100 && green > 100 && blue > 100 {
                        painted += 1
                        if firstPage { firstPagesWithImages += 1 }
                    }
                }
                checks.append(["width": width, "decoded_images": decoded, "pages_with_all_three_bands": painted, "page_count": pages.count, "first_pages_with_images": firstPagesWithImages,
                    "passed": decoded == 8 && painted == 8 && firstPagesWithImages == 8])
            }
            report["checks"] = checks
            report["passed"] = checks.allSatisfy { $0["passed"] as? Bool == true }
        } catch { report["passed"] = false; report["error"] = String(describing: error) }
        let target = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("illustration-verification.json")
        if let data = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]) { try? data.write(to: target, options: .atomic) }
    }
}
#endif
