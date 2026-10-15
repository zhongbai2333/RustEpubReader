import Foundation
@main struct I18nChecks {
    static func main() throws {
        let bundle=Bundle(path:CommandLine.arguments[1])!
        let suite="i18n-check-"+UUID().uuidString,defaults=UserDefaults(suiteName:suite)!
        defer {defaults.removePersistentDomain(forName:suite)}
        let i=ReaderI18n(defaults:defaults,bundle:bundle)
        i.setLanguage("auto");i.refresh(system:"zh-Hant-HK")
        precondition(i.preference=="auto" && i.resolvedCode=="zh_cn")
        precondition(i.text("common.cancel")=="取消")
        let old=i.text("tts.playing")
        i.setLanguage("en");i.refresh(system:"zh-CN")
        precondition(i.preference=="en" && i.resolvedCode=="en")
        precondition(i.message(old)==i.text("tts.playing"))
        precondition(defaults.string(forKey:"reader.language")=="en")
        precondition(i.count("ui.library_count",1)=="1 book")
        precondition(i.count("ui.library_count",2)=="2 books")
        precondition(i.text("missing")=="Unavailable")
        precondition(ReaderI18n.resolve("auto",system:"fr-FR")=="en")
        precondition(ReaderI18n.normalize(" EN_us ")=="en")
        precondition(ReaderI18n.format("{} / {}",args:["literal {}","second"])=="literal {} / second")
        precondition(ReaderI18n.format("{1} / {0}",args:["😀","{1}"])=="{1} / 😀")
        precondition(ReaderI18n.format("{{{name}}}",args:[],named:["name":"{}"])=="{{}}")
        precondition(ReaderI18n.format("{4}",args:["x"])=="{4}")
        print("PASS: bundled catalogs, language switching/persistence, system fallback, stored status, plural counts, brace-safe formatting")
    }
}
