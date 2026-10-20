import Foundation

enum ReadingMode: String, CaseIterable {
    case scroll, curl, slide, cover, none
    static func migrate(_ defaults: UserDefaults = .standard) {
        if defaults.string(forKey: "preview.readingMode") == nil {
            let hasLegacyMode = defaults.object(forKey: "preview.nativeReader") != nil || defaults.object(forKey: "preview.sliding") != nil
            let mode: ReadingMode = hasLegacyMode
                ? (defaults.bool(forKey: "preview.nativeReader") ? (defaults.bool(forKey: "preview.sliding") ? .slide : .curl) : .scroll)
                : .curl
            defaults.set(mode.rawValue, forKey: "preview.readingMode")
        }
        let previous = defaults.string(forKey: "preview.readingMode") ?? "scroll"
        if defaults.string(forKey: "preview.readingStyle") == nil {
            defaults.set(previous == "scroll" ? "scroll" : "paging", forKey: "preview.readingStyle")
        }
        if defaults.string(forKey: "preview.pageAnimation") == nil {
            defaults.set(previous == "scroll" ? "curl" : previous, forKey: "preview.pageAnimation")
        }
    }
    static func resolve(style: String, animation: String) -> ReadingMode {
        guard style == "paging" else {return .scroll}
        let mode = ReadingMode(rawValue: animation) ?? .curl
        return mode == .scroll ? .curl : mode
    }
    var paginated: Bool { self != .scroll }
    var customTransition: Bool { self == .cover || self == .none }
}

// Both animations use the same spread boundaries and reading coordinates.
enum ReadingSpread {
    static let preferenceKey = "reader.forceSingleColumn"
    static func columns(width: Double, height: Double, margin: Double, fontSize: Double, forceSingleColumn: Bool = false) -> Int {
        !forceSingleColumn && width > height && width / 2 - margin * 2 >= max(280, fontSize * 12) ? 2 : 1
    }
    static func start(_ page: Int, columns: Int) -> Int { max(0, page) / columns * columns }
}

// Latch first-reader eligibility before mode migration creates preference keys.
enum ReadingIntroduction {
    static let storageKey = "reader.introduction.v1"
    static func prepare(_ defaults: UserDefaults = .standard) {
        guard defaults.object(forKey: storageKey) == nil else { return }
        let previousReader = ["preview.readingMode", "preview.readingStyle", "preview.nativeReader"].contains { defaults.object(forKey: $0) != nil }
        defaults.set(previousReader, forKey: storageKey)
    }
}
