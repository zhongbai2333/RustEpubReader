import Foundation

enum AutomaticAppearanceSource: String, Codable, CaseIterable {
    case system, ambient
    var labelKey: String { "appearance.link_\(rawValue)" }
    func resolve(system: Bool, ambient: Bool?) -> Bool { self == .ambient ? (ambient ?? system) : system }
}

struct AppearanceLinkage: Codable, Equatable {
    enum CodingKeys: String, CodingKey { case version, source, appLinked, readingLinked, appManual, readingMode, readingSource }
    static let storageKey = "appearance.linkage.v1"
    static var firstRun: Self {
        var value = Self(); value.source = .ambient; value.readingSource = .ambient
        return value
    }
    var version = 1
    var source: AutomaticAppearanceSource = .system
    var appLinked = true
    var readingLinked = true
    var appManual = false
    var readingMode: ReadingDisplayMode = .automatic
    var readingSource: AutomaticAppearanceSource = .system
    func appDark(system: Bool, ambient: Bool?) -> Bool { appLinked ? source.resolve(system: system, ambient: ambient) : appManual }
    func readingDark(system: Bool, ambient: Bool?) -> Bool {
        readingLinked ? appDark(system: system, ambient: ambient) : readingMode.isDark(systemDark: readingSource.resolve(system: system, ambient: ambient))
    }
    var usesAmbient: Bool { (appLinked && source == .ambient) || (!readingLinked && readingMode == .automatic && readingSource == .ambient) }
    mutating func toggleAppLink(system: Bool, ambient: Bool?) {
        if appLinked { appManual = appDark(system: system, ambient: ambient) }
        appLinked.toggle()
    }
    mutating func toggleReadingLink(system: Bool, ambient: Bool?) {
        if readingLinked { readingMode = readingDark(system: system, ambient: ambient) ? .dark : .light }
        readingLinked.toggle()
    }
    mutating func setApp(_ dark: Bool) { appManual = dark; appLinked = false }
    mutating func toggleReading(system: Bool, ambient: Bool?) {
        let dark = !readingDark(system: system, ambient: ambient)
        if readingLinked { setApp(dark) } else { readingMode = dark ? .dark : .light }
    }
    var encoded: String { let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]; return (try? String(data: encoder.encode(self), encoding: .utf8)) ?? "" }
    static func decode(_ raw: String?) -> Self? {
        guard let raw, let value = try? JSONDecoder().decode(Self.self, from: Data(raw.utf8)), value.version == 1 else { return nil }; return value
    }
    static func migrate(reading: ReadingDisplayMode) -> Self {
        var value = Self()
        value.readingLinked = reading == .automatic
        value.readingMode = reading
        return value
    }
}

extension AppearanceLinkage {
    init(from decoder: Decoder) throws {
        self.init()
        let values=try decoder.container(keyedBy:CodingKeys.self)
        version=try values.decodeIfPresent(Int.self,forKey:.version) ?? 1
        source=try values.decodeIfPresent(AutomaticAppearanceSource.self,forKey:.source) ?? .system
        appLinked=try values.decodeIfPresent(Bool.self,forKey:.appLinked) ?? true
        readingLinked=try values.decodeIfPresent(Bool.self,forKey:.readingLinked) ?? true
        appManual=try values.decodeIfPresent(Bool.self,forKey:.appManual) ?? false
        readingMode=try values.decodeIfPresent(ReadingDisplayMode.self,forKey:.readingMode) ?? .automatic
        readingSource=try values.decodeIfPresent(AutomaticAppearanceSource.self,forKey:.readingSource) ?? .system
    }
}
