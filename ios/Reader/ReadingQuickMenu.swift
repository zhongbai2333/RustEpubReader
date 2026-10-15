import UIKit

// Let UIKit own the glass source, menu/submenu transitions, dismissal, focus,
// and adaptation on each OS. No custom presentation controller or spring copy.
final class ReaderQuickMenuCoordinator: NSObject {
    var parent: ReadingToolsMenu
    init(_ parent: ReadingToolsMenu) { self.parent = parent }

    func menu(for button: ReaderMenuButton) -> UIMenu {
        func action(_ title: String, _ symbol: String, _ perform: @escaping (ReadingToolsMenu) -> Void) -> UIAction {
            UIAction(title: title, image: UIImage(systemName: symbol), attributes: [.keepsMenuPresented]) { [weak self, weak button] _ in
                // Handle selection before UIKit's automatic dismissal defers
                // the action. Start both native transitions in the same turn.
                guard let self else { return }
                button?.contextMenuInteraction?.dismissMenu()
                perform(self.parent)
            }
        }
        let primary = UIMenu(title: "", options: [.displayInline], children: [
            action(L("toc.title"), "list.bullet") { $0.contents() },
            action(L("toolbar.tts"), "speaker.wave.2") { $0.speech() }
        ])
        primary.preferredElementSize = .medium
        let more = UIMenu(title: L("reader.more_actions"), image: UIImage(systemName: "ellipsis"), children: [
            action(L("reader.search_book"), "magnifyingglass") { $0.search() },
            action(L("toolbar.stats"), "chart.bar") { $0.statistics() },
            action(L("reader.correction"), "text.badge.checkmark") { $0.correction() }
        ])
        return UIMenu(children: [
            primary,
            action(L("annotations.title"), "bookmark") { $0.annotations() },
            action(parent.night ? L("toolbar.light_mode") : L("toolbar.dark_mode"), parent.night ? "sun.max" : "moon") { $0.dayNight() },
            action(L("settings.appearance_title"), "textformat.size") { $0.settings() },
            more
        ])
    }
}
