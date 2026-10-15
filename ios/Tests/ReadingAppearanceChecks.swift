import Foundation

@main struct ReadingAppearanceChecks {
    static func main() throws {
        let suite = "appearance-check-\(UUID())", d = UserDefaults(suiteName: suite)!
        defer { d.removePersistentDomain(forName: suite) }
        d.set(23.0, forKey: "preview.fontSize")
        d.set("custom", forKey: "preview.theme"); d.set("151515", forKey: "preview.backgroundHex")
        d.set("EDEDED", forKey: "preview.textHex"); d.set("background-old.png", forKey: "preview.backgroundImageName")
        ReadingAppearance.migrate(d)
        var value = ReadingAppearance.decode(d.string(forKey: ReadingAppearance.storageKey)!)
        precondition(value.mode == .dark && value.selected == "custom")
        precondition(value.style.dark.paper == "151515" && value.style.dark.ink == "EDEDED")
        precondition(value.style.light.paper == ReadingStyle.presets[0].light.paper)
        precondition(value.style.light.image == "background-old.png" && value.style.dark.image == "background-old.png")
        let migrated = value.encoded; ReadingAppearance.migrate(d)
        precondition(d.string(forKey: ReadingAppearance.storageKey) == migrated)
        precondition(d.double(forKey: "preview.fontSize") == 23)
        let oldCustom = value.custom
        let oldJSON = value.encoded.replacingOccurrences(of: "\"overrides\":{},", with: "")
        let old = ReadingAppearance.decode(oldJSON)
        precondition(old.custom == oldCustom && old.overrides.isEmpty && old.selected == "custom")
        precondition(old.choices.map(\.id) == ["paper", "gray", "green", "light", "warm", "custom"])
        precondition(ReadingAppearance().choices.last == ReadingStyle.customTemplate)
        let preset = ReadingStyle.presets[0]
        var draft = preset; draft.light.image = "background-new.png"; draft.dark.image = draft.light.image
        draft.dark.strength = 0.67; draft.light.image = ""
        precondition(draft.dark.image == "background-new.png" && draft.light.strength == 0.18)
        value.save(draft); precondition(value.style.dark.strength == 0.67 && ReadingStyle.presets[0] == preset)
        precondition(value.selected == "paper" && value.custom == oldCustom)
        var gray = ReadingStyle.presets[1]; gray.light.image = "background-old.png"; gray.dark.paper = "121314"
        value.save(gray)
        precondition(value.custom == oldCustom && value.selected == "gray")
        precondition(value.overrides["paper"] == draft && value.overrides["gray"] == gray)
        precondition(value.choices[2] == ReadingStyle.presets[2])
        precondition(value.imageReferences == Set(["background-old.png", "background-new.png"]))
        value.save(preset)
        precondition(value.style == preset && value.overrides["gray"] == gray && value.custom == oldCustom)
        precondition(value.imageReferences.contains("background-old.png"))
        precondition(!value.imageReferences.contains("background-new.png"))
        precondition(ReadingAppearance.decode(value.encoded) == value)
        precondition(ReadingAppearance.decode("{\"selected\":\"unknown\"}").selected == "paper")
        precondition(ReadingAppearance.decode("{\"selected\":\"custom\"}").selected == "paper")
        var invalid = ReadingStyle.presets[2]; invalid.id = "wrong"; invalid.light.paper = "bad"; invalid.dark.strength = 2
        var repaired = ReadingAppearance(overrides: ["green": invalid, "unknown": invalid])
        repaired = ReadingAppearance.decode(repaired.encoded)
        precondition(repaired.overrides.count == 1 && repaired.overrides["green"]?.id == "green")
        precondition(repaired.overrides["green"]?.light.paper == ReadingStyle.presets[2].light.paper)
        precondition(repaired.overrides["green"]?.dark.strength == 1)
        precondition(ReadingDisplayMode.automatic.isDark(systemDark: true))
        precondition(!ReadingDisplayMode.light.isDark(systemDark: true))
        precondition(ReadingDisplayMode.dark.isDark(systemDark: false))
        precondition(ReadingAppearance.decode("invalid") == ReadingAppearance())
        print("PASS: six themes, independent preset/custom slots, old JSON, shared image references, normalization, legacy migration, idempotence, shared typography preservation, independent variants, immutable presets, serialization, display modes")
    }
}
