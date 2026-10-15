import Foundation

enum ReadingDisplayMode: String, Codable, CaseIterable {
    case light, dark, automatic
    var labelKey: String { "appearance.mode_\(rawValue)" }
    func isDark(systemDark: Bool) -> Bool { self == .dark || (self == .automatic && systemDark) }
}

struct ReadingAppearanceVariant: Codable, Equatable {
    var paper: String
    var ink: String
    var image = ""
    var strength = 0.18
}

struct ReadingStyle: Codable, Equatable, Identifiable {
    var id: String
    var light: ReadingAppearanceVariant
    var dark: ReadingAppearanceVariant
    var labelKey: String { "appearance.style_\(id)" }
    subscript(dark isDark: Bool) -> ReadingAppearanceVariant {
        get { isDark ? dark : light }
        set { if isDark { dark = newValue } else { light = newValue } }
    }
    static let customTemplate = ReadingStyle(id: "custom",
        light: .init(paper: "FFFFFF", ink: "252525"),
        dark: .init(paper: "1A1A1C", ink: "DCDCDC", strength: 0.12))
    static let presets: [ReadingStyle] = [
        .init(id: "paper", light: .init(paper: "F7F2E8", ink: "252525"), dark: .init(paper: "1A1A1C", ink: "DCDCDC", strength: 0.12)),
        .init(id: "gray", light: .init(paper: "E4E4E2", ink: "252525"), dark: .init(paper: "24262B", ink: "E6E6E6", strength: 0.12)),
        .init(id: "green", light: .init(paper: "E2EADC", ink: "252525"), dark: .init(paper: "1D2823", ink: "DCE6DE", strength: 0.12)),
        .init(id: "light", light: .init(paper: "FFFFFF", ink: "252525"), dark: .init(paper: "171719", ink: "DCDCDC", strength: 0.12)),
        .init(id: "warm", light: .init(paper: "EEDDC4", ink: "252525"), dark: .init(paper: "29241E", ink: "EADFCC", strength: 0.12))
    ]
}

struct ReadingAppearance: Codable, Equatable {
    static let storageKey = "reader.appearance.v1"
    var mode: ReadingDisplayMode = .light
    var selected = "paper"
    var custom: ReadingStyle?
    var overrides: [String: ReadingStyle] = [:]
    var choices: [ReadingStyle] { ReadingStyle.presets.map { overrides[$0.id] ?? $0 } + [custom ?? .customTemplate] }
    var style: ReadingStyle { choices.first { $0.id == selected } ?? choices[0] }
    var imageReferences: Set<String> {
        Set((Array(overrides.values) + (custom.map { [$0] } ?? [])).flatMap { [$0.light.image, $0.dark.image] }.filter { !$0.isEmpty })
    }
    mutating func save(_ draft: ReadingStyle) {
        if ReadingStyle.presets.contains(where: { $0.id == draft.id }) {
            overrides[draft.id] = draft; selected = draft.id
        } else { custom = draft; custom?.id = "custom"; selected = "custom" }
        self = normalized()
    }
    var encoded: String {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
        return (try? String(data: encoder.encode(self), encoding: .utf8)) ?? ""
    }
    static func decode(_ raw: String) -> ReadingAppearance {
        guard let value = try? JSONDecoder().decode(Self.self, from: Data(raw.utf8)) else { return .init() }
        return value.normalized()
    }
    private func normalized() -> Self {
        var value = self
        func normalized(_ style: ReadingStyle, template: ReadingStyle) -> ReadingStyle {
            var result = style; result.id = template.id
            for dark in [false, true] {
                var variant = result[dark: dark]
                if variant.paper.count != 6 || !variant.paper.allSatisfy(\.isHexDigit) { variant.paper = template[dark: dark].paper }
                if variant.ink.count != 6 || !variant.ink.allSatisfy(\.isHexDigit) { variant.ink = template[dark: dark].ink }
                variant.strength = variant.strength.isFinite ? min(1, max(0, variant.strength)) : template[dark: dark].strength
                result[dark: dark] = variant
            }
            return result
        }
        value.custom = value.custom.map { normalized($0, template: .customTemplate) }
        value.overrides = Dictionary(uniqueKeysWithValues: ReadingStyle.presets.compactMap { template in
            value.overrides[template.id].map { (template.id, normalized($0, template: template)) }
        })
        if !ReadingStyle.presets.contains(where: { $0.id == value.selected }) && !(value.selected == "custom" && value.custom != nil) { value.selected = "paper" }
        return value
    }
    // Upgrade only appearance keys. Typography, reading anchors and legacy files remain untouched.
    static func migrate(_ defaults: UserDefaults = .standard) {
        if let raw = defaults.string(forKey: storageKey), (try? JSONDecoder().decode(Self.self, from: Data(raw.utf8))) != nil { return }
        let theme = defaults.string(forKey: "preview.theme") ?? "paper"
        let dark = theme == "dark"
        var value = Self(mode: dark ? .dark : .light, selected: dark ? "paper" : theme)
        let image = defaults.bool(forKey: "preview.backgroundImageEnabled") || defaults.object(forKey: "preview.backgroundImageEnabled") == nil
            ? defaults.string(forKey: "preview.backgroundImageName") ?? "" : ""
        if theme == "custom" || !image.isEmpty {
            var style = ReadingStyle.presets.first { $0.id == value.selected } ?? ReadingStyle.presets[0]
            if theme == "custom" {
                let paper = defaults.string(forKey: "preview.backgroundHex") ?? "F7F2E8"
                let ink = defaults.string(forKey: "preview.textHex") ?? "252525"
                let rgb = UInt32(paper, radix: 16) ?? 0xF7F2E8
                let luma = Double((rgb >> 16) & 255) * 0.299 + Double((rgb >> 8) & 255) * 0.587 + Double(rgb & 255) * 0.114
                let isDark = luma < 255 * 0.45
                value.mode = isDark ? .dark : .light
                style[dark: isDark] = .init(paper: paper, ink: ink)
            }
            for side in [false, true] {
                style[dark: side].image = image
                if defaults.object(forKey: "preview.backgroundImageOpacity") != nil { style[dark: side].strength = defaults.double(forKey: "preview.backgroundImageOpacity") }
            }
            style.id = "custom"
            value.save(style)
        }
        defaults.set(decode(value.encoded).encoded, forKey: storageKey)
    }
}

// Missing override data in existing installations must decode as an empty map.
extension ReadingAppearance {
    enum CodingKeys: String, CodingKey { case mode, selected, custom, overrides }
    init(from decoder: Decoder) throws {
        self.init()
        let values = try decoder.container(keyedBy: CodingKeys.self)
        mode = try values.decodeIfPresent(ReadingDisplayMode.self, forKey: .mode) ?? .light
        selected = try values.decodeIfPresent(String.self, forKey: .selected) ?? "paper"
        custom = try values.decodeIfPresent(ReadingStyle.self, forKey: .custom)
        overrides = try values.decodeIfPresent([String: ReadingStyle].self, forKey: .overrides) ?? [:]
    }
}


// Opaque quick picks shared visually with desktop and Android.
enum ReadingColorPalette {
    static let paper = ["F7F2E8", "FFFFFF", "F1F3F5", "E2EADC", "EEDDC4", "E7EFF7", "F3E6EA", "E8E2F0", "1A1A1C", "24262B", "1D2823", "29241E"]
    static let ink = ["1A1A1A", "2D2D2D", "5B4636", "314C3B", "284B63", "604A70", "7C3E45", "7A5726", "DCDCDC", "E6E6E6", "F1EAD8", "FFFFFF"]
}
