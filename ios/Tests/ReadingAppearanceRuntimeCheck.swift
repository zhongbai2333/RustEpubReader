#if DEBUG
import Foundation
import UIKit

enum ReadingAppearanceRuntimeCheck {
    @MainActor static func run() {
        Task { @MainActor in
            var report: [String: Any] = [:]
            do {
                for _ in 0..<50 {
                    if UIApplication.shared.connectedScenes.contains(where: { $0 is UIWindowScene }) { break }
                    try await Task.sleep(for: .milliseconds(100))
                }
                guard let scene = UIApplication.shared.connectedScenes.first(where: { $0 is UIWindowScene }) as? UIWindowScene else { throw CoreFailure(message: "No appearance check scene") }
                let previous = scene.windows.first(where: \.isKeyWindow)
                let host = UIViewController(), window = UIWindow(windowScene: scene)
                window.rootViewController = host; window.makeKeyAndVisible()
                defer { window.isHidden = true; previous?.makeKeyAndVisible() }
                let files = FileManager.default
                let documents = files.urls(for: .documentDirectory, in: .userDomainMask)[0]
                let caches = files.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                let client = CoreClient(root: caches.appendingPathComponent("appearance-library").path, device: "appearance-check")
                let entry: LibraryEntry = try client.call("import", ["path": documents.appendingPathComponent("appearance.epub").path])
                let book: PreviewBook = try client.call("open", ["id": entry.id])
                let anchor = ReadingAnchor(chapter: 5, block: 1, charOffset: 3)
                let data = try Data(contentsOf: documents.appendingPathComponent("appearance-background.png"))
                let imageName = try ReaderBackgroundStore.importImage(data)
                defer { ReaderBackgroundStore.remove(imageName) }
                var checks = 0
                for mode in ReadingMode.allCases {
                    var options = ReaderOptions(fontSize: 18, lineHeight: 1.65, fontName: "Helvetica", theme: "custom", sliding: mode != .curl, cjkFontName: "PingFangSC-Regular", readingMode: mode)
                    let controller = NativeReadingController(book: book, anchor: anchor, options: options)
                    host.addChild(controller); host.view.addSubview(controller.view); controller.didMove(toParent: host)
                    defer { controller.willMove(toParent: nil); controller.view.removeFromSuperview(); controller.removeFromParent() }
                    controller.view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
                    controller.view.setNeedsLayout(); controller.view.layoutIfNeeded()
                    let ranges = controller.pages.map { "\($0.chapter):\($0.range.location):\($0.range.length)" }
                    guard !ranges.isEmpty else { throw CoreFailure(message: "No appearance test pages") }
                    for dark in [true, false, true] {
                        let variant = ReadingStyle.presets[2][dark: dark]
                        options.backgroundHex = variant.paper; options.textHex = variant.ink
                        options.backgroundImageName = imageName; options.backgroundImageOpacity = variant.strength
                        controller.update(options: options, jump: nil, jumpID: 0, annotations: .empty, revision: 0)
                        controller.view.layoutIfNeeded()
                        guard controller.anchor == anchor else { throw CoreFailure(message: "Appearance changed reading anchor: \(mode)") }
                        guard controller.pages.map({ "\($0.chapter):\($0.range.location):\($0.range.length)" }) == ranges else { throw CoreFailure(message: "Appearance changed pagination: \(mode)") }
                        checks += 1
                    }
                }
                report = ["passed": true, "native_paint_checks": checks, "nonzero_anchor_preserved": true, "pagination_preserved": true, "background_image_decoded": true]
            } catch { report = ["passed": false, "error": String(describing: error)] }
            let file = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("appearance-runtime-result.json")
            try? JSONSerialization.data(withJSONObject: report, options: .prettyPrinted).write(to: file, options: .atomic)
        }
    }
}
#endif
