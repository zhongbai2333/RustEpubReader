import Foundation

@main struct AppearanceLinkageChecks {
    struct Case: Decodable {
        let name: String; let initial: AppearanceLinkage; let system: Bool; let ambient: Bool?; let actions: [String]; let expected: Expected
        struct Expected: Decodable { let app: Bool; let reading: Bool; let appLinked: Bool; let readingLinked: Bool }
    }
    static func main() throws {
        let first=AppearanceLinkage.firstRun
        precondition(first.source == .ambient && first.readingSource == .ambient && first.appLinked && first.readingLinked)
        precondition(first.readingDark(system:true,ambient:false) == false)
        precondition(first.readingDark(system:true,ambient:nil) == true)
        precondition(AppearanceLinkage.decode(first.encoded) == first)
        let cases = try JSONDecoder().decode([Case].self, from: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1])))
        for item in cases {
            var value=item.initial, system=item.system, ambient=item.ambient
            for action in item.actions {
                let parts=action.split(separator: ":").map(String.init), arg=parts.count>1 ? parts[1] : ""
                switch parts[0] {
                case "source": value.source=AutomaticAppearanceSource(rawValue: arg)!
                case "system": system=arg == "dark"
                case "ambient": ambient=arg == "dark"
                case "appLink": value.toggleAppLink(system: system, ambient: ambient)
                case "readingLink": value.toggleReadingLink(system: system, ambient: ambient)
                case "app": value.setApp(arg == "dark")
                case "reading": value.readingMode=ReadingDisplayMode(rawValue: arg)!
                case "readingSource": value.readingSource=AutomaticAppearanceSource(rawValue: arg)!
                case "shortcut": value.toggleReading(system: system, ambient: ambient)
                default: preconditionFailure(action)
                }
                precondition(AppearanceLinkage.decode(value.encoded) == value)
                value=AppearanceLinkage.decode(value.encoded)!
            }
            precondition(value.appDark(system: system, ambient: ambient) == item.expected.app,item.name)
            precondition(value.readingDark(system: system, ambient: ambient) == item.expected.reading,item.name)
            precondition(value.appLinked == item.expected.appLinked && value.readingLinked == item.expected.readingLinked,item.name)
        }
        precondition(AppearanceLinkage.decode("invalid") == nil)
        precondition(AppearanceLinkage.decode("{}") == AppearanceLinkage())
        precondition(AppearanceLinkage.migrate(reading: .light).readingMode == .light)
        precondition(AppearanceLinkage.migrate(reading: .automatic).readingLinked)
        print("PASS: \(cases.count) shared appearance linkage cases, persistence and migration")
    }
}
